//! `lang_codes_and_names` (`resource_lookup.py:~196-265`): `[(code, "national (english)", is_gateway)]`.

use std::collections::HashSet;

use doc_contract::wire::LanguageEntry;
use doc_contract::Mode;

use crate::catalog::RepoEntry;

fn entry(repo: &RepoEntry, gateway: &HashSet<&str>) -> LanguageEntry {
    let l = &repo.language;
    let is_gateway = gateway.contains(l.ietf_code.as_str());
    // `if english_name in localized_name` is a substring test; an empty English name is always "in"
    if l.national_name.contains(l.english_name.as_str()) {
        LanguageEntry(l.ietf_code.clone(), l.national_name.clone(), is_gateway)
    } else {
        LanguageEntry(l.ietf_code.clone(), format!("{} ({})", l.national_name, l.english_name), is_gateway)
    }
}

/// The language list.
///
/// `AsBuilt`: an empty primary catalog gives `[]`, and so does an empty non-primary catalog even
/// when the primary one is full (DOC's second `return []`, so one data-API hiccup on the second
/// query empties the whole list). `Fixed`: each catalog contributes what it has.
/// Order: DOC's `unique_tuples` (first occurrence kept) then a stable sort by the display name.
pub fn lang_codes_and_names(
    primary: &[RepoEntry],
    non_primary: &[RepoEntry],
    gateway: &[&str],
    mode: Mode,
) -> Vec<LanguageEntry> {
    let gateway: HashSet<&str> = gateway.iter().copied().collect();
    if mode == Mode::AsBuilt && (primary.is_empty() || non_primary.is_empty()) {
        return Vec::new();
    }
    let mut seen: HashSet<(String, String, bool)> = HashSet::new();
    let mut values: Vec<LanguageEntry> = Vec::new();
    for repo in primary.iter().chain(non_primary.iter()) {
        let e = entry(repo, &gateway);
        if seen.insert((e.0.clone(), e.1.clone(), e.2)) {
            values.push(e);
        }
    }
    values.sort_by(|a, b| a.1.cmp(&b.1));
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Language;

    fn repo(code: &str, en: &str, nat: &str) -> RepoEntry {
        RepoEntry {
            repo_url: format!("https://h/o/{code}_ulb"),
            resource_type: "ulb".into(),
            language: Language { english_name: en.into(), ietf_code: code.into(), national_name: nat.into(), direction: "ltr".into() },
        }
    }

    #[test]
    fn display_name_and_empty_second_catalog() {
        let p = vec![repo("sw", "Swahili", "Kiswahili"), repo("en", "English", "English"), repo("sw", "Swahili", "Kiswahili")];
        let np = vec![repo("bg", "Bulgarian", "български език")];
        let got = lang_codes_and_names(&p, &np, &["en", "sw"], Mode::AsBuilt);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], LanguageEntry("en".into(), "English".into(), true));
        assert_eq!(got[1].1, "Kiswahili (Swahili)");
        assert!(lang_codes_and_names(&p, &[], &["en"], Mode::AsBuilt).is_empty());
        assert_eq!(lang_codes_and_names(&p, &[], &["en"], Mode::Fixed).len(), 2);
    }
}
