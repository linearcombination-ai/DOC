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

fn doc_poetry_level(marker: &str) -> Option<u32> {
    let base = marker.trim_end_matches(|c: char| c.is_ascii_digit());
    if matches!(base, "q" | "qm" | "qr" | "qc" | "qa" | "qd") {
        let n: u32 = marker[base.len()..].parse().unwrap_or(1);
        Some(n.clamp(1, 3))
    } else {
        None
    }
}

fn doc_heading_level(marker: &str) -> u32 {
    marker.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().unwrap_or(1).clamp(1, 5)
}

/// The chapter in DOC's own markup, the shape its tests and CSS know (the sidecar's, as observed on real output in P3: one `<p>` for the prose
/// of a chapter that has `\p`, `span.verse` holding `sup.versemarker` and the text, `div.poetry-N` for `\q` lines, `div.sectionhead-N` for
/// headings, `div.chapter` with `span.chaptermarker`). Notes are not rendered. This is *compatible* markup, not byte parity: the shapes the sidecar
/// gives footnotes, callers and acrostic headings are not reproduced (see the report).
pub fn chapter_html_doc(c: &ChapterModel) -> String {
    let mut out = format!("<div class=\"chapter\">\n<span class=\"chaptermarker\">{}</span>\n", c.number);
    let mut in_p = false;
    let mut open_verse = false;
    let close_verse = |out: &mut String, open: &mut bool| {
        if *open {
            out.push_str("\n</span>\n");
            *open = false;
        }
    };
    let runs_doc = |runs: &[Run], out: &mut String, open: &mut bool| {
        for r in runs {
            match r {
                Run::Verse(l) => {
                    if *open {
                        out.push_str("\n</span>\n");
                    }
                    out.push_str(&format!("<span class=\"verse\">\n<sup class=\"versemarker\">{}</sup>", esc(l)));
                    *open = true;
                }
                Run::Text(t) => {
                    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        out.push_str(&esc(&t));
                    }
                    if matches!(r, Run::Text(raw) if raw.ends_with(char::is_whitespace)) && !t.is_empty() {
                        out.push(' ');
                    }
                }
                Run::Note(_) => {}
            }
        }
    };
    for b in &c.blocks {
        match b {
            Block::Heading { marker, text } => {
                close_verse(&mut out, &mut open_verse);
                if in_p {
                    out.push_str("</p>\n");
                    in_p = false;
                }
                out.push_str(&format!("<div class=\"sectionhead-{}\">{}</div>\n", doc_heading_level(marker), esc(text.trim())));
            }
            Block::Para { marker, runs } => {
                if marker == "b" {
                    continue;
                }
                if let Some(level) = doc_poetry_level(marker) {
                    close_verse(&mut out, &mut open_verse);
                    if in_p {
                        out.push_str("</p>\n");
                        in_p = false;
                    }
                    out.push_str(&format!("<div class=\"poetry-{level}\">"));
                    runs_doc(runs, &mut out, &mut open_verse);
                    close_verse(&mut out, &mut open_verse);
                    out.push_str("</div>\n");
                } else {
                    if !in_p && c.has_p {
                        out.push_str("<p>\n");
                        in_p = true;
                    }
                    runs_doc(runs, &mut out, &mut open_verse);
                }
            }
        }
    }
    close_verse(&mut out, &mut open_verse);
    if in_p {
        out.push_str("</p>\n");
    }
    out.push_str("</div>\n");
    out
}
