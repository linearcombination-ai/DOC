//! The catalog snapshot (what the BIEL GraphQL data API returns) and DOC's rules for it:
//! row filtering and sorting, pydantic-equivalent validation (all-or-nothing in `as_built`),
//! the six entries DOC adds itself, and `lang_codes_and_names`.
//!
//! The shell fetches and transports the rows; every rule that decides which rows survive lives
//! here, so both modes are one function with a flag, not two code paths in the shell.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::findings::{Finding, Findings};
use crate::tables_gen::GATEWAY_LANGUAGES_SORTED;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    pub english_name: String,
    pub ietf_code: String,
    pub national_name: String,
    pub direction: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    pub repo_url: String,
    pub resource_type: String,
    pub language: Language,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryKind {
    /// `fetch_source_data`: rows with a resource type and a language.
    Primary,
    /// `fetch_non_primary_source_data`: the same, and only `WA-Catalog` URLs.
    NonPrimary,
}

/// What `normalize` returns: the rows that survived, plus how the outcome came about.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Normalized {
    pub repos: Vec<Repo>,
    /// `as_built` only: DOC would have returned `SourceData(git_repo=[])` for this payload
    /// (a validation failure) and cached it. `repos` is empty then.
    pub as_built_validation_failure: Option<String>,
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// pydantic `HttpUrl` as far as the data needs: http(s) scheme, a host, no whitespace.
fn plausible_http_url(url: &str) -> bool {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    match rest {
        Some(r) => !r.is_empty() && !r.starts_with('/') && !url.chars().any(char::is_whitespace),
        None => false,
    }
}

fn parse_row(row: &Value) -> Result<Repo, String> {
    let content = row.get("content").ok_or("row has no content")?;
    let lang = content.get("language").ok_or("content has no language")?;
    let url = str_field(row, "repo_url").ok_or("repo_url is not a string")?;
    if !plausible_http_url(url) {
        return Err(format!("repo_url is not an http(s) URL: {url:?}"));
    }
    let need = |k: &str| str_field(lang, k).map(str::to_string).ok_or_else(|| format!("language.{k} is not a string"));
    let direction = need("direction")?;
    if direction != "ltr" && direction != "rtl" {
        return Err(format!("language.direction {direction:?} is neither ltr nor rtl"));
    }
    Ok(Repo {
        repo_url: url.to_string(),
        resource_type: str_field(content, "resource_type").ok_or("content.resource_type is not a string")?.to_string(),
        language: Language {
            english_name: need("english_name")?,
            ietf_code: need("ietf_code")?,
            national_name: need("national_name")?,
            direction,
        },
    })
}

/// DOC's pre-filter: `repo.get("content", {}).get("resource_type") is not None and
/// ...get("language") is not None`, and for the non-primary query also `"WA-Catalog" in repo_url`.
fn survives_prefilter(row: &Value, kind: QueryKind) -> bool {
    let content = row.get("content").filter(|c| !c.is_null());
    let ok = content.is_some_and(|c| !c.get("resource_type").unwrap_or(&Value::Null).is_null() && !c.get("language").unwrap_or(&Value::Null).is_null());
    ok && (kind == QueryKind::Primary || row.get("repo_url").and_then(Value::as_str).is_some_and(|u| u.contains("WA-Catalog")))
}

/// Filter, validate and sort the rows of `data.git_repo`.
///
/// `skip_bad_rows` is the `fixed` behaviour: a row that fails validation is dropped and reported
/// instead of emptying the whole catalog.
pub fn normalize(rows: &[Value], kind: QueryKind, skip_bad_rows: bool, findings: &mut Findings) -> Normalized {
    let mut repos = Vec::with_capacity(rows.len());
    for row in rows.iter().filter(|r| survives_prefilter(r, kind)) {
        match parse_row(row) {
            Ok(repo) => repos.push(repo),
            Err(why) if skip_bad_rows => {
                findings.push(Finding::new("CatalogRowSkipped", "warn").detail(format!("{why} (repo_url {:?})", row.get("repo_url"))));
            }
            Err(why) => {
                return Normalized { repos: Vec::new(), as_built_validation_failure: Some(why) };
            }
        }
    }
    repos.sort_by(|a, b| a.repo_url.cmp(&b.repo_url)); // Python sorts by code point; so does str Ord
    Normalized { repos, as_built_validation_failure: None }
}

fn lang(english: &str, code: &str, national: &str) -> Language {
    Language { english_name: english.into(), ietf_code: code.into(), national_name: national.into(), direction: "ltr".into() }
}

/// `add_data_not_supplied_by_data_api`: appends the entries DOC supplies itself unless the
/// (language, resource type) pair is already present. Returns the number appended.
pub fn augment(repos: &mut Vec<Repo>) -> usize {
    let bg = lang("Bulgarian", "bg", "български език");
    let id = lang("Indonesian", "id", "Bahasa Indonesian");
    let en = lang("English", "en", "English");
    let extra: [(&str, &str, &Language); 6] = [
        ("https://content.bibletranslationtools.org/WA-Catalog/bg_bpb", "bpb", &bg),
        ("https://content.bibletranslationtools.org/WycliffeAssociates/en_tn_condensed", "tn-condensed", &en),
        ("https://content.bibletranslationtools.org/WA-Catalog/id_tbi", "tbi", &id),
        ("https://content.bibletranslationtools.org/WA-Catalog/id_ayt", "ayt", &id),
        ("https://content.bibletranslationtools.org/WA-Catalog/id_tq", "tq", &id),
        ("https://content.bibletranslationtools.org/WA-Catalog/id_tw", "tw", &id),
    ];
    let mut added = 0;
    for (url, rt, l) in extra {
        let exists = repos.iter().any(|r| r.language.ietf_code == l.ietf_code && r.resource_type == rt);
        if !exists {
            repos.push(Repo { repo_url: url.into(), resource_type: rt.into(), language: l.clone() });
            added += 1;
        }
    }
    added
}

/// `unique_tuples`: order-preserving dedup.
fn dedup_keep_order<T: Eq + std::hash::Hash + Clone>(items: Vec<T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

fn entry(l: &Language) -> (String, String, bool) {
    let gateway = GATEWAY_LANGUAGES_SORTED.binary_search(&l.ietf_code.as_str()).is_ok();
    let name = if l.national_name.contains(&l.english_name) {
        l.national_name.clone()
    } else {
        format!("{} ({})", l.national_name, l.english_name)
    };
    (l.ietf_code.clone(), name, gateway)
}

/// `lang_codes_and_names`. In `as_built` an empty primary or non-primary catalog gives `[]`
/// (even when the other one has rows); `fixed` returns what it has and reports the gap.
pub fn lang_codes_and_names(primary: &[Repo], non_primary: &[Repo], tolerate_partial: bool, findings: &mut Findings) -> Vec<(String, String, bool)> {
    if primary.is_empty() {
        return Vec::new();
    }
    if non_primary.is_empty() {
        if !tolerate_partial {
            return Vec::new();
        }
        findings.push(Finding::new("NonPrimaryCatalogEmpty", "warn").detail("the non-primary (bg) query returned no rows; the list is built from the primary catalog alone"));
    }
    let values: Vec<(String, String, bool)> = primary.iter().chain(non_primary.iter()).map(|r| entry(&r.language)).collect();
    let mut unique = dedup_keep_order(values);
    unique.sort_by(|a, b| a.1.cmp(&b.1)); // stable, by name, code point order
    unique
}
