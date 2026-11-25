//! Cross-reference links in the finished document (P3, captain's TN-links question).
//!
//! The readers emit `<a class="doc-xref" href="#ID">text</a>` and give the target note's
//! heading `id="ID"`. Whether the target survives interleaving depends on the strategy (notes
//! for verses the scripture lacks are dropped, a chapter may be filtered), so the check belongs
//! on the finished parts: a link whose id no part defines is turned back into its text and
//! reported (`XrefDeadInOutput`). No regex engine: the shapes are fixed by the readers.

use std::collections::BTreeSet;

use doc_model::model::{Finding, FindingCode, Part, Severity, XREF_CLASS};

fn opener() -> String {
    format!("<a class=\"{XREF_CLASS}\" href=\"#")
}

/// Every `id="..."` value in the text.
fn defined_ids(text: &str, out: &mut BTreeSet<String>) {
    let mut rest = text;
    while let Some(i) = rest.find(" id=\"") {
        let after = &rest[i + 5..];
        match after.find('"') {
            Some(j) => {
                out.insert(after[..j].to_string());
                rest = &after[j..];
            }
            None => break,
        }
    }
}

/// Resolve the links of all parts against the ids of all parts. Returns the number of live
/// links and the findings for the dead ones (each dead target once).
pub fn resolve(parts: &mut [Part]) -> (usize, Vec<Finding>) {
    let mut ids = BTreeSet::new();
    for p in parts.iter() {
        defined_ids(&p.content, &mut ids);
    }
    let open = opener();
    let (mut live, mut findings) = (0usize, Vec::new());
    for p in parts.iter_mut() {
        if !p.content.contains(&open) {
            continue;
        }
        let mut out = String::with_capacity(p.content.len());
        let mut rest = p.content.as_str();
        while let Some(i) = rest.find(&open) {
            out.push_str(&rest[..i]);
            let after = &rest[i + open.len()..];
            let (Some(q), Some(close)) = (after.find("\">"), after.find("</a>")) else {
                out.push_str(&rest[i..i + open.len()]);
                rest = after;
                continue;
            };
            let id = &after[..q];
            let text_start = q + 2;
            if ids.contains(id) && text_start <= close {
                live += 1;
                out.push_str(&rest[i..i + open.len() + close + 4]);
            } else {
                out.push_str(&after[text_start.min(close)..close]);
                let f = Finding::new(FindingCode::XrefDeadInOutput, Severity::Info).detail(format!("#{id}"));
                if !findings.contains(&f) {
                    findings.push(f);
                }
            }
            rest = &after[close + 4..];
        }
        out.push_str(rest);
        p.content = out;
    }
    (live, findings)
}
