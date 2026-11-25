//! P5 job core: the middle of the document pipeline as one pure function (no I/O, no clock).
//!
//! Input (JSON): the request exactly as the API received it, one `Dto` + file tree per non-USFM resource
//! (the shell cloned and listed them), and the USFM text of each requested USFM book. Output: the
//! cache key, the resolved layout, the interleaved `parts` (what `assemble_content` returns in DOC)
//! and the findings. The same code path the P3b look harness ran, behind one seam call.

use std::collections::BTreeMap;

use doc_canon::look::UlbOptions;
use doc_md::links::{ReaderCompat, Req};
use doc_md::readers::{self, Dto, ReaderSettings};
use doc_model::tree::{Tree, TreeData};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct UsfmIn {
    lang_code: String,
    #[serde(default)]
    lang_name: String,
    book_code: String,
    #[serde(default)]
    national_book_name: String,
    #[serde(default)]
    resource_type_name: String,
    /// The text of the book's USFM file.
    source: String,
}

#[derive(Deserialize)]
struct Input {
    request: Value,
    #[serde(default)]
    dtos: Vec<Dto>,
    /// Directory (as in `Dto.dir`) to listing and texts.
    #[serde(default)]
    tree: BTreeMap<String, TreeData>,
    #[serde(default)]
    usfm: Vec<UsfmIn>,
    #[serde(default = "default_compat")]
    bug_compat: String,
}

#[derive(Deserialize)]
struct AssembleIn {
    request: Value,
    books: Value,
    #[serde(default = "default_structure")]
    structure: String,
    #[serde(default = "default_compat")]
    bug_compat: String,
    #[serde(default)]
    unresolved: Vec<Value>,
    #[serde(default)]
    findings: Vec<Value>,
}

fn default_structure() -> String {
    "proposed_look".into()
}
fn default_compat() -> String {
    "fixed".into()
}

fn fail(kind: &str, message: impl std::fmt::Display) -> String {
    json!({"ok": false, "kind": kind, "error": message.to_string()}).to_string()
}

fn flag(request: &Value, name: &str, default: bool) -> bool {
    request.get(name).and_then(Value::as_bool).unwrap_or(default)
}

pub use doc_request::key_json;

/// Stage "Parsing asset files": read the resources into typed books (TN from the tree, ULB from its USFM
/// text). Output: `{ok, books, findings, title1, title2}`. Never panics on input: every failure is
/// `{"ok":false,"kind":..,"error":..}`; `kind` is `input` (the shell built a bad call) or `reader`
/// (a resource could not be read, which DOC would have raised).
pub fn parse_json(input: &str) -> String {
    let parsed: Input = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return fail("input", e),
    };
    let req = &parsed.request;
    let requests: Vec<Req> = req
        .get("resource_requests")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    Some(Req {
                        lang_code: r.get("lang_code")?.as_str()?.to_string(),
                        resource_type: r.get("resource_type")?.as_str()?.to_string(),
                        book_code: r.get("book_code")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut tree = Tree::default();
    for (dir, data) in &parsed.tree {
        tree.add_dir(dir, data);
    }
    let compat = if parsed.bug_compat == "as_built" { ReaderCompat::as_built() } else { ReaderCompat::fixed() };
    let settings = ReaderSettings {
        layout_for_print: flag(req, "layout_for_print", false),
        generate_docx: flag(req, "generate_docx", false),
        show_tn_book_intro: flag(req, "show_tn_book_intro", true),
        compat,
        xref_links: false,
    };
    let read = match readers::read_books(&parsed.dtos, &requests, &tree, &settings) {
        Ok(r) => r,
        Err(e) => return fail("reader", e),
    };
    let mut usfm_books = Vec::new();
    let mut findings = Vec::new();
    let mut book_names: Vec<String> = Vec::new();
    for book in &parsed.usfm {
        let model = doc_canon::usfm::parse_book(&book.source);
        let chapters: Vec<Value> = model
            .chapters
            .iter()
            .map(|c| {
                let verses: Vec<Value> = c.verses.iter().map(|v| json!([v.label_doc, doc_canon::usfm::verse_html(&v.text_doc)])).collect();
                json!([c.number, {"content": doc_canon::look::ulb_chapter_html(c, &UlbOptions::default()), "verses": verses}])
            })
            .collect();
        for (code, n) in &model.lint {
            findings.push(json!({"code": "usfm_lint", "severity": "info", "lang": book.lang_code, "book": book.book_code, "detail": format!("{code}: {n}")}));
        }
        let national = if book.national_book_name.is_empty() { model.book.clone() } else { book.national_book_name.clone() };
        book_names.push(national.clone());
        usfm_books.push(json!({
            "lang_code": book.lang_code, "lang_name": book.lang_name, "book_code": book.book_code,
            "national_book_name": national, "resource_type_name": book.resource_type_name, "chapters": chapters,
        }));
    }
    // title page strings: "<language>: <resource names>" and the book names (DOC's get_languages_title_page_strings, simplified)
    let mut langs: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut note = |lang: &str, code: &str| {
        if !lang.is_empty() && !langs.iter().any(|l| l == lang) {
            langs.push(lang.to_string());
        }
        if let Some((_, n)) = doc_model::tables::RESOURCE_TYPE_NAMES.iter().find(|(c, _)| *c == code) {
            if !names.iter().any(|x| x == n) {
                names.push((*n).to_string());
            }
        }
    };
    for d in &parsed.dtos {
        note(&d.lang_name, &d.resource_type);
    }
    for (u, r) in parsed.usfm.iter().zip(req.get("resource_requests").and_then(Value::as_array).into_iter().flatten().filter(|r| doc_model::tables::USFM_TYPES.contains(&r.get("resource_type").and_then(Value::as_str).unwrap_or(""))))
    {
        note(&u.lang_name, r.get("resource_type").and_then(Value::as_str).unwrap_or(""));
    }
    let books = json!({"tn": read.tn, "tnc": read.tnc, "tq": read.tq, "tw": read.tw, "bc": read.bc, "usfm": usfm_books});
    json!({"ok": true, "books": books, "findings": findings, "title1": format!("{}: {}", langs.join(", "), names.join(", ")), "title2": book_names.join(", ")}).to_string()
}

/// Stage "Assembling content": the interleaver over the typed books. Output as `doc_canon::run_json`
/// (`{ok, key, layout, parts, findings}`) with the parse stage's findings appended, or
/// `{"ok":false,"kind":"violations"|"interleave"|"input",..}`.
pub fn assemble_json(input: &str) -> String {
    let parsed: AssembleIn = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return fail("input", e),
    };
    let mut request = parsed.request.clone();
    if let Some(obj) = request.as_object_mut() {
        // Translation Words in print: link out (the captain's default for the look); an explicit value wins.
        obj.entry("tw_mode").or_insert_with(|| json!("link"));
    }
    let call = json!({"request": request, "books": parsed.books, "bug_compat": parsed.bug_compat, "structure": parsed.structure, "unresolved": parsed.unresolved});
    let out = doc_canon::run_json(&call.to_string());
    let Ok(mut v) = serde_json::from_str::<Value>(&out) else { return fail("interleave", "core returned no JSON") };
    if v.get("ok") != Some(&json!(true)) {
        if let Some(violations) = v.get("violations") {
            return json!({"ok": false, "kind": "violations", "violations": violations}).to_string();
        }
        return fail("interleave", v.get("error").or_else(|| v.get("input_error")).map(Value::to_string).unwrap_or_default());
    }
    if let Some(list) = v.get_mut("findings").and_then(Value::as_array_mut) {
        list.extend(parsed.findings);
    }
    v.to_string()
}
