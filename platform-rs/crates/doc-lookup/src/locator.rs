//! Which repositories belong to a language, and what they are called on disk
//! (`repos_to_clone`, `get_last_segment`, `normalize_last_segment`, `update_repo_components`,
//! `add_data_not_supplied_by_data_api`, `filter_excluded_resource_types`).

use std::collections::HashSet;

use doc_contract::{Finding, FindingCode, Mode, Severity};
use url::Url;

use crate::catalog::{Language, RepoEntry};
use crate::tables_gen::{
    BOOK_NAMES, LANG_SPECIFIC_PREFIXES_TO_REMOVE, NON_USFM_RESOURCE_TYPES, PREFIXES_TO_REMOVE,
    REPLACEMENTS_BY_LANG_AND_SEGMENT, RESOURCE_TYPE_CODES_AND_NAMES, USFM_RESOURCE_TYPES,
};

/// A repository of the requested language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    pub url: String,
    /// The last URL segment after DOC's anomaly handling; DOC's directory name under the assets dir.
    pub segment: String,
    pub resource_type: String,
    /// `segment.split("_")`, with the owner prefix removed in `Fixed` mode.
    pub components: Vec<String>,
    pub language: Language,
}

/// The Reviewer's Guide directory name DOC returns for an English `.docx` URL (dead today: no
/// catalog URL ends in `.docx`, RAN on the recorded catalog).
pub const EN_RG_SEGMENT: &str = "en_rg";

/// `normalize_last_segment`.
pub fn normalize_last_segment(lang: &str, last_segment: &str) -> String {
    if lang == "en" && last_segment.ends_with(".docx") {
        return EN_RG_SEGMENT.to_string();
    }
    if let Some((_, _, repl)) = REPLACEMENTS_BY_LANG_AND_SEGMENT.iter().find(|(l, s, _)| *l == lang && *s == last_segment) {
        return (*repl).to_string();
    }
    for prefix in PREFIXES_TO_REMOVE {
        if let Some(rest) = last_segment.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    for (l, prefixes) in LANG_SPECIFIC_PREFIXES_TO_REMOVE {
        if *l == lang {
            for prefix in *prefixes {
                if let Some(rest) = last_segment.strip_prefix(prefix) {
                    return rest.to_string();
                }
            }
        }
    }
    last_segment.to_string()
}

/// `get_last_segment(url, lang)`: `urlparse(url).path.strip("/").split("/")[-1]`, normalised.
pub fn get_last_segment(url: &str, lang: &str) -> String {
    let path = Url::parse(url).map(|u| u.path().to_string()).unwrap_or_default();
    let last = path.trim_matches('/').split('/').next_back().unwrap_or("");
    normalize_last_segment(lang, last)
}

/// `update_repo_components`: strip an owner prefix (a leading run of components before the
/// language code) and a trailing non-type component.
pub fn update_repo_components(components: &[String]) -> Vec<String> {
    let mut comps: Vec<String> = components.to_vec();
    if let Some(last) = comps.last() {
        let known = USFM_RESOURCE_TYPES.contains(&last.as_str())
            || NON_USFM_RESOURCE_TYPES.contains(&last.as_str())
            || RESOURCE_TYPE_CODES_AND_NAMES.iter().any(|(c, _)| c == last);
        if !known {
            comps.pop();
        }
    }
    match comps.len() {
        7 => comps.split_off(3),
        6 => comps.split_off(2),
        5 => comps.split_off(1),
        3 if comps[0] == "en" && comps[1] == "condensed" => vec!["en".to_string(), "tn_condensed".to_string()],
        _ => comps,
    }
}

/// The six repositories DOC adds when the data API does not list that (language, type) pair
/// (`add_data_not_supplied_by_data_api`). Returns only the entries that would be appended.
pub fn augmentation(existing: &[RepoEntry]) -> Vec<RepoEntry> {
    let mk = |url: &str, rt: &str, en: &str, code: &str, nat: &str| RepoEntry {
        repo_url: url.to_string(),
        resource_type: rt.to_string(),
        language: Language { english_name: en.into(), ietf_code: code.into(), national_name: nat.into(), direction: "ltr".into() },
    };
    let extras = [
        mk("https://content.bibletranslationtools.org/WA-Catalog/bg_bpb", "bpb", "Bulgarian", "bg", "български език"),
        mk("https://content.bibletranslationtools.org/WycliffeAssociates/en_tn_condensed", "tn-condensed", "English", "en", "English"),
        mk("https://content.bibletranslationtools.org/WA-Catalog/id_tbi", "tbi", "Indonesian", "id", "Bahasa Indonesian"),
        mk("https://content.bibletranslationtools.org/WA-Catalog/id_ayt", "ayt", "Indonesian", "id", "Bahasa Indonesian"),
        mk("https://content.bibletranslationtools.org/WA-Catalog/id_tq", "tq", "Indonesian", "id", "Bahasa Indonesian"),
        mk("https://content.bibletranslationtools.org/WA-Catalog/id_tw", "tw", "Indonesian", "id", "Bahasa Indonesian"),
    ];
    let mut pairs: HashSet<(String, String)> =
        existing.iter().map(|e| (e.language.ietf_code.clone(), e.resource_type.clone())).collect();
    let mut out = Vec::new();
    for e in extras {
        let key = (e.language.ietf_code.clone(), e.resource_type.clone());
        if pairs.insert(key) {
            out.push(e);
        }
    }
    out
}

/// `augmented = existing + augmentation(existing)`, without mutating `existing`.
pub fn augmented(existing: &[RepoEntry]) -> Vec<RepoEntry> {
    let mut all = existing.to_vec();
    all.extend(augmentation(existing));
    all
}

/// `filter_excluded_resource_types` with DOC's default exclusion `("en", "udb")`.
pub fn filter_excluded(repos: Vec<RepoEntry>) -> Vec<RepoEntry> {
    repos.into_iter().filter(|e| !(e.language.ietf_code == "en" && e.resource_type == "udb")).collect()
}

/// `repos_to_clone(lang, repos)`: the repositories of `lang` whose type DOC knows, de-duplicated
/// by URL, in catalog order. In `Fixed` mode the owner prefix is removed from the components
/// (what the document path already does) and reported.
pub fn repos_to_clone(lang: &str, repos: &[RepoEntry], mode: Mode) -> (Vec<RepoRef>, Vec<Finding>) {
    let mut out: Vec<RepoRef> = Vec::new();
    let mut findings = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for repo in repos {
        if repo.language.ietf_code != lang || !RESOURCE_TYPE_CODES_AND_NAMES.iter().any(|(c, _)| *c == repo.resource_type) {
            continue;
        }
        if !seen.insert(repo.repo_url.as_str()) {
            continue;
        }
        let segment = get_last_segment(&repo.repo_url, lang);
        let raw: Vec<String> = segment.split('_').map(str::to_string).collect();
        let components = match mode {
            Mode::AsBuilt => raw,
            Mode::Fixed => {
                let mut updated = update_repo_components(&raw);
                // DOC's prefix removal counts components, and miscounts when the last one is not a
                // known resource type (`erwinmast_nan-x-teochi_col_text_udb`). The language code is
                // the anchor: the book code is the component after it.
                let book_ok = updated.get(1).is_some_and(|c| BOOK_NAMES.iter().any(|(b, _)| b == c));
                if updated.len() > 2 && !book_ok {
                    if let Some(i) = raw.iter().position(|c| c == lang).filter(|&i| i > 0) {
                        if raw.get(i + 1).is_some_and(|c| BOOK_NAMES.iter().any(|(b, _)| b == c)) {
                            updated = raw[i..].to_vec();
                        }
                    }
                }
                if updated != raw {
                    findings.push(
                        Finding::new(
                            FindingCode::OwnerPrefixStripped,
                            Severity::Info,
                            format!("components {raw:?} read as {updated:?}"),
                        )
                        .lang(lang)
                        .repo(&repo.repo_url),
                    );
                }
                updated
            }
        };
        out.push(RepoRef {
            url: repo.repo_url.clone(),
            segment,
            resource_type: repo.resource_type.clone(),
            components,
            language: repo.language.clone(),
        });
    }
    (out, findings)
}

/// DOC's `resource_lookup_dto` (`resource_lookup.py:1059`) over the augmented primary catalog rows, in catalog order: which repository
/// holds `resource_type` of `book` for `lang`. Three tiers, the first that has any candidate wins: a `.docx` last segment (the English
/// Reviewer's Guide, dead today: no catalog URL ends so), then repositories whose name has more than two components and names this book
/// (`bys_eph_text_reg`, one repository per book), then two-component names (`sw_ulb`, one repository per language and type). A resource
/// type DOC has no name for raises `KeyError` inside DOC's try block, which stops the loop with what it had. `None` is "DOC returns None":
/// the request is left out of the document.
pub fn resource_lookup(lang: &str, resource_type: &str, book: &str, rows: &[RepoEntry]) -> Option<RepoRef> {
    const ZMQ_USER: &str = "faustin_azaza";
    let known = |t: &str| RESOURCE_TYPE_CODES_AND_NAMES.iter().any(|(c, _)| *c == t);
    let make = |entry: &RepoEntry, segment: &str, components: Vec<String>| RepoRef {
        url: entry.repo_url.clone(),
        segment: segment.to_string(),
        resource_type: resource_type.to_string(),
        components,
        language: entry.language.clone(),
    };
    let (mut rg, mut more, mut two): (Option<RepoRef>, Option<RepoRef>, Option<RepoRef>) = (None, None, None);
    for entry in rows {
        if entry.language.ietf_code != lang {
            continue;
        }
        let last = get_last_segment(&entry.repo_url, lang);
        if last.ends_with("docx") {
            if !known(resource_type) {
                break;
            }
            rg.get_or_insert_with(|| make(entry, &last, Vec::new()));
            continue;
        }
        let comps = update_repo_components(&last.split('_').map(str::to_string).collect::<Vec<_>>());
        if comps.len() > 2 {
            let book_ = comps.get(1).map(String::as_str).unwrap_or("");
            if (entry.repo_url.contains(book_) || entry.repo_url.contains(ZMQ_USER)) && resource_type == entry.resource_type && known(&entry.resource_type) && book_ == book {
                if !known(resource_type) {
                    break;
                }
                more.get_or_insert_with(|| make(entry, &last, comps));
            }
        } else if comps.len() == 2 && resource_type == entry.resource_type {
            if !known(resource_type) {
                break;
            }
            two.get_or_insert_with(|| make(entry, &last, comps));
        }
    }
    rg.or(more).or(two)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn segments() {
        assert_eq!(get_last_segment("https://content.bibletranslationtools.org/WA-Catalog/sw_ulb", "sw"), "sw_ulb");
        assert_eq!(get_last_segment("https://h/o/Jordan_x_y", "xx"), "x_y");
        assert_eq!(get_last_segment("https://h/o/chindali_ndh_mat_text_reg", "ndh"), "ndh_mat_text_reg");
        assert_eq!(get_last_segment("https://h/DCS-Mirror/fa_opv", "fa"), "fa_ulb");
    }

    #[test]
    fn owner_prefix() {
        assert_eq!(update_repo_components(&s(&["faustin-azaza", "rw", "1co", "text", "reg"])), s(&["rw", "1co", "text", "reg"]));
        assert_eq!(update_repo_components(&s(&["danjuma", "alfred", "h", "kgo", "phm", "text", "ulb", "l1"])), s(&["kgo", "phm", "text", "ulb"]));
        assert_eq!(update_repo_components(&s(&["en", "condensed", "tn"])), s(&["en", "tn_condensed"]));
        assert_eq!(update_repo_components(&s(&["ln", "mat", "text", "reg"])), s(&["ln", "mat", "text", "reg"]));
    }

    #[test]
    fn fixed_reports_the_stripped_owner_prefix() {
        let entry = RepoEntry {
            repo_url: "https://h/DCS-Mirror/faustin-azaza_rw_1co_text_reg".into(),
            resource_type: "reg".into(),
            language: Language { english_name: "K".into(), ietf_code: "rw".into(), national_name: "K".into(), direction: "ltr".into() },
        };
        let (built, f0) = repos_to_clone("rw", std::slice::from_ref(&entry), Mode::AsBuilt);
        assert_eq!(built[0].components[1], "rw");
        assert!(f0.is_empty());
        let (fixed, f1) = repos_to_clone("rw", std::slice::from_ref(&entry), Mode::Fixed);
        assert_eq!(fixed[0].components[1], "1co");
        assert_eq!(f1[0].code, FindingCode::OwnerPrefixStripped);
    }

    #[test]
    fn fixed_anchors_on_the_language_code_when_the_count_rule_fails() {
        let entry = RepoEntry {
            repo_url: "https://h/DCS-Mirror/erwinmast_nan-x-teochi_col_text_udb".into(),
            resource_type: "reg".into(),
            language: Language { english_name: "T".into(), ietf_code: "nan-x-teochi".into(), national_name: "T".into(), direction: "ltr".into() },
        };
        let (fixed, _) = repos_to_clone("nan-x-teochi", std::slice::from_ref(&entry), Mode::Fixed);
        assert_eq!(fixed[0].components[1], "col");
        let (built, _) = repos_to_clone("nan-x-teochi", std::slice::from_ref(&entry), Mode::AsBuilt);
        assert_eq!(built[0].components[1], "nan-x-teochi");
    }

    fn entry(url: &str, rtype: &str, lang: &str) -> RepoEntry {
        RepoEntry {
            repo_url: url.into(),
            resource_type: rtype.into(),
            language: Language { english_name: "L".into(), ietf_code: lang.into(), national_name: "N".into(), direction: "ltr".into() },
        }
    }

    /// DOC's three tiers: a per-book repository (`bys_eph_text_reg`) wins over a language-wide one, and a book it does not name is not found.
    #[test]
    fn lookup_tiers() {
        let rows = vec![
            entry("https://c.org/WA-Catalog/sw_ulb", "ulb", "sw"),
            entry("https://c.org/Bayan/bys_eph_text_reg", "reg", "bys"),
            entry("https://c.org/Bayan/bys_gal_text_reg", "reg", "bys"),
        ];
        assert_eq!(resource_lookup("sw", "ulb", "jud", &rows).map(|r| r.segment), Some("sw_ulb".to_string()));
        assert_eq!(resource_lookup("bys", "reg", "eph", &rows).map(|r| r.segment), Some("bys_eph_text_reg".to_string()));
        assert_eq!(resource_lookup("bys", "reg", "gal", &rows).map(|r| r.segment), Some("bys_gal_text_reg".to_string()));
        assert!(resource_lookup("bys", "reg", "mat", &rows).is_none());
        assert!(resource_lookup("sw", "tq", "jud", &rows).is_none());
        assert!(resource_lookup("fa", "ulb", "eph", &rows).is_none());
    }

    #[test]
    fn augmentation_only_missing_pairs() {
        let extras = augmentation(&[]);
        assert_eq!(extras.len(), 6);
    }
}
