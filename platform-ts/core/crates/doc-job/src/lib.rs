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
    /// The resource type code (`ulb`, `reg`, ...), for the book name rules.
    #[serde(default)]
    resource_type: String,
    /// The text of the book's USFM file (empty when the book is empty or `writer_files` carry it).
    #[serde(default)]
    source: String,
    /// A Writer repository: `(relative path with '/', text)` of its chapter chunk files and `front/title.txt`; stitched into one USFM text.
    #[serde(default)]
    writer_files: Vec<(String, String)>,
    #[serde(default)]
    fallback_title: String,
    /// The repository's listing and the texts the name rules read; absent: the English name of the book code is used.
    #[serde(default)]
    name_input: Option<doc_books::bookname::NameInput>,
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
    /// DOC's `CHECK_USFM` setting: repair USFM known to be defective (default on).
    #[serde(default = "yes")]
    check_usfm: bool,
    /// DOC's `CHECK_ALL_BOOKS_FOR_LANGUAGE` setting: repair every book of a language with any known defective resource (default on).
    #[serde(default = "yes")]
    check_all_books_for_language: bool,
    /// DOC's `found_resource_lookup_dtos` for the title page, in request order (every resolved request, USFM or not). Absent: the simplified title.
    #[serde(default)]
    titles: Vec<TitleDto>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct TitleDto {
    lang_code: String,
    lang_name: String,
    #[serde(default)]
    localized_lang_name: String,
    resource_type_name: String,
    book_code: String,
}

/// DOC's `get_languages_title_page_strings`, ported: the first two languages (in order of appearance) each give
/// `"<lang_name> (<localized name>): <resource type names> for <book names>"`; a language whose books cover a whole testament says so.
/// `usfm_names` holds `(lang_code, lower-case book code, national book name)` of the parsed USFM books.
fn title_page_strings(dtos: &[TitleDto], usfm_names: &[(String, String, String)]) -> (String, String) {
    use doc_model::tables::{BOOKS, BOOK_NUMBERS};
    let mut lang_codes: Vec<&str> = Vec::new();
    for d in dtos {
        if !lang_codes.contains(&d.lang_code.as_str()) {
            lang_codes.push(&d.lang_code);
        }
    }
    let testament = |ot: bool| -> Vec<&'static str> { BOOK_NUMBERS.iter().filter(|(_, n)| n.parse::<u32>().map(|n| (n <= 39) == ot).unwrap_or(false)).map(|(c, _)| *c).collect() };
    let details = |lang: &str| -> String {
        let own: Vec<&TitleDto> = dtos.iter().filter(|d| d.lang_code == lang).collect();
        let Some(first) = own.first() else { return String::new() };
        let mut names: Vec<(String, String)> = Vec::new(); // (lower-case code, display name) in first-seen order
        let mut resource_names: Vec<&str> = Vec::new();
        for d in &own {
            let code = d.book_code.to_lowercase();
            if !names.iter().any(|(c, _)| *c == code) {
                let name = usfm_names
                    .iter()
                    .find(|(l, c, _)| l == lang && *c == code)
                    .map(|(_, _, n)| n.clone())
                    .or_else(|| BOOKS.iter().find(|(c, _, _)| *c == code).map(|(_, n, _)| (*n).to_string()))
                    .unwrap_or_else(|| code.clone());
                names.push((code, name));
            }
            if !resource_names.contains(&d.resource_type_name.as_str()) {
                resource_names.push(&d.resource_type_name);
            }
        }
        let have = |codes: &[&str]| codes.iter().all(|c| names.iter().any(|(n, _)| n == c));
        let (ot, nt) = (testament(true), testament(false));
        let (full_ot, full_nt) = (have(&ot), have(&nt));
        let mut items: Vec<String> = Vec::new();
        let mut remaining: Vec<&(String, String)> = names.iter().collect();
        if full_ot && full_nt {
            items.push("Old and New Testaments".into());
        } else {
            if full_ot {
                items.push("Old Testament".into());
                remaining.retain(|(c, _)| !ot.contains(&c.as_str()));
            }
            if full_nt {
                items.push("New Testament".into());
                remaining.retain(|(c, _)| !nt.contains(&c.as_str()));
            }
            remaining.sort_by_key(|(c, _)| BOOKS.iter().position(|(b, _, _)| b == c).unwrap_or(999));
            items.extend(remaining.iter().map(|(_, n)| n.clone()));
        }
        format!("{} ({}): {} for {}", first.lang_name, first.localized_lang_name, resource_names.join(", "), items.join(", "))
    };
    (lang_codes.first().map(|l| details(l)).unwrap_or_default(), lang_codes.get(1).map(|l| details(l)).unwrap_or_default())
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
        let stitched;
        let source: &str = if book.source.is_empty() && !book.writer_files.is_empty() {
            let files: Vec<(String, Vec<u8>)> = book.writer_files.iter().map(|(p, t)| (p.clone(), t.as_bytes().to_vec())).collect();
            stitched = doc_canon::stitch::stitch_writer_book(&files, &book.book_code, &book.fallback_title);
            &stitched
        } else {
            &book.source
        };
        // DOC repairs the USFM of resources it knows to be defective, chapter by chapter, before converting it (CHECK_USFM and CHECK_ALL_BOOKS_FOR_LANGUAGE, both on in DOC's .env)
        let repaired;
        let source: &str = if parsed.check_usfm && doc_canon::usfm_fixes::needs_fixing(&book.lang_code, &book.resource_type, &book.book_code, parsed.check_all_books_for_language) {
            let labelled = if flag(req, "use_chapter_labels", false) { source.to_string() } else { doc_canon::usfm_fixes::remove_chapter_labels(source) };
            repaired = doc_canon::usfm_fixes::fix_book(&labelled);
            if repaired != source {
                findings.push(json!({"code": "usfm_repaired", "severity": "info", "lang": book.lang_code, "book": book.book_code, "detail": "this resource is known to have USFM defects; DOC's repair rules were applied before parsing"}));
            }
            &repaired
        } else {
            source
        };
        let model = doc_canon::usfm::parse_book(source);
        let chapters: Vec<Value> = model
            .chapters
            .iter()
            .map(|c| {
                let verses: Vec<Value> = c.verses.iter().map(|v| json!([v.label_doc, doc_canon::usfm::verse_html(&v.text_doc)])).collect();
                json!([c.number, {"content": doc_canon::look::ulb_chapter_html(c, &UlbOptions::default()), "content_doc": doc_canon::look::chapter_html_doc(c), "verses": verses}])
            })
            .collect();
        for (code, n) in &model.lint {
            findings.push(json!({"code": "usfm_lint", "severity": "info", "lang": book.lang_code, "book": book.book_code, "detail": format!("{code}: {n}")}));
        }
        // DOC: the localized name (the book's markers, the manifest, front/title.txt), else the English name of the book code
        let mut localized = book.national_book_name.clone();
        if localized.is_empty() {
            if let Some(input) = &book.name_input {
                let (name, name_findings) = doc_books::bookname::usfm_book_name(&book.lang_code, &book.resource_type, &book.book_code, source, input);
                localized = name;
                for f in name_findings {
                    findings.push(serde_json::to_value(f).unwrap_or(Value::Null));
                }
            }
        }
        let national = if localized.is_empty() {
            doc_model::tables::BOOKS.iter().find(|(c, _, _)| *c == book.book_code.to_lowercase()).map(|(_, n, _)| (*n).to_string()).unwrap_or_else(|| model.book.clone())
        } else {
            localized
        };
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
    let (title1, title2) = if parsed.titles.is_empty() {
        (format!("{}: {}", langs.join(", "), names.join(", ")), book_names.join(", "))
    } else {
        let usfm_names: Vec<(String, String, String)> = parsed.usfm.iter().zip(&book_names).map(|(u, n)| (u.lang_code.clone(), u.book_code.to_lowercase(), n.clone())).collect();
        title_page_strings(&parsed.titles, &usfm_names)
    };
    json!({"ok": true, "books": books, "findings": findings, "title1": title1, "title2": title2}).to_string()
}

/// Stage "Assembling content": the interleaver over the typed books. Output as `doc_canon::run_json`
/// (`{ok, key, layout, parts, findings}`) with the parse stage's findings appended, or
/// `{"ok":false,"kind":"violations"|"interleave"|"input",..}`.
pub fn assemble_json(input: &str) -> String {
    let parsed: AssembleIn = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return fail("input", e),
    };
    let mut books = parsed.books.clone();
    if parsed.structure == "as_built" {
        // DOC's markup (HTML and PDF output): the ULB chapters in the sidecar's shape instead of the proposed look's
        if let Some(usfm) = books.get_mut("usfm").and_then(Value::as_array_mut) {
            for book in usfm {
                if let Some(chapters) = book.get_mut("chapters").and_then(Value::as_array_mut) {
                    for chapter in chapters {
                        if let Some(body) = chapter.get_mut(1).and_then(Value::as_object_mut) {
                            if let Some(doc) = body.get("content_doc").cloned() {
                                body.insert("content".into(), doc);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut request = parsed.request.clone();
    if let Some(obj) = request.as_object_mut() {
        // Translation Words in print: link out (the captain's default for the look); an explicit value wins.
        obj.entry("tw_mode").or_insert_with(|| json!("link"));
    }
    let call = json!({"request": request, "books": books, "bug_compat": parsed.bug_compat, "structure": parsed.structure, "unresolved": parsed.unresolved});
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

#[cfg(test)]
mod title_tests {
    use super::*;

    fn dto(lang: &str, name: &str, local: &str, rt: &str, book: &str) -> TitleDto {
        TitleDto { lang_code: lang.into(), lang_name: name.into(), localized_lang_name: local.into(), resource_type_name: rt.into(), book_code: book.into() }
    }

    /// The strings DOC's Playwright tests read from the HTML (frontend/tests/e2e/doc_test_3.ts and doc_test_4.ts).
    #[test]
    fn two_languages_keep_clicked_order() {
        let dtos = [dto("fr", "French", "Français", "Unlocked Literal Bible", "mat"), dto("ceb", "Cebuano", "Cebuano", "Unlocked Literal Bible", "mat")];
        let names = [("fr".to_string(), "mat".to_string(), "Matthieu".to_string()), ("ceb".to_string(), "mat".to_string(), "Mateo".to_string())];
        let (one, two) = title_page_strings(&dtos, &names);
        assert_eq!(one, "French (Français): Unlocked Literal Bible for Matthieu");
        assert_eq!(two, "Cebuano (Cebuano): Unlocked Literal Bible for Mateo");
    }

    #[test]
    fn resource_names_join_and_books_follow_canonical_order() {
        let dtos = [
            dto("en", "English", "English", "Unlocked Literal Bible", "jud"),
            dto("en", "English", "English", "Translation Notes", "jud"),
            dto("en", "English", "English", "Unlocked Literal Bible", "col"),
        ];
        let (one, two) = title_page_strings(&dtos, &[]);
        assert_eq!(one, "English (English): Unlocked Literal Bible, Translation Notes for Colossians, Jude");
        assert_eq!(two, "");
    }

    #[test]
    fn a_whole_testament_is_named_as_one() {
        let mut dtos: Vec<TitleDto> = doc_model::tables::BOOK_NUMBERS.iter().filter(|(_, n)| n.parse::<u32>().unwrap_or(0) >= 40).map(|(c, _)| dto("en", "English", "English", "Unlocked Literal Bible", c)).collect();
        dtos.push(dto("en", "English", "English", "Unlocked Literal Bible", "gen"));
        let (one, _) = title_page_strings(&dtos, &[]);
        assert_eq!(one, "English (English): Unlocked Literal Bible for New Testament, Genesis");
    }
}
