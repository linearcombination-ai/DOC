//! KITCHEN-GAP ADAPTER (P3b, captain 2026-11-25: "no issues to the kitchen team yet; a thin adapter in
//! DOC to address those we can until fixes land").
//!
//! Everything in this file is a **workaround for a gap in scripture-kitchen v0.1.24**. Each one is
//! marked `KITCHEN-GAP[Gn]`, has a switch in [`Gaps`], a test in `harness/tests/p3b.rs`, and a
//! removal note saying what kitchen change makes it unnecessary. Deleting the whole file (and the
//! four call sites, which `grep KITCHEN-GAP` finds) is the intended end state.
//!
//! | id | gap in kitchen | workaround here | remove when kitchen... |
//! |----|----------------|-----------------|------------------------|
//! | G1 | `bttwriter::stitch` writes `\cl \c 1` when a chapter's `title.txt` holds a `\c` marker, which makes onion see a second chapter (`byn__2sa`: 48 chapters instead of 24) | [`sanitize_chapter_titles`] strips backslash markers from `NN/title.txt` before stitching | does not write a `\cl` that contains a marker |
//! | G2 | onion's `Filter::verse_text` keeps `\qa` (poetic acrostic heading, Psalm 119) as verse text | [`verse_filter`] removes `\qa` | treats `\qa` as a heading in `Filter::verse_text` |
//! | G3 | no per-verse text helper over the table of contents and the mask | [`verse_text`] (the slice, 12 lines) | publishes `Toc`-level `verse_text(&toc, &mask, source, anchor)` |
//! | G4 | no lint for a verse number glued to text (`\v 14Ɖeka`) | [`glued_designator_findings`] | adds a `verse-number-glued` lint code |
//!
//! Not kitchen gaps but DOC-side bridges, kept elsewhere and marked there: the names bridge
//! `bridge/doc_book_name_corrections.TEMPORARY-BRIDGE.tsv` (a temporary file, to be upstreamed).

use std::collections::BTreeMap;

use doc_model::model::{Finding, FindingCode, Severity};
use usfm_onion::mask::Mask;
use usfm_onion::{Action, Filter};

/// One switch per workaround; all on by default. Turning one off shows what kitchen alone gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gaps {
    /// G1: strip markers from chapter titles before `bttwriter::stitch`.
    pub cl_marker: bool,
    /// G2: `\qa` is a heading, not verse text.
    pub qa_heading: bool,
    /// G4: report verse numbers glued to text as a finding.
    pub glued_lint: bool,
}

impl Default for Gaps {
    fn default() -> Self {
        Gaps { cl_marker: true, qa_heading: true, glued_lint: true }
    }
}

impl Gaps {
    /// Kitchen as it is: no workaround.
    pub fn none() -> Gaps {
        Gaps { cl_marker: false, qa_heading: false, glued_lint: false }
    }
}

// KITCHEN-GAP[G2] -------------------------------------------------------------------------------
/// The mask filter for verse text: onion's `verse_text` recipe, plus `\qa` removed when `gaps.qa_heading`.
/// A Psalm 119 acrostic heading (the Hebrew letter name) is a heading; DOC's HTML keeps it outside the
/// verse (`span.acrostic-heading`), and the captain's rule is that headings and verses are separate
/// things. Remove when kitchen's `Filter::verse_text` does this itself.
pub fn verse_filter(gaps: &Gaps) -> Filter {
    let mut f = Filter::verse_text();
    if gaps.qa_heading {
        f.markers.push(("qa".to_string(), Action::Remove));
    }
    f
}

// KITCHEN-GAP[G3] -------------------------------------------------------------------------------
/// The text of the bytes `from..to` of the source as the mask reads them, whitespace collapsed.
/// This is the per-verse text helper kitchen does not publish: it is `Mask::project_source` plus a
/// slice of the masked text. Remove when kitchen publishes a verse-text helper over `Toc` and `Mask`.
pub fn verse_text(masked: &Mask, masked_text: &str, from: u32, to: u32) -> String {
    match masked.project_source(from..to.max(from)) {
        Some(r) => masked_text.get(r.start as usize..r.end as usize).unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" "),
        None => String::new(),
    }
}

// KITCHEN-GAP[G4] -------------------------------------------------------------------------------
/// A finding for every chapter that has a verse number glued to text (`\v 14Ɖeka`, `\v 3The`): a segment of
/// more than one character cannot be told from the first word of the verse. One finding per book, with
/// the chapter:verse list in the detail (capped at 20). Remove when kitchen lints this.
pub fn glued_designator_findings(glued: &BTreeMap<u32, Vec<String>>, lang: &str) -> Vec<Finding> {
    let total: usize = glued.values().map(Vec::len).sum();
    if total == 0 {
        return Vec::new();
    }
    let mut at: Vec<String> = Vec::new();
    for (ch, labels) in glued {
        for l in labels {
            if at.len() < 20 {
                at.push(format!("{ch}:{l}"));
            }
        }
    }
    vec![Finding::new(FindingCode::UsfmVerseNumberGlued, Severity::Warning)
        .lang(lang)
        .detail(format!("{total} verse designator(s) carry more than a number: {}{}", at.join(" "), if total > at.len() { " ..." } else { "" }))]
}

// KITCHEN-GAP[G1] -------------------------------------------------------------------------------
/// Hand the Writer files to `bttwriter::stitch` with chapter titles that kitchen cannot turn into a
/// malformed `\cl`: in every `NN/title.txt` a backslash marker (and a following number, as in `\c 1`)
/// is removed. Returns the files and the number of titles changed. Remove when kitchen no longer
/// writes a `\cl` that contains a marker (D38).
pub fn sanitize_chapter_titles(files: &[(String, Vec<u8>)]) -> (Vec<(String, Vec<u8>)>, usize) {
    let mut changed = 0;
    let out = files
        .iter()
        .map(|(path, bytes)| {
            let is_chapter_title = path.ends_with("/title.txt") && !path.starts_with("front/") && path.split('/').next().is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()));
            if is_chapter_title {
                if let Ok(text) = std::str::from_utf8(bytes) {
                    if text.contains('\\') {
                        let cleaned = strip_markers(text);
                        changed += 1;
                        return (path.clone(), cleaned.into_bytes());
                    }
                }
            }
            (path.clone(), bytes.clone())
        })
        .collect();
    (out, changed)
}

/// Remove `\marker` tokens (letters, digits, `*`, `+` after a backslash) and the number that directly
/// follows a `\c` or `\v`, then trim. A title that is only a marker becomes empty (kitchen then writes no `\cl`).
fn strip_markers(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        while let Some(&n) = it.peek() {
            if n.is_ascii_alphanumeric() || n == '*' || n == '+' || n == '-' {
                name.push(n);
                it.next();
            } else {
                break;
            }
        }
        if name == "c" || name == "v" {
            while it.peek().is_some_and(|n| n.is_whitespace()) {
                it.next();
            }
            while it.peek().is_some_and(char::is_ascii_digit) {
                it.next();
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
