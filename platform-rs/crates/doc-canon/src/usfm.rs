//! The USFM model through kitchen's onion (P3).
//!
//! DOC's USFM parser, as built, sends each chapter's text to a .NET sidecar for HTML, then a
//! BeautifulSoup pass (`clean_chapter_html`, `split_into_verses`) turns the HTML into
//! `USFMChapter.content` and `USFMChapter.verses` (`parsing.py:496-588,1446-1587`). This module
//! produces the same structures from onion's token stream, table of contents and text mask,
//! without a second parser and without HTML as an intermediate: a verse is a byte range, its
//! text is the masked bytes in it, its label is the designator as written.
//!
//! What the stable surface this needs from kitchen is: `lex`, `cst::build`, `toc` (chapter
//! rows, verse anchors, designator label spans), `mask` with `Filter::verse_text`, `lint`
//! (codes and names), `usj` for the structural renderer, and the `\id`/`\h`/`\mt`/`\toc*` text.
//! Only this file names onion types.

use std::collections::BTreeMap;

use crate::kitchen_gaps::{self, Gaps};
use doc_model::model::{Finding, FindingCode, Severity};
use usfm_onion::lint::lint;
use usfm_onion::usj::usj;
use usfm_onion::{cst, lex, mask, toc};

/// One verse as the model holds it: the label as written (`1`, `3-4`, `5a`, `1,3`), the hull
/// onion read from the designator, and the plain verse text (notes, headings and markers removed;
/// whitespace collapsed, ends trimmed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseModel {
    /// The designator as onion reads it (`5a`, `3-4`, `14Ɖeka` for a missing space).
    pub label: String,
    /// The label as DOC's sidecar path produces it: the leading number or `N-M` range; anything
    /// after it (a segment letter, or the first word of a verse written without a space) is
    /// moved into the text. Equal to `label` for ordinary verses.
    pub label_doc: String,
    pub first: u16,
    pub last: u16,
    /// Verse text as onion's mask reads it (notes, headings and markers removed).
    pub text: String,
    /// `text` with the designator's remainder put back in front, as DOC's path has it.
    pub text_doc: String,
    /// The designator carried more than a number or range (a finding is due).
    pub glued: bool,
    /// P3b (captain: "at the object level headings and verses should be kept separate things"):
    /// the section headings that follow this verse in the source, before the next `\v`. They are NOT
    /// part of `text` or `text_doc`. DOC's as-built path puts them inside the verse (defect D34);
    /// [`VerseModel::as_built_text`] reproduces that for the oracle comparison.
    pub headings_after: Vec<String>,
}

impl VerseModel {
    /// The verse text as DOC's sidecar path leaves it: `text_doc` with the following section
    /// heading(s) glued on (defect D34). Only the as_built oracle comparisons use this.
    pub fn as_built_text(&self) -> String {
        let mut t = self.text_doc.clone();
        for h in &self.headings_after {
            if !t.is_empty() {
                t.push(' ');
            }
            t.push_str(h);
        }
        t
    }
}

/// One run of a paragraph: a verse number, text, or a note (footnote or cross reference text,
/// kept apart from the prose).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    Verse(String),
    Text(String),
    Note(String),
}

/// A structural block of a chapter in source order. Headings are their own objects (P3b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A paragraph or poetry line (`p`, `m`, `q`, `q1`, `q2`, `b`, `li`, ...), its marker and runs.
    Para { marker: String, runs: Vec<Run> },
    /// A section heading or related line (`s`, `s1`..`s5`, `ms`, `mr`, `r`, `sp`, `d`), its marker and text.
    Heading { marker: String, text: String },
}

/// `\s`, `\s1`..`\s5`, `\ms`, `\mr`, `\r`, `\sr`, `\sp`: DOC's HTML gives these a `div.sectionhead-N`
/// or similar and (as built) nests them in the previous verse's span.
pub fn is_heading_marker(m: &str) -> bool {
    let base = m.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(base, "s" | "ms" | "mr" | "r" | "sr" | "sp" | "d" | "cd")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterModel {
    /// The chapter number onion read; 0 when the `\c` designator is malformed (`\c 12b`).
    pub number: u32,
    /// The designator as written.
    pub label: String,
    pub verses: Vec<VerseModel>,
    /// Text of a `\d` title (verse 0's slot), when the chapter has one.
    pub title: Option<String>,
    /// the chapter's source has a `\p` paragraph (DOC's HTML then wraps the chapter's verses in one `<p>`)
    pub has_p: bool,
    /// The chapter's paragraphs and headings in source order (P3b), for renderers that keep the
    /// structure; built from onion's USJ.
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Headers {
    pub h: Option<String>,
    pub mt: Option<String>,
    pub toc1: Option<String>,
    pub toc2: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UsfmModel {
    pub book: String,
    pub headers: Headers,
    pub chapters: Vec<ChapterModel>,
    /// onion lint findings: code name to count (kebab-case names as kitchen publishes them).
    pub lint: BTreeMap<String, u32>,
    pub bytes: usize,
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// DOC's `extract_usfm_frontmatter` patterns: the marker's value is the rest of its line, up to
/// a following backslash marker; the marker must be followed by a space or tab.
fn header_value(front: &str, marker: &str) -> Option<String> {
    let needle = format!("\\{marker}");
    let mut from = 0;
    while let Some(i) = front[from..].find(&needle) {
        let at = from + i + needle.len();
        from = at;
        let rest = &front[at..];
        let mut chars = rest.chars();
        match chars.next() {
            Some(' ' | '\t') => {}
            _ => continue,
        }
        let after_ws = rest.trim_start_matches([' ', '\t']);
        let line = after_ws.split('\n').next().unwrap_or("");
        // lazy `(.*?)(?=\s+\\|\n|$)`: stop at whitespace followed by a backslash
        let mut end = line.len();
        let bytes = line.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j].is_ascii_whitespace() {
                let mut k = j;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b'\\' {
                    end = j;
                    break;
                }
                j = k;
            } else {
                j += 1;
            }
        }
        let v = line[..end].trim();
        return Some(v.to_string());
    }
    None
}

/// DOC's reading of a verse designator: the leading `N` or `N-M`, and the rest.
fn split_designator(label: &str) -> (String, &str) {
    let digits = |s: &str| s.chars().take_while(char::is_ascii_digit).map(char::len_utf8).sum::<usize>();
    let a = digits(label);
    if a == 0 {
        return (String::new(), label);
    }
    let mut end = a;
    if label[a..].starts_with('-') {
        let b = digits(&label[a + 1..]);
        if b > 0 {
            end = a + 1 + b;
        }
    }
    (label[..end].to_string(), &label[end..])
}

/// Parse one book's USFM text with the kitchen-gap workarounds on.
pub fn parse_book(source: &str) -> UsfmModel {
    parse_book_with(source, &Gaps::default())
}

/// The chapter blocks from onion's USJ, one entry per `\c` in source order.
fn usj_blocks(bytes: &[u8], tokens: &[usfm_onion::Token], tree: &usfm_onion::cst::Cst) -> Vec<Vec<Block>> {
    use serde_json::Value;
    fn runs(node: &Value, out: &mut Vec<Run>, in_note: bool) {
        match node {
            Value::String(t) => {
                if !t.is_empty() {
                    out.push(if in_note { Run::Note(t.clone()) } else { Run::Text(t.clone()) });
                }
            }
            Value::Object(o) => {
                let ty = o.get("type").and_then(Value::as_str).unwrap_or("");
                if ty == "verse" {
                    out.push(Run::Verse(o.get("number").and_then(Value::as_str).unwrap_or("").to_string()));
                    return;
                }
                let note = in_note || ty == "note" || o.get("marker").and_then(Value::as_str).is_some_and(|m| matches!(m, "f" | "fe" | "x"));
                if let Some(Value::Array(c)) = o.get("content") {
                    for n in c {
                        runs(n, out, note);
                    }
                }
            }
            _ => {}
        }
    }
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(&usj(bytes, tokens, tree)) else { return Vec::new() };
    let mut chapters: Vec<Vec<Block>> = Vec::new();
    let mut seen_chapter = false;
    if let Some(Value::Array(content)) = root.get("content") {
        for node in content {
            // onion's USJ floats verses and text that follow an empty heading paragraph (`\s5`) at the
            // top level: they continue the current paragraph (marker ""), no paragraph break
            let floating = match node {
                Value::String(_) => true,
                Value::Object(o) => matches!(o.get("type").and_then(Value::as_str), Some("verse" | "char" | "note")),
                _ => false,
            };
            if floating && seen_chapter {
                let mut rs = Vec::new();
                runs(node, &mut rs, false);
                if let Some(last) = chapters.last_mut() {
                    match last.last_mut() {
                        Some(Block::Para { marker, runs: r }) if marker.is_empty() => r.extend(rs),
                        _ => last.push(Block::Para { marker: String::new(), runs: rs }),
                    }
                }
                continue;
            }
            let Value::Object(o) = node else { continue };
            match o.get("type").and_then(Value::as_str) {
                Some("chapter") => {
                    chapters.push(Vec::new());
                    seen_chapter = true;
                }
                Some("para") if seen_chapter => {
                    let marker = o.get("marker").and_then(Value::as_str).unwrap_or("").to_string();
                    let mut rs = Vec::new();
                    if let Some(Value::Array(c)) = o.get("content") {
                        for n in c {
                            runs(n, &mut rs, false);
                        }
                    }
                    let Some(last) = chapters.last_mut() else { continue };
                    if is_heading_marker(&marker) {
                        // onion nests the verses that follow a heading (`\s5` chunk markers especially) in
                        // the heading's own paragraph: the heading text is what comes before the first
                        // verse; what follows is body text, a continuation ("" marker: no paragraph break)
                        let cut = rs.iter().position(|r| matches!(r, Run::Verse(_))).unwrap_or(rs.len());
                        let rest = rs.split_off(cut);
                        let text = rs.iter().filter_map(|r| if let Run::Text(t) = r { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join(" ");
                        last.push(Block::Heading { marker, text: collapse(&text) });
                        if !rest.is_empty() {
                            last.push(Block::Para { marker: String::new(), runs: rest });
                        }
                    } else {
                        last.push(Block::Para { marker, runs: rs });
                    }
                }
                _ => {}
            }
        }
    }
    chapters
}

/// Parse one book's USFM text, with the kitchen-gap workarounds chosen by `gaps`.
pub fn parse_book_with(source: &str, gaps: &Gaps) -> UsfmModel {
    let bytes = source.as_bytes();
    let tokens = lex(source);
    let tree = cst::build(&tokens);
    let table = toc(bytes, &tokens);
    let masked = mask(bytes, &tokens, &tree, &kitchen_gaps::verse_filter(gaps));
    let masked_text = masked.text(bytes);
    let mut blocks_by_chapter = usj_blocks(bytes, &tokens, &tree).into_iter();

    let mut model = UsfmModel { bytes: bytes.len(), ..Default::default() };
    model.book = String::from_utf8_lossy(&table.book).trim_matches('\0').to_string();

    // front matter: chapter row 0
    if let Some(row0) = table.chapters.first() {
        let front = &source[row0.start as usize..row0.end as usize];
        model.headers = Headers {
            h: header_value(front, "h"),
            mt: header_value(front, "mt"),
            toc1: header_value(front, "toc1"),
            toc2: header_value(front, "toc2"),
        };
    }

    let slice = |from: u32, to: u32| -> String { kitchen_gaps::verse_text(&masked, &masked_text, from, to) };

    for (ci, row) in table.chapters.iter().enumerate().skip(1) {
        let verses_here: Vec<_> = table.verses.iter().filter(|v| usize::from(v.chapter) == usize::from(row.number) && v.at >= row.start && v.at < row.end).collect();
        let mut verses = Vec::with_capacity(verses_here.len());
        for (i, v) in verses_here.iter().enumerate() {
            let end = verses_here.get(i + 1).map_or(row.end, |n| n.at);
            let label = source[v.label_start as usize..v.label_end as usize].trim().to_string();
            let text = slice(v.at, end);
            let (label_doc, rest) = split_designator(&label);
            let text_doc = if rest.is_empty() { text.clone() } else if text.is_empty() { rest.to_string() } else { format!("{rest} {text}") };
            verses.push(VerseModel { glued: !rest.is_empty(), label_doc, label, first: v.first, last: v.last, text, text_doc, headings_after: Vec::new() });
        }
        let title = table.titles.iter().find(|t| t.at >= row.start && t.at < row.end).map(|t| collapse(&source[t.at as usize..t.end as usize]));
        let label = source[row.label_start as usize..row.label_end as usize].trim().to_string();
        let _ = ci;
        let body = &source[row.start as usize..row.end as usize];
        let has_p = body.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("\\p") && t[2..].chars().next().is_none_or(char::is_whitespace)
        });
        let blocks = blocks_by_chapter.next().unwrap_or_default();
        // headings stay separate objects; remember which verse each follows (D34's as-built glue)
        let mut vi: Option<usize> = None;
        // a heading with no verse after it in the chapter (`\s` just before the next `\c`) belongs to the next
        // chapter: DOC does not glue it onto the last verse, and neither does the model
        let last_verse_block = blocks.iter().rposition(|b| matches!(b, Block::Para { runs, .. } if runs.iter().any(|r| matches!(r, Run::Verse(_)))));
        for (bi, b) in blocks.iter().enumerate() {
            match b {
                Block::Para { runs, .. } => {
                    for r in runs {
                        if let Run::Verse(l) = r {
                            vi = verses.iter().position(|v: &VerseModel| v.label == *l).or(vi);
                        }
                    }
                }
                Block::Heading { marker, text } => {
                    // measured against DOC (P3b): the sidecar nests `\s`/`\s1`..`\s5` in the previous verse;
                    // `\d`, `\ms`, `\mr`, `\r`, `\sp` it keeps outside the verse
                    let glued_by_doc = marker.trim_end_matches(|c: char| c.is_ascii_digit()) == "s";
                    if let (Some(i), false, true, true) = (vi, text.is_empty(), glued_by_doc, last_verse_block.is_some_and(|l| bi < l)) {
                        verses[i].headings_after.push(text.clone());
                    }
                }
            }
        }
        model.chapters.push(ChapterModel { number: u32::from(row.number), label, verses, title, has_p, blocks });
    }

    let report = lint(bytes, &tokens, &tree);
    for o in &report.observations {
        *model.lint.entry(o.code.row().name.to_string()).or_default() += 1;
    }
    model
}

/// Findings a caller returns with the book (never silent loss): one per lint code with its count,
/// plus the structural facts DOC's sidecar path turns into empty chapters or crashes.
pub fn findings(model: &UsfmModel, lang: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    if model.book.is_empty() {
        out.push(Finding::new(FindingCode::UsfmBookIdMissing, Severity::Info).lang(lang));
    }
    for ch in &model.chapters {
        if ch.number == 0 {
            out.push(Finding::new(FindingCode::UsfmChapterNumberMalformed, Severity::Warning).lang(lang).detail(format!("\\c {}", ch.label)));
        } else if ch.verses.is_empty() {
            out.push(Finding::new(FindingCode::UsfmChapterEmpty, Severity::Warning).lang(lang).chapter(ch.number));
        }
    }
    for (code, n) in &model.lint {
        // the noisy structural codes are reported once per book as counts, never per occurrence
        out.push(Finding::new(FindingCode::UsfmLint, Severity::Info).lang(lang).detail(format!("{code} x{n}")));
    }
    // KITCHEN-GAP[G4]: the lint kitchen does not have
    let glued: BTreeMap<u32, Vec<String>> = model
        .chapters
        .iter()
        .filter_map(|c| {
            let g: Vec<String> = c.verses.iter().filter(|v| v.glued).map(|v| v.label.clone()).collect();
            if g.is_empty() { None } else { Some((c.number, g)) }
        })
        .collect();
    out.extend(kitchen_gaps::glued_designator_findings(&glued, lang));
    out
}

// ---------------------------------------------------------------------------------
// DOC's HTML shape, from the model (what `clean_chapter_html` / `split_into_verses` leave)
// ---------------------------------------------------------------------------------

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `re.sub(r"\s*-\s*", "-", s)`
fn tighten_hyphens(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        // leftmost match: the run of spaces before a hyphen, the hyphen, the spaces after
        let mut j = i;
        while j < cs.len() && py_space(cs[j]) {
            j += 1;
        }
        if j < cs.len() && cs[j] == '-' {
            let mut k = j + 1;
            while k < cs.len() && py_space(cs[k]) {
                k += 1;
            }
            out.push('-');
            i = k;
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

/// `re.sub(r"\s+([,;.])", r"\1", s)`
fn no_space_before_punct(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        if py_space(cs[i]) {
            let mut j = i;
            while j < cs.len() && py_space(cs[j]) {
                j += 1;
            }
            if j < cs.len() && matches!(cs[j], ',' | ';' | '.') {
                out.push(cs[j]);
                i = j + 1;
                continue;
            }
            out.extend(cs[i..j].iter());
            i = j;
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

/// `re.sub(r"([,;])(?!\d)(?!\s)", r"\1 ", s)`
fn space_after_comma(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in cs.iter().enumerate() {
        out.push(*c);
        if matches!(c, ',' | ';') {
            // (?!\d)(?!\s): holds at the end of the string as well
            let next = cs.get(i + 1);
            if next.is_none_or(|n| !n.is_ascii_digit() && !py_space(*n)) {
                out.push(' ');
            }
        }
    }
    out
}

/// `re.sub(r"'\s*", "'", s)`
fn tighten_apostrophes(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        out.push(cs[i]);
        if cs[i] == '\'' {
            i += 1;
            while i < cs.len() && py_space(cs[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// DOC's `_normalize_verse_interior` for one caller-free run of text (`parsing.py:1459-1476`).
pub fn doc_interior(text: &str) -> String {
    // `\n+` to space, strip, `  +` to one space (Python `str.strip()` strips whitespace)
    let mut s = String::with_capacity(text.len());
    let mut prev_nl = false;
    for c in text.chars() {
        if c == '\n' {
            if !prev_nl {
                s.push(' ');
            }
            prev_nl = true;
        } else {
            prev_nl = false;
            s.push(c);
        }
    }
    let s = s.trim_matches(py_space).to_string();
    let mut collapsed = String::with_capacity(s.len());
    let mut run = 0;
    for c in s.chars() {
        if c == ' ' {
            run += 1;
            if run <= 1 {
                collapsed.push(c);
            }
        } else {
            run = 0;
            collapsed.push(c);
        }
    }
    let s = tighten_hyphens(&collapsed);
    let s = no_space_before_punct(&s);
    let s = space_after_comma(&s);
    tighten_apostrophes(&s)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `USFMChapter.verses[label]` for a verse with no markup of its own: `<span class="verse">`,
/// the normalised text on its own line, `</span>` (the versemarker `<sup>` is removed, as
/// `split_into_verses` does).
pub fn verse_html(text: &str) -> String {
    format!("<span class=\"verse\">\n{}\n</span>", escape(&doc_interior(text)))
}

/// `USFMChapter.content` for a chapter of plain prose (no poetry, headings or notes): what the
/// sidecar and `clean_chapter_html` leave. Anything with markup of its own needs the mapping
/// pass from USJ (see the report): this is the part that does not.
pub fn chapter_html_plain(number: u32, verses: &[VerseModel], has_p: bool) -> String {
    let mut out = format!("<div class=\"chapter\">\n<span class=\"chaptermarker\">{number}</span>\n{}", if has_p { "<p>\n" } else { "" });
    for v in verses {
        out.push_str(&format!("<span class=\"verse\">\n<sup class=\"versemarker\">{}</sup>{}\n</span>\n", escape(&v.label_doc), escape(&doc_interior(&v.text_doc))));
    }
    out.push_str(if has_p { "</p>\n</div>\n" } else { "</div>\n" });
    out
}
