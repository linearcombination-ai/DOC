//! The proposed look, ULB side (P3b): the model's chapter blocks rendered as semantic HTML classes that the
//! shared style table styles, instead of the sidecar's shape (`div.poetry-1`, one `<p>` per chapter, ...).
//!
//! Classes (chosen from the sample, `proposed-doc-format.docx`, chapters 1 and 2; see the report):
//! * `p.ulb-chapter-num`: the chapter number, a 2-line drop cap in the sample (`w:framePr dropCap`);
//! * `p.ulb-text`: running prose; verse numbers are `sup.vn`;
//! * `p.ulb-poetry-1`, `p.ulb-poetry-2`: poetry lines (`\q`/`\q1` and `\q2`), first line indent 0.5 in for level 2.
//! The sample collapses all `\p` of a run of prose into one paragraph and breaks paragraphs only at poetry;
//! that is what `UlbOptions::collapse_paragraphs` (default on) reproduces. Turning it off honours each `\p`/`\m`.

use crate::usfm::{Block, ChapterModel, Run};

#[derive(Clone, Copy, Debug)]
pub struct UlbOptions {
    /// One `p.ulb-text` per run of prose between poetry blocks (the sample), not one per `\p`.
    pub collapse_paragraphs: bool,
    /// Emit the chapter number as its own `p.ulb-chapter-num` (a drop cap in the sample).
    pub chapter_number: bool,
}

impl Default for UlbOptions {
    fn default() -> Self {
        UlbOptions { collapse_paragraphs: true, chapter_number: true }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn poetry_level(marker: &str) -> Option<u32> {
    let base = marker.trim_end_matches(|c: char| c.is_ascii_digit());
    if matches!(base, "q" | "qm" | "qr" | "qc" | "qa" | "qd") {
        let n: u32 = marker[base.len()..].parse().unwrap_or(1);
        Some(n.clamp(1, 2))
    } else {
        None
    }
}

fn runs_html(runs: &[Run], tab_after_first_verse: bool) -> String {
    let mut out = String::new();
    let mut first = true;
    for r in runs {
        match r {
            Run::Verse(l) => {
                out.push_str(&format!("<sup class=\"vn\">{}</sup>", esc(l)));
                if first && tab_after_first_verse {
                    out.push('\t');
                }
            }
            Run::Text(t) => {
                let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
                if !t.is_empty() {
                    let lead = if first && tab_after_first_verse { "" } else { " " };
                    let _ = lead;
                    out.push_str(&esc(&t));
                }
                // keep the single space USFM puts between a text run and the next verse number
                if matches!(r, Run::Text(raw) if raw.ends_with(char::is_whitespace)) {
                    out.push(' ');
                }
            }
            Run::Note(_) => {}
        }
        first = false;
    }
    out
}

/// The chapter as semantic HTML. Section headings are objects of their own in the model and are not part of
/// the ULB text block (the sample shows none inside the ULB section).
pub fn ulb_chapter_html(c: &ChapterModel, o: &UlbOptions) -> String {
    let mut out = String::new();
    if o.chapter_number {
        out.push_str(&format!("<p class=\"ulb-chapter-num\">{}</p>\n", c.number));
    }
    let mut prose: Vec<String> = Vec::new();
    let flush = |prose: &mut Vec<String>, out: &mut String| {
        if !prose.is_empty() {
            out.push_str(&format!("<p class=\"ulb-text\">{}</p>\n", prose.concat().trim()));
            prose.clear();
        }
    };
    for b in &c.blocks {
        let Block::Para { marker, runs } = b else { continue };
        if marker == "b" {
            continue;
        }
        if let Some(level) = poetry_level(marker) {
            flush(&mut prose, &mut out);
            out.push_str(&format!("<p class=\"ulb-poetry-{level}\">{}</p>\n", runs_html(runs, true).trim_end()));
            continue;
        }
        let html = runs_html(runs, false);
        if html.trim().is_empty() {
            continue;
        }
        // The sample (and DOC's sidecar) break a paragraph only where a `\p` starts text that continues the
        // previous verse (`\p` + text, no verse number): `\p` followed by a verse number merges into the
        // running paragraph (measured: chapter 1, verse 6, and chapter 2's seven `\p`)
        let starts_with_verse = runs.iter().find(|r| !matches!(r, Run::Text(t) if t.trim().is_empty())).is_some_and(|r| matches!(r, Run::Verse(_)));
        if marker.is_empty() || (o.collapse_paragraphs && (starts_with_verse || prose.is_empty())) {
            prose.push(html);
        } else {
            flush(&mut prose, &mut out);
            prose.push(html);
            if !o.collapse_paragraphs {
                flush(&mut prose, &mut out);
            }
        }
    }
    flush(&mut prose, &mut out);
    out
}
