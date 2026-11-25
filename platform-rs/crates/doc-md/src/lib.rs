//! doc-md: the markdown readers and everything they need (C9, C10, C11): the Python-`re`
//! compatibility layer, the link transformers, the markdown engine adapter (comrak), the
//! TN/TNC/TQ/TW/BC readers over a [`doc_model::tree::Tree`], and the name localizer.
//! Depends on `doc-model` only. Builds for `wasm32-unknown-unknown`.


pub mod links;
pub mod localize;
pub mod markdown;
pub mod pyre;
pub mod readers;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use doc_model::tree::{Tree, TreeData};
use links::{ReaderCompat, Req};
use localize::LocalizerInput;
use readers::{Dto, ReaderSettings};

#[derive(Deserialize)]
#[serde(untagged)]
enum ReaderCompatIn {
    Named(String),
    Flags(ReaderCompat),
}
impl Default for ReaderCompatIn {
    fn default() -> Self {
        ReaderCompatIn::Named("fixed".into())
    }
}

#[derive(Deserialize)]
struct ReadInput {
    requests: Vec<ReqIn>,
    dtos: Vec<Dto>,
    /// directory name (relative to the assets root) to its listing
    tree: BTreeMap<String, TreeData>,
    #[serde(default)]
    layout_for_print: bool,
    #[serde(default)]
    generate_docx: bool,
    #[serde(default = "yes")]
    show_tn_book_intro: bool,
    #[serde(default)]
    bug_compat: ReaderCompatIn,
    /// option: TN cross-references as working links (default: plain text)
    #[serde(default)]
    xref_links: bool,
    #[serde(default)]
    localizer: LocalizerInput,
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct ReqIn {
    lang_code: String,
    resource_type: String,
    book_code: String,
}

/// Reader and localizer entry point: JSON in, JSON out. `pre` is the typed books before
/// localisation, `post` after (what `assemble_content` receives).
pub fn read_json(input: &str) -> String {
    let parsed: ReadInput = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return json!({"ok": false, "input_error": e.to_string()}).to_string(),
    };
    let compat = match parsed.bug_compat {
        ReaderCompatIn::Named(n) if n == "as_built" => ReaderCompat::as_built(),
        ReaderCompatIn::Named(n) if n == "fixed" => ReaderCompat::fixed(),
        ReaderCompatIn::Named(n) => return json!({"ok": false, "input_error": format!("unknown bug_compat {n}")}).to_string(),
        ReaderCompatIn::Flags(f) => f,
    };
    let mut tree = Tree::default();
    for (dir, data) in &parsed.tree {
        tree.add_dir(dir, data);
    }
    let requests: Vec<Req> = parsed
        .requests
        .into_iter()
        .map(|r| Req { lang_code: r.lang_code, resource_type: r.resource_type, book_code: r.book_code })
        .collect();
    let settings = ReaderSettings {
        layout_for_print: parsed.layout_for_print,
        generate_docx: parsed.generate_docx,
        show_tn_book_intro: parsed.show_tn_book_intro,
        compat,
        xref_links: parsed.xref_links,
    };
    let fail = |e: links::PyError| json!({"ok": false, "error": e.to_string()}).to_string();
    let pre = match readers::read_books(&parsed.dtos, &requests, &tree, &settings) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let (mut tn, mut tnc, mut tq) = (pre.tn.clone(), pre.tnc.clone(), pre.tq.clone());
    let name_findings = match localize::localize(&parsed.localizer, &mut tn, &mut tnc, &mut tq) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    json!({
        "ok": true,
        "pre": {"tn": pre.tn, "tnc": pre.tnc, "tq": pre.tq, "tw": pre.tw, "bc": pre.bc},
        "post": {"tn": tn, "tnc": tnc, "tq": tq, "tw": pre.tw, "bc": pre.bc},
        "name_findings": name_findings,
        "xref_findings": pre.findings,
    })
    .to_string()
}
