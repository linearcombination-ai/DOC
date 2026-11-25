//! C9 `md-readers`: DOC's `parsing.py` readers for Translation Notes (TN), Condensed TN
//! (TNC), Translation Questions (TQ), Translation Words (TW) and Bible Commentary (BC),
//! as pure functions over a [`Tree`]. The output is the typed-book seam of slice S1
//! (`model::NoteBook`, `TwBook`, `BcBook`), so S1's interleavers consume it unchanged.
//!
//! USFM is not read here (P3). The shell supplies, per found resource, a [`Dto`] (the
//! lookup result) and the resource's directory in the tree.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::links::{
    self, py_strip, transform_ta_and_tn_links, transform_tw_links, LinkEnv, PyError, ReaderCompat, Req, Res,
};
use crate::markdown;
use doc_model::model::{Finding, BcBook, BcChapter, BookCode, Chapters, Direction, NoteBook, NoteChapter, TwBook, TwEntry, Verse};
use doc_model::tables;
use doc_model::tree::{path_name, path_stem, Tree};

/// A found `ResourceLookupDto` plus the directory the shell cloned it into.
#[derive(Clone, Debug, Deserialize)]
pub struct Dto {
    pub lang_code: String,
    #[serde(default)]
    pub lang_name: String,
    pub resource_type: String,
    #[serde(default)]
    pub resource_type_name: String,
    pub book_code: String,
    #[serde(default)]
    pub lang_direction: Direction,
    /// Directory in the tree (relative to the assets root), e.g. `en_tn`.
    pub dir: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReadBooks {
    pub tn: Vec<NoteBook>,
    pub tnc: Vec<NoteBook>,
    pub tq: Vec<NoteBook>,
    pub tw: Vec<TwBook>,
    pub bc: Vec<BcBook>,
    /// Cross-reference findings (only when `xref_links` is on).
    pub findings: Vec<Finding>,
}

pub struct ReaderSettings {
    pub layout_for_print: bool,
    pub generate_docx: bool,
    /// `settings.SHOW_TN_BOOK_INTRO`
    pub show_tn_book_intro: bool,
    pub compat: ReaderCompat,
    /// Option: TN cross-references become working links (`<a class="doc-xref" href="#id">`)
    /// and the matching notes carry the `id`. Off: plain text, exactly as built.
    pub xref_links: bool,
}

fn english_name(code: &str) -> Res<&'static str> {
    tables::BOOKS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, n, _)| *n)
        .ok_or_else(|| PyError::new("KeyError", format!("'{code}'")))
}

/// Python `int(str)` for the strings DOC passes (digits, optional sign and whitespace).
pub fn py_int(s: &str) -> Res<i64> {
    let t = py_strip(s);
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut value: i64 = 0;
    let mut prev_us = true; // leading underscore is invalid
    let mut any = false;
    for c in digits.chars() {
        if c == '_' {
            if prev_us {
                return Err(PyError::new("ValueError", format!("invalid literal for int() with base 10: '{s}'")));
            }
            prev_us = true;
            continue;
        }
        let d = c.to_digit(10).or_else(|| unicode_decimal(c));
        match d {
            Some(d) => {
                value = value.saturating_mul(10).saturating_add(i64::from(d));
                prev_us = false;
                any = true;
            }
            None => return Err(PyError::new("ValueError", format!("invalid literal for int() with base 10: '{s}'"))),
        }
    }
    if !any || prev_us {
        return Err(PyError::new("ValueError", format!("invalid literal for int() with base 10: '{s}'")));
    }
    Ok(if neg { -value } else { value })
}

/// Decimal digit value of a non-ASCII Unicode `Nd` character (Python's `int` accepts them).
fn unicode_decimal(c: char) -> Option<u32> {
    use unicode_general_category::{get_general_category, GeneralCategory};
    if get_general_category(c) != GeneralCategory::DecimalNumber {
        return None;
    }
    // Nd blocks are contiguous runs of ten starting at a code point whose offset is 0.
    let mut zero = c as u32;
    while zero > 0 {
        let prev = char::from_u32(zero - 1)?;
        if get_general_category(prev) == GeneralCategory::DecimalNumber && (c as u32 - (zero - 1)) < 10 {
            zero -= 1;
        } else {
            break;
        }
    }
    Some((c as u32 - zero) % 10)
}

/// `clean_numeric_string`: keep `str.isdigit()` characters.
fn clean_numeric_string(s: &str) -> String {
    use unicode_general_category::{get_general_category, GeneralCategory};
    s.chars()
        .filter(|c| c.is_ascii_digit() || get_general_category(*c) == GeneralCategory::DecimalNumber)
        .collect()
}

fn sub_heading(html: &str, from: &str, to: &str, compat: ReaderCompat) -> String {
    if compat.heading_substring_replace {
        // re.sub("h1", "h5", html): the literal two characters, anywhere
        html.replace(from, to)
    } else {
        html.replace(&format!("<{from}"), &format!("<{to}")).replace(&format!("</{from}"), &format!("</{to}"))
    }
}

struct Ctx<'a> {
    tree: &'a Tree,
    settings: &'a ReaderSettings,
    requests: &'a [Req],
    dirs: &'a [(Req, String)],
    findings: std::rc::Rc<std::cell::RefCell<Vec<Finding>>>,
}

impl<'a> Ctx<'a> {
    fn env<'b>(&'b self, tw_words: &'b BTreeMap<String, String>) -> LinkEnv<'b>
    where
        'a: 'b,
    {
        let mut env = LinkEnv::new(self.requests, tw_words, self.tree, self.settings.compat, self.dirs);
        env.xref_links = self.settings.xref_links;
        env.findings = self.findings.clone();
        env
    }

    fn read(&self, path: &str) -> Res<String> {
        match self.tree.read(path) {
            Ok(Some(t)) => Ok(t.to_string()),
            Ok(None) => Err(PyError::new("FileNotFoundError", path)),
            Err(e) => Err(PyError::new("UnicodeDecodeError", e)),
        }
    }

    /// `tw_resource_dir(lang_code)`: as built, a glob that assumes a nested clone layout.
    fn tw_resource_dir(&self, lang: &str) -> Option<String> {
        if self.settings.compat.nested_clone_layout {
            self.tree.glob(&format!("{lang}_tw*/{lang}_tw*")).into_iter().next()
        } else {
            self.dirs.iter().find(|(r, _)| r.lang_code == lang && r.resource_type == "tw").map(|(_, d)| d.clone())
        }
    }

    /// `translation_words_dict`.
    fn translation_words_dict(&self, dir: Option<&str>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if let Some(dir) = dir {
            for path in self.translation_word_filepaths(dir) {
                out.insert(path_stem(&path).to_string(), path);
            }
        }
        out
    }

    fn translation_word_filepaths(&self, dir: &str) -> Vec<String> {
        let mut v = self.tree.glob(&format!("{dir}/bible/kt/*.md"));
        v.extend(self.tree.glob(&format!("{dir}/bible/names/*.md")));
        v.extend(self.tree.glob(&format!("{dir}/bible/other/*.md")));
        v
    }

    fn glob_chapter_dirs(&self, dir: &str, book: &str) -> Vec<String> {
        let upper = book.to_uppercase();
        let mut found = self.tree.glob(&format!("{dir}/**/*{book}/*[0-9]*"));
        if found.is_empty() {
            found = self.tree.glob(&format!("{dir}/**/*{upper}/*[0-9]*"));
        }
        if found.is_empty() {
            found = self.tree.glob(&format!("{dir}/*{book}/*[0-9]*"));
        }
        if found.is_empty() {
            found = self.tree.glob(&format!("{dir}/*{upper}/*[0-9]*"));
        }
        found.sort();
        found
    }

    fn first_text(&self, md_glob: &str, txt_glob: &str) -> Res<Option<String>> {
        let mut paths = self.tree.glob(md_glob);
        if paths.is_empty() {
            paths = self.tree.glob(txt_glob);
        }
        match paths.first() {
            Some(p) => Ok(Some(self.read(p)?)),
            None => Ok(None),
        }
    }

    /// markdown to HTML; with `xref_links` the TN cross-reference anchors get the shared class,
    /// so one CSS selector (or dropping this one call) switches every link's look or existence.
    fn html(&self, md: &str) -> String {
        let html = markdown::to_html(md);
        if self.settings.xref_links {
            mark_xrefs(&html)
        } else {
            html
        }
    }

    /// The intro pipeline shared by chapter and book intros (TN, TNC): sections, TW links,
    /// TA/TN links, markdown.
    fn intro_markdown(&self, raw: &str, lang: &str, tw_words: &BTreeMap<String, String>) -> Res<String> {
        let env = self.env(tw_words);
        let s = links::remove_sections(raw);
        let s = transform_tw_links(&s, lang, &env)?;
        transform_ta_and_tn_links(&s, lang, &env)
    }
}

/// Insert keeping Python dict semantics: an existing key keeps its position.
/// `<a href="#en-065-ch-001-v-005">` becomes `<a class="doc-xref" href="#en-065-ch-001-v-005">`.
fn mark_xrefs(html: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<crate::pyre::PyRegex> = OnceLock::new();
    let re = RE.get_or_init(|| crate::pyre::PyRegex::new(r#"<a href="(#[^"]*-ch-\d{3}-v-\d{3})">"#, false).expect("xref anchor pattern"));
    let mut out = String::with_capacity(html.len() + 32);
    let mut last = 0;
    for m in re.finditer(html).unwrap_or_default() {
        out.push_str(&html[last..m.start]);
        out.push_str(&format!("<a class=\"{}\" href=\"{}\">", doc_model::model::XREF_CLASS, m.group(1).unwrap_or("")));
        last = m.end;
    }
    out.push_str(&html[last..]);
    out
}

fn dict_set<T>(chapters: &mut Vec<(u32, T)>, key: u32, value: T) {
    if let Some(slot) = chapters.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = value;
    } else {
        chapters.push((key, value));
    }
}

fn verse_set(verses: &mut Vec<Verse>, label: String, html: String) {
    if let Some(slot) = verses.iter_mut().find(|v| v.label.raw == label) {
        slot.html = html;
    } else {
        verses.push(Verse::from((label, html)));
    }
}

fn to_u32(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

// ------------------------------------------------------------------------------ TN / TNC

fn tn_verses_html(cx: &Ctx<'_>, chapter_dir: &str, lang: &str, book: &str, target: bool, tw: &BTreeMap<String, String>) -> Res<Vec<Verse>> {
    let mut paths = cx.tree.glob(&format!("{chapter_dir}/*[0-9]*.md"));
    if paths.is_empty() {
        paths = cx.tree.glob(&format!("{chapter_dir}/*[0-9]*.txt"));
    }
    let env = cx.env(tw);
    let mut verses: Vec<Verse> = Vec::new();
    for path in paths {
        let verse_ref = path_stem(&path).to_string();
        let md = cx.read(&path)?;
        let md = transform_ta_and_tn_links(&md, lang, &env)?;
        let html = cx.html(&md);
        let html = sub_heading(&html, "h1", "h5", cx.settings.compat);
        let chapter_num = py_int(path_stem(chapter_dir))?;
        let verse_num = py_int(&verse_ref)?;
        let id = if target && cx.settings.xref_links {
            format!(" id=\"{}\"", links::note_anchor_id(lang, book, path_stem(chapter_dir), &verse_ref)?)
        } else {
            String::new()
        };
        let formatted = format!("<h4{}>{} {}:{}</h4>\n{}", id, english_name(book)?, chapter_num, verse_num, html);
        verse_set(&mut verses, verse_ref, formatted);
    }
    Ok(verses)
}

fn tn_chapters(cx: &Ctx<'_>, dir: &str, lang: &str, book: &str, target: bool) -> Res<Chapters<NoteChapter>> {
    let mut chapters: Vec<(u32, NoteChapter)> = Vec::new();
    let tw_dir = cx.tw_resource_dir(lang);
    let tw = cx.translation_words_dict(tw_dir.as_deref());
    for chapter_dir in cx.glob_chapter_dirs(dir, book) {
        let chapter_num = to_u32(py_int(path_name(&chapter_dir))?);
        let intro = cx.first_text(&format!("{chapter_dir}/*intro.md"), &format!("{chapter_dir}/*intro.txt"))?;
        let mut intro_html = String::new();
        if let Some(raw) = intro.filter(|s| !s.is_empty()) {
            let md = cx.intro_markdown(&raw, lang, &tw)?;
            intro_html = links::remove_pagination_symbols(&cx.html(&md));
        }
        let verses = tn_verses_html(cx, &chapter_dir, lang, book, target, &tw)?;
        dict_set(&mut chapters, chapter_num, NoteChapter { intro_html, verses });
    }
    Ok(Chapters(chapters))
}

fn book_intro(cx: &Ctx<'_>, dir: &str, lang: &str, book: &str) -> Res<String> {
    if !cx.settings.show_tn_book_intro {
        return Ok(String::new());
    }
    let raw = cx.first_text(&format!("{dir}/*{book}/front/intro.md"), &format!("{dir}/*{book}/front/intro.txt"))?;
    match raw.filter(|s| !s.is_empty()) {
        Some(raw) => {
            let tw_dir = cx.tw_resource_dir(lang);
            let tw = cx.translation_words_dict(tw_dir.as_deref());
            let md = cx.intro_markdown(&raw, lang, &tw)?;
            Ok(cx.html(&md))
        }
        None => Ok(String::new()),
    }
}

fn note_book(dto: &Dto, chapters: Chapters<NoteChapter>, book_intro: String) -> Res<NoteBook> {
    Ok(NoteBook {
        lang_code: dto.lang_code.clone(),
        lang_name: dto.lang_name.clone(),
        book_code: BookCode::parse(&dto.book_code).ok_or_else(|| PyError::new("KeyError", format!("'{}'", dto.book_code)))?,
        resource_type_name: dto.resource_type_name.clone(),
        lang_direction: dto.lang_direction,
        book_intro,
        chapters,
    })
}

// ------------------------------------------------------------------------------ TQ

fn tq_book(cx: &Ctx<'_>, dto: &Dto) -> Res<NoteBook> {
    let (lang, book) = (&dto.lang_code, &dto.book_code);
    let mut chapters: Vec<(u32, NoteChapter)> = Vec::new();
    let tw_dir = cx.tw_resource_dir(lang);
    let tw = cx.translation_words_dict(tw_dir.as_deref());
    let env = cx.env(&tw);
    for chapter_dir in cx.glob_chapter_dirs(&dto.dir, book) {
        let chapter_num = to_u32(py_int(path_name(&chapter_dir))?);
        let mut verses: Vec<Verse> = Vec::new();
        for path in cx.tree.glob(&format!("{chapter_dir}/*[0-9]*.md")) {
            let verse_ref = clean_numeric_string(path_stem(&path));
            let md = cx.read(&path)?;
            let md = transform_ta_and_tn_links(&md, lang, &env)?;
            let html = sub_heading(&cx.html(&md), "h1", "h5", cx.settings.compat);
            let formatted = format!("<h4>{} {}:{}</h4>\n{}", english_name(book)?, chapter_num, py_int(&verse_ref)?, html);
            verse_set(&mut verses, verse_ref, formatted);
            // as built: the chapter entry is (re)assigned inside the verse loop, so a
            // chapter directory without verse files produces no chapter at all
            dict_set(&mut chapters, chapter_num, NoteChapter { intro_html: String::new(), verses: verses.clone() });
        }
    }
    note_book(dto, Chapters(chapters), String::new())
}

// ------------------------------------------------------------------------------ TW

fn tw_book(cx: &Ctx<'_>, dto: &Dto) -> Res<TwBook> {
    let lang = &dto.lang_code;
    let tw = cx.translation_words_dict(Some(&dto.dir));
    let env = cx.env(&tw);
    let mut pairs: Vec<TwEntry> = Vec::new();
    for path in cx.translation_word_filepaths(&dto.dir) {
        let mut content = cx.read(&path)?;
        if let Some(rest) = content.strip_prefix("# \"") {
            content = format!("# {rest}");
        }
        let word = links::localized_translation_word(&content);
        if word.is_empty() {
            continue;
        }
        let md = links::remove_sections(&content);
        let md = transform_ta_and_tn_links(&md, lang, &env)?;
        let md = transform_tw_links(&md, lang, &env)?;
        let html = cx.html(&md);
        let html = sub_heading(&html, "h2", "h4", cx.settings.compat);
        let html = sub_heading(&html, "h1", "h3", cx.settings.compat);
        pairs.push(TwEntry { localized_word: word, path, content: html });
    }
    pairs.sort_by(|a, b| a.localized_word.cmp(&b.localized_word));
    Ok(TwBook {
        lang_code: dto.lang_code.clone(),
        lang_name: dto.lang_name.clone(),
        resource_type_name: dto.resource_type_name.clone(),
        lang_direction: dto.lang_direction,
        name_content_pairs: pairs,
    })
}

// ------------------------------------------------------------------------------ BC

/// `re.sub(r"<h1>(.*?)<\/h1>", r"<h1>\1 Commentary</h1>", html)`
fn modify_commentary_label(html: &str, chapter_num: u32) -> String {
    if chapter_num != 1 {
        return html.to_string();
    }
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<h1>") {
        let after = &rest[start + 4..];
        // `.` does not match a newline
        let line_end = after.find('\n').unwrap_or(after.len());
        match after[..line_end].find("</h1>") {
            Some(end) => {
                out.push_str(&rest[..start]);
                out.push_str("<h1>");
                out.push_str(&after[..end]);
                out.push_str(" Commentary</h1>");
                rest = &after[end + 5..];
            }
            None => {
                out.push_str(&rest[..start + 4]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub const BC_ARTICLE_URL: &str = "https://content.bibletranslationtools.org/WycliffeAssociates/en_bc/src/branch/master/";

/// `re.sub(r'<a\s+href="\/(.*?)">', ..., html)`
fn replace_relative_with_absolute_links(html: &str) -> String {
    let re = crate::pyre::PyRegex::new(r#"<a\s+href="\/(.*?)">"#, false);
    let re = match re {
        Ok(r) => r,
        Err(_) => return html.to_string(),
    };
    let mut out = String::new();
    let mut last = 0;
    for m in re.finditer(html).unwrap_or_default() {
        out.push_str(&html[last..m.start]);
        out.push_str(&format!("<a href=\"{}{}\" target=\"_blank\">", BC_ARTICLE_URL, m.group(1).unwrap_or("")));
        last = m.end;
    }
    out.push_str(&html[last..]);
    out
}

fn remove_links(html: &str) -> String {
    html.replace("<a ", "<span ").replace("</a>", "</span>")
}

fn bc_book(cx: &Ctx<'_>, dto: &Dto) -> Res<BcBook> {
    let (lang, book, dir) = (&dto.lang_code, &dto.book_code, &dto.dir);
    let no_tw = BTreeMap::new();
    let env = cx.env(&no_tw);
    // intro
    let intro_raw = match cx.tree.glob(&format!("{dir}/*{book}/intro.md")).first() {
        Some(p) => cx.read(p)?,
        None => String::new(),
    };
    let md = transform_ta_and_tn_links(&links::remove_sections(&intro_raw), lang, &env)?;
    let html = cx.html(&md);
    let html = doc_model::text::demote_headings_by_one(&html);
    let html = links::remove_pagination_symbols(&html);
    let book_intro = remove_links(&html);
    // chapters
    let mut chapters: Vec<(u32, BcChapter)> = Vec::new();
    for path in cx.tree.glob(&format!("{dir}/*{book}/*[0-9]*.md")) {
        let chapter_num = to_u32(py_int(path_stem(&path))?);
        let raw = cx.read(&path)?;
        let md = transform_ta_and_tn_links(&links::remove_sections(&raw), lang, &env)?;
        let html = cx.html(&md);
        let html = modify_commentary_label(&html, chapter_num);
        let html = links::remove_pagination_symbols(&html);
        let html = replace_relative_with_absolute_links(&html);
        let html = doc_model::text::demote_headings_by_one(&html);
        dict_set(&mut chapters, chapter_num, BcChapter { commentary: remove_links(&html) });
    }
    Ok(BcBook {
        lang_code: dto.lang_code.clone(),
        lang_name: dto.lang_name.clone(),
        book_code: BookCode::parse(book).ok_or_else(|| PyError::new("KeyError", format!("'{book}'")))?,
        resource_type_name: dto.resource_type_name.clone(),
        lang_direction: Direction::Ltr,
        book_intro,
        chapters: Chapters(chapters),
    })
}

// ------------------------------------------------------------------------------ dispatcher

/// `parsing.books` for the non-USFM resource types, in request order.
pub fn read_books(dtos: &[Dto], requests: &[Req], tree: &Tree, settings: &ReaderSettings) -> Res<ReadBooks> {
    let dirs: Vec<(Req, String)> = dtos
        .iter()
        .map(|d| (Req { lang_code: d.lang_code.clone(), resource_type: d.resource_type.clone(), book_code: d.book_code.clone() }, d.dir.clone()))
        .collect();
    let cx = Ctx { tree, settings, requests, dirs: &dirs, findings: Default::default() };
    let mut out = ReadBooks::default();
    for dto in dtos {
        match dto.resource_type.as_str() {
            "tn" | "tn-condensed" => {
                // the link logic targets the FIRST matching tn request: only its notes carry ids
                let target = requests
                    .iter()
                    .find(|r| r.lang_code == dto.lang_code && r.resource_type.contains("tn") && r.book_code == dto.book_code)
                    .is_some_and(|r| r.resource_type == dto.resource_type);
                let chapters = tn_chapters(&cx, &dto.dir, &dto.lang_code, &dto.book_code, target)?;
                let intro = book_intro(&cx, &dto.dir, &dto.lang_code, &dto.book_code)?;
                let nb = note_book(dto, chapters, intro)?;
                if dto.resource_type == "tn" {
                    out.tn.push(nb)
                } else {
                    out.tnc.push(nb)
                }
            }
            "tq" => out.tq.push(tq_book(&cx, dto)?),
            "tw" => out.tw.push(tw_book(&cx, dto)?),
            "bc" => out.bc.push(bc_book(&cx, dto)?),
            _ => {}
        }
    }
    out.findings = cx.findings.borrow().clone();
    Ok(out)
}
