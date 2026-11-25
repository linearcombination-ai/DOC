//! The locator: which repositories hold a language's resources and what each is called on disk.
//! DOC `url_utils.get_last_segment`/`normalize_last_segment`, `resource_lookup.repos_to_clone` and
//! `update_repo_components`.

use serde::Serialize;

use crate::catalog::Repo;
use crate::tables_gen::{
    LANG_SPECIFIC_PREFIXES_TO_REMOVE, NON_USFM_RESOURCE_TYPES, PREFIXES_TO_REMOVE, REPLACEMENTS_BY_LANG_AND_LAST_SEGMENT,
    RESOURCE_TYPE_CODES_AND_NAMES, USFM_RESOURCE_TYPES,
};

/// `urlparse(url).path.strip("/").split("/")[-1]` for the http(s) URLs the catalog holds.
fn raw_last_segment(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let path = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => "",
    };
    // urlparse separates `?query` and `#fragment` from the path
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.trim_matches('/').rsplit('/').next().unwrap_or("")
}

/// `normalize_last_segment`. `en_rg_dir` is `passages.domain.survey_files.EN_RG_DIR`.
pub fn normalize_last_segment(lang: &str, last: &str, en_rg_dir: &str) -> String {
    if lang == "en" && last.ends_with(".docx") {
        return en_rg_dir.to_string();
    }
    for ((l, seg), replacement) in REPLACEMENTS_BY_LANG_AND_LAST_SEGMENT {
        if *l == lang && *seg == last {
            return (*replacement).to_string();
        }
    }
    for prefix in PREFIXES_TO_REMOVE {
        if let Some(rest) = last.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    for (l, prefixes) in LANG_SPECIFIC_PREFIXES_TO_REMOVE {
        if *l == lang {
            for prefix in *prefixes {
                if let Some(rest) = last.strip_prefix(prefix) {
                    return rest.to_string();
                }
            }
        }
    }
    last.to_string()
}

pub fn get_last_segment(url: &str, lang: &str, en_rg_dir: &str) -> String {
    normalize_last_segment(lang, raw_last_segment(url), en_rg_dir)
}

/// `update_repo_components` (DOC applies it in the document path only; `fixed` also uses it
/// for the list path, for DCS-Mirror owners, which is what `repos_to_clone` intended).
pub fn update_repo_components(mut comps: Vec<String>) -> Vec<String> {
    let Some(last) = comps.last() else { return comps };
    let known = |c: &str| USFM_RESOURCE_TYPES.contains(&c) || NON_USFM_RESOURCE_TYPES.contains(&c) || RESOURCE_TYPE_CODES_AND_NAMES.iter().any(|(k, _)| *k == c);
    if !known(last) {
        comps.pop();
    }
    match comps.len() {
        7 => comps.split_off(3),
        6 => comps.split_off(2),
        5 => comps.split_off(1),
        3 if comps[0] == "en" && comps[1] == "condensed" => vec!["en".to_string(), "tn_condensed".to_string()],
        _ => comps,
    }
}

/// One entry of DOC's `repo_clone_list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Located {
    pub url: String,
    /// Directory name under the assets directory: the normalised last URL segment.
    pub dir: String,
    pub resource_type: String,
    /// `"rg" != resource_type`: DOC never fetches these.
    pub fetch: bool,
}

/// `repos_to_clone`: repositories of `lang` whose resource type DOC offers, first URL wins.
/// `needed` says which resource types the endpoint reads from disk; the others are located (so
/// the list is DOC's) but `fetch` is false: DOC clones every type, including `tw` (the largest
/// repository) for an endpoint that never opens it.
pub fn repos_to_clone(lang: &str, repos: &[Repo], en_rg_dir: &str, needed: &dyn Fn(&str) -> bool) -> Vec<Located> {
    let mut out: Vec<Located> = Vec::new();
    for repo in repos {
        if repo.language.ietf_code == lang && RESOURCE_TYPE_CODES_AND_NAMES.iter().any(|(k, _)| *k == repo.resource_type) && !out.iter().any(|l| l.url == repo.repo_url) {
            out.push(Located {
                url: repo.repo_url.clone(),
                dir: get_last_segment(&repo.repo_url, lang, en_rg_dir),
                resource_type: repo.resource_type.clone(),
                fetch: repo.resource_type != "rg" && needed(&repo.resource_type),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments() {
        assert_eq!(get_last_segment("https://content.bibletranslationtools.org/WA-Catalog/sw_ulb", "sw", "x"), "sw_ulb");
        assert_eq!(get_last_segment("https://c.org/faustin_azaza/faustin_azaza", "zmq", "x"), "zmq_mrk_text_reg");
        assert_eq!(get_last_segment("https://c.org/o/nbtt_ln_mat_text_reg", "ln", "x"), "ln_mat_text_reg");
        assert_eq!(get_last_segment("https://c.org/o/en_rg_nt_survey.docx", "en", "/p/en_rg"), "/p/en_rg");
    }

    #[test]
    fn owner_prefix() {
        let c = |s: &str| s.split('_').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(update_repo_components(c("faustin-azaza_rw_php_text_reg")), c("rw_php_text_reg"));
        assert_eq!(update_repo_components(c("danjuma_alfred_h_kgo_phm_text_ulb_l1")), c("kgo_phm_text_ulb"));
        assert_eq!(update_repo_components(c("en_condensed_tn")), vec!["en".to_string(), "tn_condensed".to_string()]);
    }
}
