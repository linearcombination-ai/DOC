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

/// What DOC's `resource_lookup_dto` returns for one requested resource, plus where its repository lives on disk (`dir`, DOC's `prepare_resource_filepath`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LookupDto {
    pub lang_code: String,
    pub lang_name: String,
    pub localized_lang_name: String,
    pub resource_type: String,
    pub resource_type_name: String,
    pub book_code: String,
    pub lang_direction: String,
    pub url: String,
    pub dir: String,
}

/// DOC `resource_lookup.resource_lookup_dto` over the (augmented) catalog rows, in catalog order. Three tiers, first non-empty wins:
/// a `.docx` last segment (reviewer's guide), then repositories whose name has more than two components and names this book
/// (`bys_eph_text_reg`: a per-book repository), then two-component names (`sw_ulb`: one repository per language and type).
/// A resource type DOC has no name for raises `KeyError` in DOC inside its try block: the loop stops with what it had.
pub fn resource_lookup_dto(lang: &str, resource_type: &str, book: &str, repos: &[Repo], en_rg_dir: &str) -> Option<LookupDto> {
    const ZMQ_USER: &str = "faustin_azaza";
    let name_of = |t: &str| RESOURCE_TYPE_CODES_AND_NAMES.iter().find(|(k, _)| *k == t).map(|(_, n)| *n);
    let make = |repo: &Repo, name: &str, last: &str| LookupDto {
        lang_code: lang.to_string(),
        lang_name: repo.language.english_name.clone(),
        localized_lang_name: repo.language.national_name.clone(),
        resource_type: resource_type.to_string(),
        resource_type_name: name.to_string(),
        book_code: book.to_string(),
        lang_direction: repo.language.direction.clone(),
        url: repo.repo_url.clone(),
        dir: last.to_string(),
    };
    let (mut rg, mut more, mut two): (Option<LookupDto>, Option<LookupDto>, Option<LookupDto>) = (None, None, None);
    for repo in repos {
        if repo.language.ietf_code != lang {
            continue;
        }
        let last = get_last_segment(&repo.repo_url, lang, en_rg_dir);
        if last.ends_with("docx") {
            let Some(name) = name_of(resource_type) else { break };
            rg.get_or_insert_with(|| make(repo, name, &last));
            continue;
        }
        let comps = update_repo_components(last.split('_').map(str::to_string).collect());
        if comps.len() > 2 {
            let book_code = comps.get(1).map(String::as_str).unwrap_or("");
            if (repo.repo_url.contains(book_code) || repo.repo_url.contains(ZMQ_USER)) && resource_type == repo.resource_type && name_of(&repo.resource_type).is_some() && book_code == book {
                let Some(name) = name_of(resource_type) else { break };
                more.get_or_insert_with(|| make(repo, name, &last));
            }
        } else if comps.len() == 2 && resource_type == repo.resource_type {
            let Some(name) = name_of(resource_type) else { break };
            two.get_or_insert_with(|| make(repo, name, &last));
        }
    }
    rg.or(more).or(two)
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

    fn repo(url: &str, rtype: &str, lang: &str) -> Repo {
        Repo { repo_url: url.into(), resource_type: rtype.into(), language: crate::catalog::Language { english_name: "L".into(), ietf_code: lang.into(), national_name: "L".into(), direction: "ltr".into() } }
    }

    #[test]
    fn lookup_tiers() {
        let rows = vec![
            repo("https://c.org/WA-Catalog/sw_ulb", "ulb", "sw"),
            repo("https://c.org/Bayan/bys_eph_text_reg", "reg", "bys"),
            repo("https://c.org/Bayan/bys_gal_text_reg", "reg", "bys"),
        ];
        let d = resource_lookup_dto("sw", "ulb", "jud", &rows, "x").unwrap();
        assert_eq!((d.url.as_str(), d.dir.as_str()), ("https://c.org/WA-Catalog/sw_ulb", "sw_ulb"));
        assert_eq!(resource_lookup_dto("bys", "reg", "eph", &rows, "x").unwrap().dir, "bys_eph_text_reg");
        assert_eq!(resource_lookup_dto("bys", "reg", "gal", &rows, "x").unwrap().dir, "bys_gal_text_reg");
        assert!(resource_lookup_dto("bys", "reg", "mat", &rows, "x").is_none());
        assert!(resource_lookup_dto("sw", "tq", "jud", &rows, "x").is_none());
    }

    #[test]
    fn owner_prefix() {
        let c = |s: &str| s.split('_').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(update_repo_components(c("faustin-azaza_rw_php_text_reg")), c("rw_php_text_reg"));
        assert_eq!(update_repo_components(c("danjuma_alfred_h_kgo_phm_text_ulb_l1")), c("kgo_phm_text_ulb"));
        assert_eq!(update_repo_components(c("en_condensed_tn")), vec!["en".to_string(), "tn_condensed".to_string()]);
    }
}
