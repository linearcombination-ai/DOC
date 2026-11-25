//! doc-interleave: the interleavers (C12) and the Translation Words matcher (C13), plus the
//! request-to-parts entry point [`run_json`]. Depends on `doc-model` and `doc-request` only:
//! no regex engine, no markdown engine, no kitchen. Builds for `wasm32-unknown-unknown`.

pub mod interleave;
pub mod look;
pub mod tw;
pub mod xref;

use serde::Deserialize;
use serde_json::{json, Value};

use doc_model::model::{Books, Finding, FindingCode, Severity};
use doc_request as request;
use interleave::BugCompat;

#[derive(Deserialize)]
struct Input {
    request: Value,
    #[serde(default)]
    books: Books,
    /// Requests the shell could not resolve to a repository (reported as findings).
    #[serde(default)]
    unresolved: Vec<UnresolvedIn>,
    #[serde(default)]
    bug_compat: CompatIn,
    /// option: the books carry working TN cross-reference links; check them in the output
    #[serde(default)]
    xref_links: bool,
    /// option: Translation Words word rule, `"python"`, `"uax29"` or `"kitchen"`. Absent: `as_built`
    /// uses Python's `\b`; otherwise the shell default (kitchen's `words/1` through `doc-canon::run_json`,
    /// UAX #29 when this crate runs alone).
    #[serde(default)]
    tw_word_rule: Option<String>,
    /// option: output structure, `"as_built"` (default: DOC's heading levels and shapes) or `"proposed_look"`
    /// (P3b: heading levels and semantic classes of `proposed-doc-format.docx`; interleaving unchanged).
    #[serde(default)]
    structure: Option<String>,
}

#[derive(Deserialize)]
struct UnresolvedIn {
    lang_code: String,
    resource_type: String,
    book_code: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CompatIn {
    Named(String),
    Flags(BugCompat),
}
impl Default for CompatIn {
    fn default() -> Self {
        CompatIn::Named("fixed".into())
    }
}

/// JSON in, JSON out. Never panics on bad input: every failure is a JSON `ok:false`.
pub fn run_json(input: &str) -> String {
    run_json_with(input, None)
}

/// [`run_json`] with the Translation Words word rule supplied by the caller (`None`: the rule
/// `bug_compat` names). `doc-canon` uses this to supply kitchen's `words/1`.
pub fn run_json_with(input: &str, rule: Option<&dyn doc_model::text::WordRule>) -> String {
    let parsed: Input = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return json!({"ok": false, "input_error": e.to_string()}).to_string(),
    };
    let compat = match parsed.bug_compat {
        CompatIn::Named(n) if n == "as_built" => BugCompat::as_built(),
        CompatIn::Named(n) if n == "fixed" => BugCompat::fixed(),
        CompatIn::Named(n) => return json!({"ok": false, "input_error": format!("unknown bug_compat {n}")}).to_string(),
        CompatIn::Flags(f) => f,
    };
    let valid = match request::validate(&parsed.request) {
        Ok(v) => v,
        Err(violations) => return json!({"ok": false, "violations": violations}).to_string(),
    };
    let key = request::request_key(&valid);
    let named: Option<Box<dyn doc_model::text::WordRule>> = match parsed.tw_word_rule.as_deref() {
        Some("python") => Some(Box::new(doc_model::text::PythonBoundary)),
        Some("uax29") => Some(Box::new(tw::Uax29)),
        Some("kitchen") if rule.is_none() => return json!({"ok": false, "input_error": "tw_word_rule kitchen needs doc-canon::run_json"}).to_string(),
        Some("kitchen") | None => None,
        Some(other) => return json!({"ok": false, "input_error": format!("unknown tw_word_rule {other}")}).to_string(),
    };
    let built = match (rule, named.as_deref()) {
        (_, Some(n)) => interleave::build_with(&valid, &parsed.books, compat, n),
        (Some(r), None) => interleave::build_with(&valid, &parsed.books, compat, r),
        (None, None) => interleave::build(&valid, &parsed.books, compat),
    };
    match built {
        Ok(mut out) => {
            match parsed.structure.as_deref() {
                None | Some("as_built") => {}
                Some("proposed_look") => look::restructure(&mut out.parts),
                Some(other) => return json!({"ok": false, "input_error": format!("unknown structure {other}")}).to_string(),
            }
            if parsed.xref_links {
                let (_live, dead) = xref::resolve(&mut out.parts);
                out.findings.extend(dead);
            }
            for u in &parsed.unresolved {
                out.findings.insert(
                    0,
                    Finding::new(FindingCode::UnresolvedRequest, Severity::Warning)
                        .lang(&u.lang_code)
                        .resource(&u.resource_type)
                        .detail(u.book_code.clone()),
                );
            }
            json!({"ok": true, "key": key, "layout": valid.layout.code(), "parts": out.parts, "findings": out.findings}).to_string()
        }
        Err(e) => json!({"ok": false, "key": key, "error": e.to_string()}).to_string(),
    }
}
