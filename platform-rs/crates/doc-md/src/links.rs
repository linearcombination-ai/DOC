//! C10 `md-links`: DOC's link transformers (`markdown_transforms/link_regexes.py` and
//! `markdown_transformer.py`) ported function by function, in DOC's order, with the
//! as-built behaviours kept (each transformer re-scans the ORIGINAL text for matches and
//! applies a global literal `str.replace` to the CURRENT text; see [`replace_all`]).
//!
//! The only I/O DOC does here is `exists(path)` on translation-note files and reading a
//! translation-word file's first line; both go through [`Tree`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::pyre::{Match, PyRegex};
use doc_model::model::{Finding, FindingCode, Severity};
use doc_model::tables;
use doc_model::tree::{py_join, Tree};

/// A Python exception DOC would raise (kept as data so equivalence can be tested).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyError {
    pub kind: &'static str,
    pub message: String,
}
impl PyError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> PyError {
        PyError { kind, message: message.into() }
    }
}
impl std::fmt::Display for PyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl From<String> for PyError {
    fn from(message: String) -> PyError {
        PyError { kind: "re.error", message }
    }
}

pub type Res<T> = Result<T, PyError>;

/// `(lang_code, resource_type, book_code)` as the reader sees a `ResourceRequest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Req {
    pub lang_code: String,
    pub resource_type: String,
    pub book_code: String,
}

/// Reader-side as-built behaviours that are defects (flag on = DOC as built).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct ReaderCompat {
    /// D3: `exists()` and `tw_resource_dir()` assume a nested clone layout
    /// (`<assets>/en_tn/en_tn/...`) although DOC clones flat (`<assets>/en_tn`).
    pub nested_clone_layout: bool,
    /// D24: `re.sub("h1", "h5", html)` rewrites the substring `h1` anywhere, not only tags.
    pub heading_substring_replace: bool,
}
impl ReaderCompat {
    pub fn as_built() -> ReaderCompat {
        ReaderCompat { nested_clone_layout: true, heading_substring_replace: true }
    }
    pub fn fixed() -> ReaderCompat {
        ReaderCompat { nested_clone_layout: false, heading_substring_replace: false }
    }
}

/// Everything the transformers need beyond the text.
pub struct LinkEnv<'a> {
    pub requests: &'a [Req],
    /// `translation_words_dict`: file stem to path (relative to the assets root).
    pub tw_words: &'a BTreeMap<String, String>,
    pub tree: &'a Tree,
    pub compat: ReaderCompat,
    /// resource dirs by request (needed only by the `fixed` TN existence check)
    pub dirs: &'a [(Req, String)],
    /// Option (not a defect flag): TN cross-references become working links. The existence
    /// check then uses the real clone layout whatever `compat.nested_clone_layout` says, and
    /// a reference that cannot become a link is recorded in `findings`.
    pub xref_links: bool,
    pub findings: Rc<RefCell<Vec<Finding>>>,
}

impl<'a> LinkEnv<'a> {
    pub fn new(requests: &'a [Req], tw_words: &'a BTreeMap<String, String>, tree: &'a Tree, compat: ReaderCompat, dirs: &'a [(Req, String)]) -> LinkEnv<'a> {
        LinkEnv { requests, tw_words, tree, compat, dirs, xref_links: false, findings: Rc::new(RefCell::new(Vec::new())) }
    }
    fn note_missing(&self, lang: &str, book: &str, chapter: &str, verse: &str, why: &str) {
        if self.xref_links {
            let f = Finding::new(FindingCode::XrefTargetMissing, Severity::Info)
                .lang(lang)
                .detail(format!("{book} {chapter}:{verse}: {why}"));
            let mut list = self.findings.borrow_mut();
            if !list.contains(&f) {
                list.push(f);
            }
        }
    }
}

// ---------------------------------------------------------------------------------
// the regex table: every pattern of link_regexes.py, compiled once
// ---------------------------------------------------------------------------------

macro_rules! pat {
    ($name:ident, $ic:expr, $src:expr) => {
        fn $name() -> &'static PyRegex {
            static CELL: OnceLock<PyRegex> = OnceLock::new();
            CELL.get_or_init(|| PyRegex::new($src, $ic).expect(concat!("pattern ", stringify!($name))))
        }
    };
}

pat!(tw_rc_link, false, r"rc:\/\/(?P<lang_code>[^\[\]\(\)]+?)\/tw\/dict\/bible\/(?:kt|names|other)\/(?P<word>[^\[\]\(\)]+?)$");
pat!(tw_obe_rc_link, false, r"\[\[rc:\/\/(?P<lang_code>[^\[\]\(\)\/]+?)\/obe\/(?:kt|names|other)\/(?P<word>[^\[\]\(\)]+?)\]\]");
pat!(rc_question_link, false, r"\[\[rc:\/\/(?P<lang_code>[^\[\]\(\)\/]+)\/bible\/questions\/comprehension\/(?P<book>[^\[\]\(\)\/]+)\/(?P<chapter>[^\[\]\(\)\/]+)\]\]");
pat!(tw_star_rc_link, false, r"\[\[rc://[^/]+/[^/]+/[^/]+/[^/]+/(?P<word>[^/]+)\]\]");
pat!(tw_wiki_rc_link, false, r"\[\[rc:\/\/\*\/tw\/dict\/bible\/(?:kt|names|other)\/(?P<word>[^\]]+?)\]\]");
pat!(tw_wiki_rc_link2, false, r"\[\[rc:\/\/\*\/tw\/bible\/(?:kt|names|other)\/(?P<word>[^\]]+?)\]\]");
pat!(tw_wiki_prefixed_rc_link, false, r"\((?P<prefix_text>.+?):* *\[\[rc://(?P<lang_code>[^\[\]]+?)\/tw\/dict\/bible\/(?:kt|names|other)\/(?P<word>[^\[\]]+?)\]\]\)*");
pat!(tw_wiki_prefixed_rc_link_no_close_paren, false, r"\((?P<prefix_text>.+?):* *\[\[rc://(?P<lang_code>[^\[\]]+?)\/tw\/dict\/bible\/(?:kt|names|other)\/(?P<word>[^)\[\]]+?)\]\][^)]");
pat!(tw_markdown_link, false, r"\[(?P<link_text>[^\]]+?)\]\(\.*/(?:kt|names|other)/(?P<word>[^\)]+?)\.md\)");
pat!(ta_wiki_prefixed_rc_link, false, r"\((?P<prefix_text>.+?):* *\[\[rc://(?P<lang_code>[^\[\]\(\)]+?)\/ta\/man\/.+?\/(?P<word>[^\[\]]+?)\]\]\)");
pat!(ta_wiki_rc_link, false, r",* *\[\[rc://(?P<lang_code>[^\[\]\(\)]+?)\/ta\/man\/\w+?\/(?P<word>[^\[\]]+?)\]\]\)*");
pat!(ta_star_rc_link, false, r"\[\[rc://\*\/ta\/man\/[^\]]+\]\]");
pat!(ta_prefixed_markdown_link, false, r"\(.+?:* *\[(?P<link_text>[^\[\]\(\)]*?)\] *\(rc:\/\/(?P<lang_code>[^\[\]\(\)]+?)\/ta\/man\/translate\/(?P<word>[^\[\]\(\)]+?)(.md)*\) *\),*.*");
pat!(ta_prefixed_markdown_https_link, false, r"\(.+?:* *\[(?P<link_text>.*?)\] *\(https:\/\/(?P<domain>.+?)\/.+?-ta-.*?\/(?P<work>.+?).md\)\),*.*");
pat!(ta_markdown_https_link, false, r"\[(?P<link_text>.*?)\] *\(https:\/\/(?P<domain>.+?)\/.+?-ta-.*?\/(?P<work>.+?).md\),*.*");
pat!(tn_markdown_scripture_link, false, r"\[(?P<scripture_ref>.+?)\]\(rc:\/\/(?P<lang_code>.+?)\/tn\/help\/(?P<book_code>(?!obs).+?)\/(?P<chapter_num>\d+?)\/(?P<verse_ref>.+?)\)");
pat!(markdown_link, false, r"(?<!\\)\[(?P<link_text>.+?)\]\((?P<url>.+?)\)");
pat!(wiki_link, false, r"\[\[(?P<url>[^\]]+)\]\]");
pat!(tn_markdown_relative_scripture_link, false, r"\(\[(?P<scripture_ref>.+?)\]\((\.\.\/)+(?P<book_code>\w+?)\/(?P<chapter_num>\d+?)\/(?P<verse_ref>.+?).md\)\)");
pat!(tn_relative_to_current_book, false, r"\(\[(?P<scripture_ref>.+?)\]\((?:\.\.\/)+(?P<chapter_num>\d+?)\/(?P<verse_ref>.+?).md\)\)");
pat!(tn_relative_to_current_book_no_parens, false, r"\[(?P<scripture_ref>.+?)\]\((?:\.\.\/)+(?P<chapter_num>\d+?)\/(?P<verse_ref>.+?).md\)");
pat!(tn_relative_to_current_chapter_no_parens, false, r"\[(?P<scripture_ref>.+?)\]\((?:\.\/)+(?P<verse_ref>.+?).md\)");
pat!(tn_relative_scripture_no_parens, false, r"\[(?P<scripture_ref>.+?)\]\((\.\.\/)+(?P<book_code>\w+?)\/(?P<chapter_num>\d+?)\/(?P<verse_ref>.+?).md\)");
pat!(tn_obs_markdown_link, false, r"\[(?P<link_text>.+?)\] *\(rc:\/\/(?P<lang_code>.+?)\/tn\/help\/obs\/(?P<chapter_num>.+?)\/(?P<verse_ref>.+?)\)");
pat!(bc_markdown_link, false, r"\[(?P<link_text>.+?)\] *\(\.\.\/(?P<link_ref>articles.+?)\)");
pat!(see_parenthetical, true, r"\((?P<prefix_text>[^\s():]+:)\s*.*?\)");

/// All 25 patterns by name, with whether the main flow (`transform_*_links`) uses them.
pub fn pattern_table() -> Vec<(&'static str, &'static PyRegex, bool)> {
    vec![
        ("TW_RC_LINK_RE", tw_rc_link(), true),
        ("TW_OBE_RC_LINK_RE", tw_obe_rc_link(), true),
        ("RC_QUESTION_LINK_RE", rc_question_link(), true),
        ("TW_STAR_RC_LINK_RE", tw_star_rc_link(), true),
        ("TW_WIKI_RC_LINK_RE", tw_wiki_rc_link(), true),
        ("TW_WIKI_RC_LINK_RE2", tw_wiki_rc_link2(), true),
        ("TW_WIKI_PREFIXED_RC_LINK_RE", tw_wiki_prefixed_rc_link(), true),
        ("TW_WIKI_PREFIXED_RC_LINK_NO_CLOSE_PAREN_RE", tw_wiki_prefixed_rc_link_no_close_paren(), false),
        ("TW_MARKDOWN_LINK_RE", tw_markdown_link(), true),
        ("TA_WIKI_PREFIXED_RC_LINK_RE", ta_wiki_prefixed_rc_link(), true),
        ("TA_WIKI_RC_LINK_RE", ta_wiki_rc_link(), true),
        ("TA_STAR_RC_LINK_RE", ta_star_rc_link(), true),
        ("TA_PREFIXED_MARKDOWN_LINK_RE", ta_prefixed_markdown_link(), true),
        ("TA_PREFIXED_MARKDOWN_HTTPS_LINK_RE", ta_prefixed_markdown_https_link(), true),
        ("TA_MARKDOWN_HTTPS_LINK_RE", ta_markdown_https_link(), true),
        ("TN_MARKDOWN_SCRIPTURE_LINK_RE", tn_markdown_scripture_link(), true),
        ("MARKDOWN_LINK_RE", markdown_link(), false),
        ("WIKI_LINK_RE", wiki_link(), true),
        ("TN_MARKDOWN_RELATIVE_SCRIPTURE_LINK_RE", tn_markdown_relative_scripture_link(), true),
        ("TN_MARKDOWN_RELATIVE_TO_CURRENT_BOOK_SCRIPTURE_LINK_RE", tn_relative_to_current_book(), true),
        ("TN_MARKDOWN_RELATIVE_TO_CURRENT_BOOK_SCRIPTURE_LINK_RE_NO_PARENS", tn_relative_to_current_book_no_parens(), true),
        ("TN_MARKDOWN_RELATIVE_TO_CURRENT_CHAPTER_SCRIPTURE_LINK_RE_NO_PARENS", tn_relative_to_current_chapter_no_parens(), false),
        ("TN_MARKDOWN_RELATIVE_SCRIPTURE_LINK_RE_NO_PARENS", tn_relative_scripture_no_parens(), false),
        ("TN_OBS_MARKDOWN_LINK_RE", tn_obs_markdown_link(), true),
        ("BC_MARKDOWN_LINK_RE", bc_markdown_link(), false),
        ("SEE_PARENTHETICAL_RE", see_parenthetical(), true),
    ]
}

// ---------------------------------------------------------------------------------
// small Python helpers
// ---------------------------------------------------------------------------------

/// `text.replace(from, to)` (Python replaces every occurrence, including for `from == ""`).
pub fn replace_all(text: &str, from: &str, to: &str) -> String {
    text.replace(from, to)
}

/// Python's `str.strip()` whitespace set (adds U+001C..U+001F to Unicode White_Space).
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

/// `str.zfill(3)`.
pub fn zfill3(s: &str) -> String {
    let n = s.chars().count();
    if n >= 3 {
        return s.to_string();
    }
    let (sign, rest) = match s.chars().next() {
        Some(c @ ('+' | '-')) => (c.to_string(), &s[1..]),
        _ => (String::new(), s),
    };
    format!("{sign}{}{rest}", "0".repeat(3 - n))
}

fn tw_requested(env: &LinkEnv<'_>) -> bool {
    env.requests.iter().any(|r| r.resource_type.contains("tw"))
}

/// `localized_translation_word` (`tw_utils.py:52`).
pub fn localized_translation_word(content: &str) -> String {
    let first_line = content.split('\n').next().unwrap_or("");
    let comps: Vec<&str> = first_line.split("# ").collect();
    let word = if comps.len() >= 2 { comps[1] } else { "" };
    let word = if word.contains(',') { word.split(',').next().unwrap_or("") } else { word };
    py_strip(word).to_string()
}

fn tw_localized(env: &LinkEnv<'_>, stem: &str) -> Res<Option<String>> {
    // `filename_sans_suffix in translation_words_dict and tw_resources_requests`
    if !(env.tw_words.contains_key(stem) && tw_requested(env)) {
        return Ok(None);
    }
    let path = &env.tw_words[stem];
    match env.tree.read(path) {
        Ok(Some(text)) => Ok(Some(localized_translation_word(text))),
        Ok(None) => Err(PyError::new("FileNotFoundError", path.clone())),
        Err(e) => Err(PyError::new("UnicodeDecodeError", e)),
    }
}

fn anchor(word: &str, lang: &str, target: &str) -> String {
    format!("[{word}](#{lang}-{target})")
}

// ---------------------------------------------------------------------------------
// section removal and pagination
// ---------------------------------------------------------------------------------

pub const SECTIONS_TO_REMOVE: &[&str] = &["Examples from the Bible stories", "Links", "Tautan", "Picture of", "Pictures"];

/// `str.splitlines()` line boundaries.
pub fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < bytes.len() {
        let (pos, c) = bytes[i];
        let is_break = matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if is_break {
            out.push(&s[start..pos]);
            let mut next = pos + c.len_utf8();
            if c == '\r' && i + 1 < bytes.len() && bytes[i + 1].1 == '\n' {
                next += 1;
                i += 1;
            }
            start = next;
        }
        i += 1;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// `^#+ *<name>.*` via `re.match` on one line.
fn is_section_header(line: &str, name: &str) -> bool {
    let after_hashes = line.trim_start_matches('#');
    if after_hashes.len() == line.len() {
        return false;
    }
    after_hashes.trim_start_matches(' ').starts_with(name)
}

/// `remove_md_section`: drop a header and its text until the next header line.
pub fn remove_md_section(source: &str, name: &str) -> String {
    let mut out = String::new();
    let mut in_section = false;
    for line in py_splitlines(source) {
        if in_section {
            if line.starts_with('#') {
                out.push_str(line);
                out.push('\n');
                in_section = false;
            }
        } else if is_section_header(line, name) {
            in_section = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub fn remove_sections(source: &str) -> String {
    let mut s = source.to_string();
    for header in SECTIONS_TO_REMOVE {
        s = remove_md_section(&s, header);
    }
    s
}

pub fn remove_pagination_symbols(source: &str) -> String {
    let s = source.replace("<strong>| &gt;&gt;</strong>", "");
    let s = s.replace("<p>__&lt;&lt; | __</p>", "");
    s.replace("<strong>&lt;&lt; | &gt;&gt;</strong>", "")
}

// ---------------------------------------------------------------------------------
// TW transformers
// ---------------------------------------------------------------------------------

/// `transform_tw_links` (`markdown_transformer.py:94`).
pub fn transform_tw_links(source: &str, lang_code: &str, env: &LinkEnv<'_>) -> Res<String> {
    let mut source = source.to_string();
    for wl in wiki_link().finditer(&source.clone())? {
        let url = wl.name("url").to_string();
        source = transform_tw_rc_link(&url, &source, lang_code, env)?;
        source = transform_tw_star_rc_link(&url, &source)?;
    }
    source = transform_tw_wiki_prefixed_rc_links(&source, lang_code, env)?;
    source = transform_tw_wiki_rc_links(&source, lang_code, env, tw_wiki_rc_link())?;
    source = transform_tw_wiki_rc_links(&source, lang_code, env, tw_wiki_rc_link2())?;
    source = transform_tw_markdown_links(&source, lang_code, env, tw_markdown_link())?;
    source = transform_tw_markdown_links(&source, lang_code, env, tw_obe_rc_link())?;
    Ok(remove_see_colon_prefixed_parentheticals(&source))
}

fn transform_tw_rc_link(url: &str, source: &str, lang_code: &str, env: &LinkEnv<'_>) -> Res<String> {
    let mut source = source.to_string();
    if let Some(m) = tw_rc_link().search(url)? {
        let stem = m.name("word");
        let replacement = match tw_localized(env, stem)? {
            Some(word) => anchor(&word, lang_code, &word),
            None => stem.to_string(),
        };
        let new_url = url.replace(m.whole(), &replacement);
        // regexp = r"\[\[{}\]\]".format(wikilink.url): the URL is used as a pattern, unescaped
        let dynamic = PyRegex::new(&format!(r"\[\[{url}\]\]"), false)?;
        for m2 in dynamic.finditer(&source.clone())? {
            source = replace_all(&source, m2.whole(), &new_url);
        }
    }
    Ok(source)
}

/// `transform_tw_star_rc_link`: its pattern needs literal `[[` and is applied to the text
/// BETWEEN the brackets, so it can never match; kept for completeness (as built: dead).
fn transform_tw_star_rc_link(url: &str, source: &str) -> Res<String> {
    if tw_star_rc_link().search(url)?.is_some() {
        // DOC would raise IndexError in `fmt_str.format("#{lang_code}-{url}")`.
        return Err(PyError::new("IndexError", "Replacement index 1 out of range for positional args tuple"));
    }
    Ok(source.to_string())
}

fn transform_tw_wiki_prefixed_rc_links(source: &str, lang_code: &str, env: &LinkEnv<'_>) -> Res<String> {
    let mut source = source.to_string();
    for m in tw_wiki_prefixed_rc_link().finditer(&source.clone())? {
        match tw_localized(env, m.name("word"))? {
            Some(word) => {
                let new = format!("({}: {})", m.name("prefix_text"), anchor(&word, lang_code, &word));
                source = replace_all(&source, m.whole(), &new);
            }
            None => source = replace_all(&source, m.whole(), ""),
        }
    }
    Ok(source)
}

/// `transform_tw_wiki_rc_links` and `..._links2`: anchor on the localized word, or delete.
fn transform_tw_wiki_rc_links(source: &str, lang_code: &str, env: &LinkEnv<'_>, re: &PyRegex) -> Res<String> {
    let mut source = source.to_string();
    for m in re.finditer(&source.clone())? {
        match tw_localized(env, m.name("word"))? {
            Some(word) => source = replace_all(&source, m.whole(), &anchor(&word, lang_code, &word)),
            None => source = replace_all(&source, m.whole(), ""),
        }
    }
    Ok(source)
}

/// `transform_tw_markdown_links` and `transform_rc_obe_tw_links`: anchor on the FILE STEM;
/// no match leaves the link untouched.
fn transform_tw_markdown_links(source: &str, lang_code: &str, env: &LinkEnv<'_>, re: &PyRegex) -> Res<String> {
    let mut source = source.to_string();
    for m in re.finditer(&source.clone())? {
        let stem = m.name("word");
        if let Some(word) = tw_localized(env, stem)? {
            source = replace_all(&source, m.whole(), &anchor(&word, lang_code, stem));
        }
    }
    Ok(source)
}

// ---------------------------------------------------------------------------------
// TA and TN transformers
// ---------------------------------------------------------------------------------

/// `transform_ta_and_tn_links` (`markdown_transformer.py:127`).
pub fn transform_ta_and_tn_links(source: &str, lang_code: &str, env: &LinkEnv<'_>) -> Res<String> {
    let mut s = source.to_string();
    for re in [
        ta_wiki_prefixed_rc_link(),
        ta_wiki_rc_link(),
        ta_star_rc_link(),
        ta_prefixed_markdown_https_link(),
        ta_prefixed_markdown_link(),
        ta_markdown_https_link(),
    ] {
        s = delete_matches(&s, re)?;
    }
    s = transform_tn_prefixed_markdown_links(&s, env)?;
    s = transform_tn_relative_links(&s, lang_code, env, tn_markdown_relative_scripture_link(), true)?;
    s = transform_tn_relative_links(&s, lang_code, env, tn_relative_to_current_book(), false)?;
    s = transform_tn_missing_book_code_no_paren(&s)?;
    s = transform_tn_obs(&s)?;
    s = delete_matches(&s, rc_question_link())?;
    Ok(remove_see_colon_prefixed_parentheticals(&s))
}

/// `for match in finditer(re, source): source = source.replace(match.group(0), "")`.
fn delete_matches(source: &str, re: &PyRegex) -> Res<String> {
    let mut s = source.to_string();
    for m in re.finditer(source)? {
        s = replace_all(&s, m.whole(), "");
    }
    Ok(s)
}

/// Does the translation-note file for this link exist? As built (D3) the path assumes a
/// nested clone layout and so is never found in DOC's flat clones.
fn tn_note_exists(env: &LinkEnv<'_>, rr: &Req, book_code: &str, chapter: &str, verse: &str) -> bool {
    if env.compat.nested_clone_layout && !env.xref_links {
        let first = format!("{}_{}", rr.lang_code, rr.resource_type);
        let second = format!("{}_tn", rr.lang_code);
        let path = format!("{}.md", py_join(&[&first, &second, book_code, chapter, verse]));
        env.tree.exists(&path)
    } else {
        match env.dirs.iter().find(|(q, _)| q == rr) {
            Some((_, dir)) => env.tree.exists(&format!("{}.md", py_join(&[dir, book_code, chapter, verse]))),
            None => false,
        }
    }
}

pub fn book_number(code: &str) -> Res<&'static str> {
    tables::BOOK_NUMBERS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .ok_or_else(|| PyError::new("KeyError", format!("'{code}'")))
}

/// The id a TN verse heading must carry for `note_anchor` links to resolve.
pub fn note_anchor_id(lang: &str, book_code: &str, chapter: &str, verse: &str) -> Res<String> {
    Ok(format!("{}-{}-ch-{}-v-{}", lang, zfill3(book_number(book_code)?), zfill3(chapter), zfill3(verse)))
}

pub fn note_anchor(scripture_ref: &str, lang: &str, book_code: &str, chapter: &str, verse: &str) -> Res<String> {
    Ok(format!(
        "[{}](#{}-{}-ch-{}-v-{})",
        scripture_ref,
        lang,
        zfill3(book_number(book_code)?),
        zfill3(chapter),
        zfill3(verse)
    ))
}

fn transform_tn_prefixed_markdown_links(source: &str, env: &LinkEnv<'_>) -> Res<String> {
    let mut s = source.to_string();
    for m in tn_markdown_scripture_link().finditer(source)? {
        let (sref, lang, book, ch, verse) =
            (m.name("scripture_ref"), m.name("lang_code"), m.name("book_code"), m.name("chapter_num"), m.name("verse_ref"));
        let found = env
            .requests
            .iter()
            .find(|r| r.lang_code == lang && r.resource_type.contains("tn") && r.book_code == book);
        let replacement = match found {
            Some(rr) if tn_note_exists(env, rr, book, ch, verse) => {
                format!("({})", note_anchor(sref, &rr.lang_code, &rr.book_code, ch, verse)?)
            }
            Some(_) => {
                env.note_missing(lang, book, ch, verse, "note file missing");
                sref.to_string()
            }
            None => {
                env.note_missing(lang, book, ch, verse, "tn resource not requested");
                sref.to_string()
            }
        };
        s = replace_all(&s, m.whole(), &replacement);
    }
    Ok(s)
}

/// `transform_tn_markdown_links` (`with_book`: the link names the book) and
/// `transform_tn_missing_book_code_markdown_links` (the book is the first TN request's).
fn transform_tn_relative_links(source: &str, lang_code: &str, env: &LinkEnv<'_>, re: &PyRegex, with_book: bool) -> Res<String> {
    let mut s = source.to_string();
    for m in re.finditer(source)? {
        let (sref, ch, verse) = (m.name("scripture_ref"), m.name("chapter_num"), m.name("verse_ref"));
        let found = if with_book {
            let book = m.name("book_code");
            env.requests.iter().find(|r| r.lang_code == lang_code && r.resource_type.contains("tn") && r.book_code == book)
        } else {
            env.requests.iter().find(|r| r.lang_code == lang_code && r.resource_type.contains("tn"))
        };
        let wrapped = format!("({})", m.whole());
        match found {
            Some(rr) => {
                let book = if with_book { m.name("book_code") } else { rr.book_code.as_str() };
                if tn_note_exists(env, rr, book, ch, verse) {
                    let link = note_anchor(sref, lang_code, book, ch, verse)?;
                    s = replace_all(&s, m.whole(), &format!("({link})"));
                } else {
                    env.note_missing(lang_code, book, ch, verse, "note file missing");
                    s = replace_all(&s, &wrapped, sref);
                }
            }
            None => {
                let book = if with_book { m.name("book_code") } else { "" };
                env.note_missing(lang_code, book, ch, verse, "tn resource not requested");
                s = replace_all(&s, &wrapped, sref)
            }
        }
    }
    Ok(s)
}

fn transform_tn_missing_book_code_no_paren(source: &str) -> Res<String> {
    let mut s = source.to_string();
    for m in tn_relative_to_current_book_no_parens().finditer(source)? {
        s = replace_all(&s, m.whole(), m.name("scripture_ref"));
    }
    Ok(s)
}

fn transform_tn_obs(source: &str) -> Res<String> {
    let mut s = source.to_string();
    for m in tn_obs_markdown_link().finditer(source)? {
        s = replace_all(&s, m.whole(), m.name("link_text"));
    }
    Ok(s)
}

/// `remove_see_colon_prefixed_parenthicals`.
pub fn remove_see_colon_prefixed_parentheticals(source: &str) -> String {
    see_parenthetical().sub_plain(source, "").unwrap_or_else(|_| source.to_string())
}

/// Exposed for the unit tests: one match of a named family.
pub fn first_match(name: &str, text: &str) -> Option<Match> {
    pattern_table().into_iter().find(|(n, _, _)| *n == name).and_then(|(_, re, _)| re.search(text).ok().flatten())
}
