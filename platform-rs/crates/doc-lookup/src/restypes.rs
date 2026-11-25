//! `resource_types(lang, book_codes)` (`resource_lookup.py:~330-415`): which resource types a
//! language offers for the requested books, sorted by display name.

use std::collections::HashSet;

use doc_contract::wire::ResourceTypeEntry;
use doc_contract::Mode;

use crate::booklist::{find_usfm_files, is_usfm_type};
use crate::locator::RepoRef;
use crate::pytext::is_py_space;
use crate::reader::Workspace;
use crate::tables_gen::{BOOK_NAMES, RESOURCE_TYPE_CODES_AND_NAMES};
use crate::{Abort, Outcome};

/// `book_codes_str.split(",")`, with `all` as the first code meaning every book.
pub fn split_book_codes(book_codes_str: &str) -> Vec<String> {
    let codes: Vec<String> = book_codes_str.split(',').map(str::to_string).collect();
    if codes.first().is_some_and(|c| c == "all") {
        return BOOK_NAMES.iter().map(|(c, _)| (*c).to_string()).collect();
    }
    codes
}

/// `^\d{2,}-[0-9a-z]{3}$` (`re.search`; `\d` is any Unicode decimal digit).
fn is_bc_asset_dir(name: &str) -> bool {
    let name = name.strip_suffix('\n').unwrap_or(name);
    let Some((digits, tail)) = name.split_once('-') else {
        return false;
    };
    let tail_ok = tail.chars().count() == 3 && tail.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase());
    digits.chars().count() >= 2 && digits.chars().all(|c| c.is_numeric() && !is_py_space(c)) && tail_ok
}

fn type_name(code: &str) -> &'static str {
    RESOURCE_TYPE_CODES_AND_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("")
}

/// `get_resource_types(repo_clone_list, book_codes)`.
pub fn get_resource_types(repos: &[RepoRef], book_codes: &[String], ws: &Workspace, mode: Mode) -> Result<Outcome<Vec<ResourceTypeEntry>>, Abort> {
    let mut out: Vec<ResourceTypeEntry> = Vec::new();
    for repo in repos {
        let rt = repo.resource_type.as_str();
        if rt.is_empty() {
            continue;
        }
        let mut have_assets = false;
        if matches!(rt, "tq" | "tn" | "tn-condensed") || rt == "bc" {
            match ws.scandir(&repo.segment) {
                Some(entries) => {
                    have_assets = entries.iter().any(|(name, is_dir)| {
                        if !*is_dir || name.starts_with('.') {
                            return false;
                        }
                        if rt == "bc" {
                            is_bc_asset_dir(name) && name.split('-').nth(1).is_some_and(|b| book_codes.iter().any(|c| *c == b.to_lowercase()))
                        } else {
                            book_codes.iter().any(|c| *c == name.to_lowercase())
                        }
                    });
                }
                None => match mode {
                    Mode::AsBuilt => return Err(Abort(format!("scandir {}: FileNotFoundError", repo.segment))),
                    Mode::Fixed => continue, // not fetched: reported by the acquirer
                },
            }
        } else if is_usfm_type(rt) {
            have_assets = !find_usfm_files(ws, &repo.segment).is_empty();
        }
        if have_assets || rt == "tw" {
            out.push(ResourceTypeEntry(rt.to_string(), type_name(rt).to_string()));
        }
    }
    // unique_tuples (first kept) then a stable sort by display name
    let mut seen: HashSet<(String, String)> = HashSet::new();
    out.retain(|e| seen.insert((e.0.clone(), e.1.clone())));
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(Outcome { value: out, findings: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{MemRepo, RepoReader};
    use std::sync::Arc;

    fn repo(segment: &str, rt: &str) -> RepoRef {
        RepoRef {
            url: format!("https://h/o/{segment}"),
            segment: segment.into(),
            resource_type: rt.into(),
            components: segment.split('_').map(str::to_string).collect(),
            language: crate::catalog::Language { english_name: "e".into(), ietf_code: "xx".into(), national_name: "n".into(), direction: "ltr".into() },
        }
    }

    #[test]
    fn types_sorted_by_name() {
        let tn: Arc<dyn RepoReader> = Arc::new(MemRepo::new().with("mat/01.md", "x"));
        let ulb: Arc<dyn RepoReader> = Arc::new(MemRepo::new().with("41-MAT.usfm", "x"));
        let tw: Arc<dyn RepoReader> = Arc::new(MemRepo::new());
        let ws = Workspace::new(vec![("xx_tn".into(), tn), ("xx_ulb".into(), ulb), ("xx_tw".into(), tw)]);
        let repos = vec![repo("xx_tn", "tn"), repo("xx_ulb", "ulb"), repo("xx_tw", "tw")];
        let out = get_resource_types(&repos, &["mat".to_string()], &ws, Mode::AsBuilt).unwrap();
        let names: Vec<&str> = out.value.iter().map(|e| e.1.as_str()).collect();
        assert_eq!(names, vec!["Translation Notes", "Translation Words", "Unlocked Literal Bible"]);
        assert!(is_bc_asset_dir("01-mat") && !is_bc_asset_dir("1-mat") && !is_bc_asset_dir("01-MAT"));
    }
}
