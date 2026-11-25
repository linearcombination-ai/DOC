//! The catalog model and the parsing of the BIEL GraphQL response bytes.
//!
//! `as_built` reproduces `fetch_source_data` / `fetch_non_primary_source_data`
//! (`resource_lookup.py:61-201`): a bad response becomes an empty catalog (which DOC then caches
//! for 180 s and every caller treats as "no data"). `fixed` skips invalid rows with a finding and
//! reports a structural failure as a typed error so the adapter can retry it.

use doc_contract::{Finding, FindingCode, Severity};
use serde_json::Value;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    pub english_name: String,
    pub ietf_code: String,
    pub national_name: String,
    /// `ltr` or `rtl` (DOC's `LangDirEnum`).
    pub direction: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoEntry {
    /// `str(HttpUrl)`: the form Pydantic's URL type (the `url` crate, as here) serialises.
    pub repo_url: String,
    pub resource_type: String,
    pub language: Language,
}

/// Which of DOC's two queries a body answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Query {
    /// `status = "Primary"` rows.
    Primary,
    /// the `bg` rows; only repositories under `WA-Catalog` are kept (`"WA-Catalog" in repo_url`).
    NonPrimary,
}

/// What the as-built parse did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsBuilt {
    /// Valid rows, sorted by `repo_url` as DOC sorts them.
    Repos(Vec<RepoEntry>),
    /// DOC returns `SourceData(git_repo=[])` (and caches it): the reason is for the log.
    EmptyCached(String),
    /// DOC's code raises an exception it does not catch (a 500 from FastAPI, nothing cached).
    Crash(String),
}

fn as_str(v: &Value) -> Option<&str> {
    v.as_str()
}

fn valid_row(row: &Value) -> Result<RepoEntry, String> {
    let url = row.get("repo_url").and_then(as_str).ok_or("repo_url missing or not a string")?;
    let parsed = Url::parse(url).map_err(|e| format!("repo_url invalid: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("repo_url is not an http(s) URL".into());
    }
    let content = row.get("content").ok_or("content missing")?;
    let resource_type = content.get("resource_type").and_then(as_str).ok_or("resource_type missing or not a string")?;
    let lang = content.get("language").ok_or("language missing")?;
    let text = |k: &str| lang.get(k).and_then(as_str).map(str::to_string).ok_or(format!("language.{k} missing or not a string"));
    let direction = text("direction")?;
    if direction != "ltr" && direction != "rtl" {
        return Err(format!("language.direction {direction:?} is not ltr or rtl"));
    }
    Ok(RepoEntry {
        repo_url: parsed.to_string(),
        resource_type: resource_type.to_string(),
        language: Language {
            english_name: text("english_name")?,
            ietf_code: text("ietf_code")?,
            national_name: text("national_name")?,
            direction,
        },
    })
}

fn rows_of(body: &[u8]) -> Result<Vec<Value>, AsBuilt> {
    let Ok(json) = serde_json::from_slice::<Value>(body) else {
        return Err(AsBuilt::EmptyCached("response is not JSON".into()));
    };
    let data = match json.get("data") {
        None => return Err(AsBuilt::EmptyCached("Invalid payload structure, no data.".into())),
        Some(Value::Object(o)) => o,
        // `"git_repo" in None` is a TypeError in DOC, not caught
        Some(other) => return Err(AsBuilt::Crash(format!("data is {}", type_name(other)))),
    };
    match data.get("git_repo") {
        Some(Value::Array(rows)) => Ok(rows.clone()),
        Some(_) | None => Err(AsBuilt::EmptyCached("Invalid payload structure, no data.".into())),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// DOC's `valid_repos` comprehension: rows whose `content.resource_type` and `content.language`
/// are not null (and, for the non-primary query, whose URL mentions `WA-Catalog`).
fn doc_filter(rows: Vec<Value>, query: Query) -> Result<Vec<Value>, AsBuilt> {
    let mut kept = Vec::new();
    for row in rows {
        let content = match row.get("content") {
            // `repo.get("content", {})` is None when the key is present and null: `.get` on None raises
            Some(Value::Null) => return Err(AsBuilt::Crash("content is null (AttributeError)".into())),
            Some(c) => c.clone(),
            None => Value::Object(Default::default()),
        };
        let has_type = content.get("resource_type").is_some_and(|v| !v.is_null());
        let has_lang = content.get("language").is_some_and(|v| !v.is_null());
        let mut keep = has_type && has_lang;
        if keep && query == Query::NonPrimary {
            keep = row.get("repo_url").and_then(as_str).is_some_and(|u| u.contains("WA-Catalog"));
        }
        if keep {
            kept.push(row);
        }
    }
    // `valid_repos.sort(key=lambda repo: repo["repo_url"])`
    let mut keyed = Vec::with_capacity(kept.len());
    for row in kept {
        match row.get("repo_url").and_then(as_str) {
            Some(u) => keyed.push((u.to_string(), row)),
            None => return Err(AsBuilt::Crash("repo_url missing or not a string (KeyError/TypeError in sort)".into())),
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(keyed.into_iter().map(|(_, r)| r).collect())
}

/// `fetch_source_data` semantics for a 200 response body.
pub fn parse_as_built(body: &[u8], query: Query) -> AsBuilt {
    let rows = match rows_of(body) {
        Ok(r) => r,
        Err(outcome) => return outcome,
    };
    let rows = match doc_filter(rows, query) {
        Ok(r) => r,
        Err(outcome) => return outcome,
    };
    let mut repos = Vec::with_capacity(rows.len());
    for row in &rows {
        match valid_row(row) {
            Ok(r) => repos.push(r),
            // `SourceData.model_validate` raises ValidationError for the whole list
            Err(reason) => return AsBuilt::EmptyCached(format!("ValidationError: {reason}")),
        }
    }
    AsBuilt::Repos(repos)
}

/// Why the fixed parse refused a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Malformed {
    NotJson(String),
    GraphqlErrors(String),
    Shape(String),
}

impl std::fmt::Display for Malformed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Malformed::NotJson(m) => write!(f, "response is not JSON: {m}"),
            Malformed::GraphqlErrors(m) => write!(f, "GraphQL errors: {m}"),
            Malformed::Shape(m) => write!(f, "unexpected shape: {m}"),
        }
    }
}

/// The fixed parse: structure errors are typed (the adapter retries them), bad rows are skipped
/// with a finding. An empty `git_repo` is a valid, empty catalog (the data API says so), not an
/// error; the adapter decides whether an empty primary catalog is plausible.
pub fn parse_fixed(body: &[u8], query: Query) -> Result<(Vec<RepoEntry>, Vec<Finding>), Malformed> {
    let json: Value = serde_json::from_slice(body).map_err(|e| Malformed::NotJson(e.to_string()))?;
    if let Some(errors) = json.get("errors") {
        if json.get("data").is_none_or(Value::is_null) {
            return Err(Malformed::GraphqlErrors(errors.to_string().chars().take(300).collect()));
        }
    }
    let rows = match json.get("data").and_then(|d| d.get("git_repo")) {
        Some(Value::Array(rows)) => rows,
        Some(other) => return Err(Malformed::Shape(format!("data.git_repo is {}", type_name(other)))),
        None => return Err(Malformed::Shape("data.git_repo missing".into())),
    };
    let mut repos = Vec::with_capacity(rows.len());
    let mut findings = Vec::new();
    let mut skipped = 0usize;
    for (index, row) in rows.iter().enumerate() {
        let content = row.get("content");
        let null_part = content.is_none_or(Value::is_null)
            || content.and_then(|c| c.get("resource_type")).is_none_or(Value::is_null)
            || content.and_then(|c| c.get("language")).is_none_or(Value::is_null);
        if null_part {
            // DOC drops these too (100 rows with a null language in the recorded catalog)
            skipped += 1;
            continue;
        }
        let url_ok = row.get("repo_url").and_then(as_str);
        if query == Query::NonPrimary && !url_ok.is_some_and(|u| u.contains("WA-Catalog")) {
            continue;
        }
        match valid_row(row) {
            Ok(r) => repos.push(r),
            Err(reason) => findings.push(
                Finding::new(FindingCode::CatalogRowInvalid, Severity::Warning, format!("row {index}: {reason}")),
            ),
        }
    }
    let _ = skipped;
    repos.sort_by(|a, b| a.repo_url.cmp(&b.repo_url));
    Ok((repos, findings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(rows: &str) -> Vec<u8> {
        format!(r#"{{"data":{{"git_repo":[{rows}]}}}}"#).into_bytes()
    }
    const ROW: &str = r#"{"repo_url":"https://content.bibletranslationtools.org/WA-Catalog/sw_ulb","content":{"resource_type":"ulb","language":{"english_name":"Swahili","ietf_code":"sw","national_name":"Kiswahili","direction":"ltr"}}}"#;

    #[test]
    fn as_built_parses_and_discards_all_on_one_bad_row() {
        let good = parse_as_built(&body(ROW), Query::Primary);
        assert!(matches!(good, AsBuilt::Repos(ref r) if r.len() == 1));
        let bad_row = ROW.replace("\"ltr\"", "\"sideways\"");
        let both = body(&format!("{ROW},{bad_row}"));
        assert!(matches!(parse_as_built(&both, Query::Primary), AsBuilt::EmptyCached(_)));
    }

    #[test]
    fn as_built_null_data_crashes_and_null_language_is_dropped() {
        assert!(matches!(parse_as_built(br#"{"data":null}"#, Query::Primary), AsBuilt::Crash(_)));
        let nulled = r#"{"repo_url":"https://x.org/a/b","content":{"resource_type":"ulb","language":null}}"#;
        assert_eq!(parse_as_built(&body(nulled), Query::Primary), AsBuilt::Repos(vec![]));
    }

    #[test]
    fn fixed_skips_bad_rows_with_a_finding_and_types_structure_errors() {
        let bad_row = ROW.replace("\"ltr\"", "\"sideways\"");
        let (repos, findings) = parse_fixed(&body(&format!("{ROW},{bad_row}")), Query::Primary).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(findings.len(), 1);
        assert!(parse_fixed(b"<html>", Query::Primary).is_err());
        assert!(matches!(parse_fixed(br#"{"errors":[{"message":"boom"}]}"#, Query::Primary), Err(Malformed::GraphqlErrors(_))));
        assert!(matches!(parse_fixed(br#"{"data":{}}"#, Query::Primary), Err(Malformed::Shape(_))));
    }
}
