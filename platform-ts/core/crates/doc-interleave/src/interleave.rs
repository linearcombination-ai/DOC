//! C12 interleave: typed books + validated request -> document parts + findings.
//!
//! A line-by-line port of DOC's `assemble_by_book.py`, `assemble_by_chapter.py` and
//! `assembly_strategy_utils.py` (commit 50de21cd). Every place where the as-built code
//! is defective is guarded by a [`BugCompat`] flag: `BugCompat::as_built()` reproduces
//! DOC's output exactly (for equivalence testing); `BugCompat::fixed()` is the
//! deliberately corrected behaviour.

use serde::{Deserialize, Serialize};

use doc_model::model::*;
use doc_request::request::{Layout, Strategy, TwMode, ValidRequest};
use doc_model::tables;
use doc_model::text::{demote_headings_by_one, h1, h2_book, verse_span, HR};
use crate::tw;

/// One flag per as-built defect. `true` = reproduce DOC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BugCompat {
    /// D1: notes verse labels are compared as strings with USFM labels ("01" != "1").
    pub verse_key_padding: bool,
    /// D2: `blo` prints Bible Commentary chapter text twice.
    pub duplicate_bc_commentary: bool,
    /// D7a: zero or 3+ USFM books for a (language, book) means no scripture at all.
    pub three_usfm_drops_scripture: bool,
    /// D7b: a secondary USFM book without the chapter raises `KeyError` (blo, lvo, bvo).
    pub secondary_missing_chapter_error: bool,
    /// D7c: a notes book without the chapter raises `KeyError` in the verse strategies.
    pub notes_missing_chapter_error: bool,
    /// D8a: `blo`/`bvo` take the text direction of the first USFM book for every part.
    pub first_book_direction: bool,
    /// D8b: `blo`/`bvo` iterate chapters `1..BOOK_CHAPTERS[book]` only.
    pub static_chapter_table: bool,
    /// D17: `blo` wraps TQ in two columns using the *TN* flag.
    pub blo_tq_uses_tn_two_column_flag: bool,
    /// D18: `blo` book intros for Bible Commentary are not heading-demoted and lose `is_rtl`.
    pub blo_bc_intro_quirk: bool,
    /// D20: include mode while printing skips the definitions section but the per-chapter
    /// lists still link to anchors that no longer exist. Off: print forces link-out.
    pub include_print_dead_links: bool,
    /// D31 (P3): the Translation Words matcher uses Python's `\b` (splits inside Khmer, Sinhala
    /// and Indic words, and at apostrophes). Off: UAX #29 word segmentation.
    pub tw_python_word_boundary: bool,
}

impl BugCompat {
    pub fn as_built() -> BugCompat {
        BugCompat {
            verse_key_padding: true,
            duplicate_bc_commentary: true,
            three_usfm_drops_scripture: true,
            secondary_missing_chapter_error: true,
            notes_missing_chapter_error: true,
            first_book_direction: true,
            static_chapter_table: true,
            blo_tq_uses_tn_two_column_flag: true,
            blo_bc_intro_quirk: true,
            include_print_dead_links: true,
            tw_python_word_boundary: true,
        }
    }
    pub fn fixed() -> BugCompat {
        BugCompat {
            verse_key_padding: false,
            duplicate_bc_commentary: false,
            three_usfm_drops_scripture: false,
            secondary_missing_chapter_error: false,
            notes_missing_chapter_error: false,
            first_book_direction: false,
            static_chapter_table: false,
            blo_tq_uses_tn_two_column_flag: false,
            blo_bc_intro_quirk: false,
            include_print_dead_links: false,
            tw_python_word_boundary: false,
        }
    }
}

impl Default for BugCompat {
    fn default() -> Self {
        BugCompat::fixed()
    }
}

/// The only failures the core has: where DOC raises, `as_built` raises.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreError {
    /// DOC: `KeyError: <chapter>`.
    KeyError { chapter: u32 },
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::KeyError { chapter } => write!(f, "KeyError: {chapter}"),
        }
    }
}

type Res<T> = Result<T, CoreError>;

struct Ctx<'a> {
    req: &'a ValidRequest,
    books: &'a Books,
    compat: BugCompat,
    findings: Vec<Finding>,
    rule: &'a dyn doc_model::text::WordRule,
}

// ------------------------------------------------------------------ lookups

fn type_name(code: &str) -> &'static str {
    tables::RESOURCE_TYPE_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("")
}

/// `order_usfm_resources`, operator precedence preserved: `(A and B) or C or (D and E) or (F and G)`.
fn order_usfm<'a>(a: &'a UsfmBook, b: &'a UsfmBook) -> (&'a UsfmBook, &'a UsfmBook) {
    let n = a.resource_type_name.as_str();
    let swap = (a.lang_code == "fr" && n == type_name("ulb"))
        || n == type_name("udb")
        || (a.lang_code == "id" && n == type_name("ayt"))
        || (a.lang_code == "pt-br" && n == type_name("blv"));
    if swap {
        (b, a)
    } else {
        (a, b)
    }
}

fn unique<T: PartialEq + Clone>(items: impl Iterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for i in items {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn collect_lang_codes(
    usfm: &[&UsfmBook],
    tn: &[&NoteBook],
    tnc: &[&NoteBook],
    tq: &[&NoteBook],
    tw: &[&TwBook],
    bc: &[&BcBook],
) -> Vec<String> {
    let all = usfm
        .iter()
        .map(|b| b.lang_code.clone())
        .chain(tn.iter().map(|b| b.lang_code.clone()))
        .chain(tnc.iter().map(|b| b.lang_code.clone()))
        .chain(tq.iter().map(|b| b.lang_code.clone()))
        .chain(tw.iter().map(|b| b.lang_code.clone()))
        .chain(bc.iter().map(|b| b.lang_code.clone()));
    unique(all)
}

impl<'a> Ctx<'a> {
    fn all_usfm(&self) -> Vec<&'a UsfmBook> {
        self.books.usfm.iter().collect()
    }
    fn all_tn(&self) -> Vec<&'a NoteBook> {
        self.books.tn.iter().collect()
    }
    fn all_tnc(&self) -> Vec<&'a NoteBook> {
        self.books.tnc.iter().collect()
    }
    fn all_tq(&self) -> Vec<&'a NoteBook> {
        self.books.tq.iter().collect()
    }
    fn all_tw(&self) -> Vec<&'a TwBook> {
        self.books.tw.iter().collect()
    }
    fn all_bc(&self) -> Vec<&'a BcBook> {
        self.books.bc.iter().collect()
    }

    /// `collect_unique_book_codes`: union over USFM, TN, TNC, TQ, BC (not TW), canonical order.
    fn book_codes(&self) -> Vec<BookCode> {
        let mut codes: Vec<BookCode> = Vec::new();
        let push = |codes: &mut Vec<BookCode>, c: BookCode| {
            if !codes.contains(&c) {
                codes.push(c);
            }
        };
        for b in &self.books.usfm {
            push(&mut codes, b.book_code);
        }
        for b in self.books.tn.iter().chain(&self.books.tnc).chain(&self.books.tq) {
            push(&mut codes, b.book_code);
        }
        for b in &self.books.bc {
            push(&mut codes, b.book_code);
        }
        codes.sort();
        codes
    }

    fn lang_codes_all(&self) -> Vec<String> {
        collect_lang_codes(&self.all_usfm(), &self.all_tn(), &self.all_tnc(), &self.all_tq(), &self.all_tw(), &self.all_bc())
    }

    /// The (primary, secondary) USFM pair for a set of candidates, plus an arity finding.
    fn pick_usfm(&mut self, candidates: &[&'a UsfmBook]) -> (Option<&'a UsfmBook>, Option<&'a UsfmBook>) {
        match candidates.len() {
            0 => (None, None),
            1 => (Some(candidates[0]), None),
            2 => {
                let (p, s) = order_usfm(candidates[0], candidates[1]);
                (Some(p), Some(s))
            }
            n => {
                let first = candidates[0];
                self.findings.push(
                    Finding::new(FindingCode::UsfmArityUnsupported, Severity::Warning)
                        .lang(&first.lang_code)
                        .book(first.book_code)
                        .detail(format!("{n} USFM resources for one language and book")),
                );
                if self.compat.three_usfm_drops_scripture {
                    (None, None)
                } else {
                    let (p, s) = order_usfm(candidates[0], candidates[1]);
                    (Some(p), Some(s))
                }
            }
        }
    }

    /// The mode actually applied: the user's choice stays in the request, but while
    /// printing an `include` choice is forced to link-out unless D20 is reproduced.
    fn tw_mode(&self) -> TwMode {
        effective_tw_mode(self.req, self.compat)
    }
    fn sep(&self) -> bool {
        self.req.options.use_section_visual_separator
    }

    // -------------------------------------------------- utils (assembly_strategy_utils)

    fn hr_trick(&self) -> Part {
        Part::new("&nbsp;", false).sep(self.sep())
    }

    fn chapter_commentary(bc: Option<&BcBook>, n: u32) -> String {
        match bc.and_then(|b| b.chapters.get(n)) {
            Some(c) if !c.commentary.is_empty() => c.commentary.clone(),
            _ => String::new(),
        }
    }

    fn tn_chapter_intro(tn: Option<&NoteBook>, n: u32) -> String {
        match tn.and_then(|b| b.chapters.get(n)) {
            Some(c) if !c.intro_html.is_empty() => demote_headings_by_one(&c.intro_html),
            _ => String::new(),
        }
    }

    fn tnc_chapter_intro(&self, tnc: Option<&NoteBook>, n: u32) -> String {
        match tnc.and_then(|b| b.chapters.get(n)) {
            Some(c) if !c.intro_html.is_empty() => {
                let mut s = demote_headings_by_one(&c.intro_html);
                if self.sep() {
                    s.push_str(HR);
                }
                s
            }
            _ => String::new(),
        }
    }

    /// `tn_chapter_verses` / `tq_chapter_verses` / `tnc_chapter_verses`: all verse
    /// fragments of the chapter in one div; empty string when the chapter is absent.
    fn notes_div(book: Option<&NoteBook>, n: u32, two_col: bool, tnc_style: bool) -> String {
        let Some(chapter) = book.and_then(|b| b.chapters.get(n)) else {
            return String::new();
        };
        let joined: String = chapter.verses.iter().map(|v| v.html.as_str()).collect();
        let style = if two_col {
            if tnc_style {
                "<div style='column-count: 2; padding-top: 2px; padding-bottom: 4px;'>"
            } else {
                "<div style='column-count: 2;'>"
            }
        } else {
            "<div>"
        };
        format!("{style}{joined}</div>")
    }

    fn book_intros(
        &self,
        tn: Option<&NoteBook>,
        tnc: Option<&NoteBook>,
        bc: Option<&BcBook>,
        rtl: bool,
        show_tn: bool,
        show_bc: bool,
    ) -> Vec<Part> {
        let mut parts = Vec::new();
        let sep = self.sep();
        if show_tn {
            for book in [tn, tnc].into_iter().flatten() {
                if !book.book_intro.is_empty() {
                    parts.push(Part::new(h1(&book.resource_type_name), rtl));
                    parts.push(Part::new(demote_headings_by_one(&book.book_intro), rtl).sep(sep));
                }
            }
        }
        if show_bc {
            if let Some(bc) = bc {
                if !bc.book_intro.is_empty() {
                    parts.push(Part::new(h1(&bc.resource_type_name), rtl));
                    parts.push(Part::new(demote_headings_by_one(&bc.book_intro), rtl).sep(sep));
                }
            }
        }
        parts
    }

    /// `get_book_intros_for_groups` (blo): every language's TN, then TNC, then BC intro.
    fn book_intros_for_groups(
        &self,
        tn: &[&NoteBook],
        tnc: &[&NoteBook],
        bc: &[&BcBook],
        rtl: bool,
        show_tn: bool,
        show_bc: bool,
        h: fn(&str) -> String,
    ) -> Vec<Part> {
        let sep = self.sep();
        let mut parts = Vec::new();
        if show_tn {
            for b in tn.iter().chain(tnc.iter()) {
                if !b.book_intro.is_empty() {
                    parts.push(Part::new(h(&b.resource_type_name), rtl));
                    parts.push(Part::new(demote_headings_by_one(&b.book_intro), rtl).sep(sep));
                }
            }
        }
        if show_bc {
            for b in bc {
                if !b.book_intro.is_empty() {
                    parts.push(Part::new(h(&b.resource_type_name), rtl));
                    if self.compat.blo_bc_intro_quirk {
                        // as built: not demoted, and is_rtl is dropped
                        parts.push(Part::new(b.book_intro.clone(), false).sep(sep));
                    } else {
                        parts.push(Part::new(demote_headings_by_one(&b.book_intro), rtl).sep(sep));
                    }
                }
            }
        }
        parts
    }

    fn chapter_intros(&self, tn: Option<&NoteBook>, tnc: Option<&NoteBook>, n: u32, rtl: bool, show: bool) -> Vec<Part> {
        let mut parts = Vec::new();
        if !show {
            return parts;
        }
        let sep = self.sep();
        let intro = Self::tn_chapter_intro(tn, n);
        if let (false, Some(tn)) = (intro.is_empty(), tn) {
            parts.push(Part::new(h1(&tn.resource_type_name), rtl));
            parts.push(Part::new(intro, rtl).sep(sep));
        }
        let intro = self.tnc_chapter_intro(tnc, n);
        if let (false, Some(tnc)) = (intro.is_empty(), tnc) {
            parts.push(Part::new(h1(&tnc.resource_type_name), rtl));
            parts.push(Part::new(intro, rtl).sep(sep));
        }
        parts
    }

    /// `get_chapter_intros_for_groups` (blo): intros of one language, with BC chapter
    /// commentary always included (no `show_bc_chapter_commentary` in lbo/blo).
    fn chapter_intros_for_groups(
        &self,
        tn: &[&NoteBook],
        tnc: &[&NoteBook],
        bc: &[&BcBook],
        n: u32,
        lang: &str,
        rtl: bool,
    ) -> Vec<Part> {
        let sep = self.sep();
        let mut parts = Vec::new();
        if self.req.options.show_tn_chapter_intro {
            for b in tn.iter().filter(|b| b.lang_code == lang) {
                let intro = Self::tn_chapter_intro(Some(b), n);
                if !intro.is_empty() {
                    parts.push(Part::new(h1(&b.resource_type_name), rtl));
                    parts.push(Part::new(intro, rtl).sep(sep));
                }
            }
            for b in tnc.iter().filter(|b| b.lang_code == lang) {
                let intro = self.tnc_chapter_intro(Some(b), n);
                if !intro.is_empty() {
                    parts.push(Part::new(h1(&b.resource_type_name), rtl));
                    parts.push(Part::new(intro, rtl).sep(sep));
                }
            }
        }
        for b in bc.iter().filter(|b| b.lang_code == lang) {
            let c = Self::chapter_commentary(Some(b), n);
            if !c.is_empty() {
                parts.push(Part::new(h1(&b.resource_type_name), rtl));
                parts.push(Part::new(c, rtl).sep(sep));
            }
        }
        parts
    }

    fn commentary_parts(&self, bc: Option<&BcBook>, n: u32, rtl: bool) -> Vec<Part> {
        let c = Self::chapter_commentary(bc, n);
        match (c.is_empty(), bc) {
            (false, Some(bc)) => vec![Part::new(h1(&bc.resource_type_name), rtl), Part::new(c, rtl).sep(self.sep())],
            _ => Vec::new(),
        }
    }

    fn notes_parts(&self, book: Option<&NoteBook>, n: u32, rtl: bool, two_col: bool, tnc_style: bool) -> Vec<Part> {
        let div = Self::notes_div(book, n, two_col, tnc_style);
        match (div.is_empty(), book) {
            (false, Some(book)) => vec![
                Part::new(h1(&book.resource_type_name), rtl),
                Part::new(div, rtl).two_col(two_col),
                self.hr_trick(),
            ],
            _ => Vec::new(),
        }
    }

    fn non_usfm_chapter(
        &self,
        tn: Option<&NoteBook>,
        tnc: Option<&NoteBook>,
        tq: Option<&NoteBook>,
        bc: Option<&BcBook>,
        n: u32,
        rtl: bool,
        tn2c: bool,
        tq2c: bool,
    ) -> Vec<Part> {
        let mut parts = self.commentary_parts(bc, n, rtl);
        parts.extend(self.notes_parts(tn, n, rtl, tn2c, false));
        parts.extend(self.notes_parts(tnc, n, rtl, tn2c, true));
        parts.extend(self.notes_parts(tq, n, rtl, tq2c, false));
        parts
    }

    fn usfm_and_tw(&self, rtn: &str, content: &str, tw: Option<&TwBook>, rtl: bool) -> Vec<Part> {
        let mut parts = vec![Part::new(h1(rtn), rtl), Part::new(content, rtl).sep(self.sep())];
        if let Some(tw) = tw {
            parts.extend(tw::content_parts(tw, content, self.tw_mode(), self.rule));
        }
        parts
    }

    fn usfm_and_tw_verse(&self, book_name: &str, n: u32, verse_ref: &str, rtn: &str, verse: &str, tw: Option<&TwBook>, rtl: bool) -> Vec<Part> {
        let mut parts = vec![
            Part::new(h1(&format!("{book_name} {n}:{verse_ref}")), rtl),
            Part::new(h1(rtn), rtl),
            Part::new(verse_span(verse), rtl).sep(self.sep()),
        ];
        if let Some(tw) = tw {
            parts.extend(tw::content_parts(tw, verse, self.tw_mode(), self.rule));
        }
        parts
    }

    /// `get_non_usfm_resources_verse`: TN, TNC, TQ fragments for one verse label.
    fn non_usfm_verse(
        &mut self,
        tn: Option<&NoteBook>,
        tnc: Option<&NoteBook>,
        tq: Option<&NoteBook>,
        label: &VerseLabel,
        n: u32,
        rtl: bool,
    ) -> Res<Vec<Part>> {
        let mut chapters: Vec<Option<&NoteChapter>> = Vec::new();
        for book in [tn, tnc, tq] {
            match book {
                None => chapters.push(None),
                Some(b) => match b.chapters.get(n) {
                    Some(c) => chapters.push(Some(c)),
                    None if self.compat.notes_missing_chapter_error => return Err(CoreError::KeyError { chapter: n }),
                    None => chapters.push(None),
                },
            }
        }
        let sep = self.sep();
        let mut parts = Vec::new();
        for (book, chapter) in [tn, tnc, tq].into_iter().zip(chapters) {
            if let (Some(book), Some(chapter)) = (book, chapter) {
                if let Some(v) = chapter.verses.iter().find(|v| v.label.matches(label, self.compat.verse_key_padding)) {
                    parts.push(Part::new(h1(&book.resource_type_name), rtl).sep(false));
                    parts.push(Part::new(v.html.clone(), rtl).sep(sep));
                }
            }
        }
        Ok(parts)
    }

    fn note_findings_for_chapter(&mut self, lang: &str, book: BookCode, n: u32, usfm_verses: &[Verse], notes: &[(&str, Option<&NoteBook>)]) {
        for (name, nb) in notes {
            let Some(chapter) = nb.and_then(|b| b.chapters.get(n)) else { continue };
            let unmatched: Vec<&str> = chapter
                .verses
                .iter()
                .filter(|v| !usfm_verses.iter().any(|u| u.label.key == v.label.key))
                .map(|v| v.label.raw.as_str())
                .collect();
            if !unmatched.is_empty() {
                self.findings.push(
                    Finding::new(FindingCode::NotesNeverMatched, Severity::Warning)
                        .lang(lang)
                        .book(book)
                        .chapter(n)
                        .resource(name)
                        .detail(unmatched.join(",")),
                );
            }
        }
    }

    fn usfm_chapter_findings(&mut self, book: &UsfmBook) {
        for (n, c) in book.chapters.iter() {
            if c.content.trim().is_empty() || c.verses.is_none() {
                self.findings.push(
                    Finding::new(FindingCode::UsfmChapterEmpty, Severity::Warning)
                        .lang(&book.lang_code)
                        .book(book.book_code)
                        .chapter(*n)
                        .resource(&book.resource_type_name),
                );
            }
        }
        if book.lang_code != "en" && book.national_book_name.trim().eq_ignore_ascii_case(book.book_code.english_name()) {
            self.findings.push(
                Finding::new(FindingCode::BookNameLooksEnglish, Severity::Info)
                    .lang(&book.lang_code)
                    .book(book.book_code)
                    .detail(book.national_book_name.clone()),
            );
        }
    }
}

// ================================================================== lbo / lvo

fn per_lang_book<'a>(ctx: &mut Ctx<'a>, lang: &str, book: BookCode) -> LangBook<'a> {
    let candidates: Vec<&UsfmBook> = ctx.books.usfm.iter().filter(|b| b.lang_code == lang && b.book_code == book).collect();
    let (usfm, usfm2) = ctx.pick_usfm(&candidates);
    let first_note = |v: &'a [NoteBook]| v.iter().find(|b| b.lang_code == lang && b.book_code == book);
    LangBook {
        usfm,
        usfm2,
        tn: first_note(&ctx.books.tn),
        tnc: first_note(&ctx.books.tnc),
        tq: first_note(&ctx.books.tq),
        tw: ctx.books.tw.iter().find(|b| b.lang_code == lang),
        bc: ctx.books.bc.iter().find(|b| b.lang_code == lang && b.book_code == book),
    }
}

struct LangBook<'a> {
    usfm: Option<&'a UsfmBook>,
    usfm2: Option<&'a UsfmBook>,
    tn: Option<&'a NoteBook>,
    tnc: Option<&'a NoteBook>,
    tq: Option<&'a NoteBook>,
    tw: Option<&'a TwBook>,
    bc: Option<&'a BcBook>,
}

fn by_book(ctx: &mut Ctx, verse: bool) -> Res<Vec<Part>> {
    let mut parts = Vec::new();
    let o = ctx.req.options.clone();
    for lang in ctx.lang_codes_all() {
        for book in ctx.book_codes() {
            let lb = per_lang_book(ctx, &lang, book);
            if let Some(usfm) = lb.usfm {
                ctx.usfm_chapter_findings(usfm);
                if verse {
                    parts.extend(usfm_by_verse_book(ctx, &lb, usfm)?);
                } else {
                    parts.extend(usfm_by_book(ctx, &lb, usfm)?);
                }
            } else if lb.tn.is_some() || lb.tnc.is_some() {
                parts.extend(tn_by_book(ctx, &lb));
            } else if lb.tq.is_some() {
                parts.extend(tq_by_book(ctx, &lb, o.show_bc_book_intro));
            } else if lb.tw.is_some() || lb.bc.is_some() {
                parts.extend(tw_by_book(ctx, &lb, o.show_bc_book_intro));
            }
        }
    }
    Ok(parts)
}

fn usfm_by_book(ctx: &mut Ctx, lb: &LangBook, usfm: &UsfmBook) -> Res<Vec<Part>> {
    let o = ctx.req.options.clone();
    let rtl = usfm.lang_direction.is_rtl();
    let mut parts = ctx.book_intros(lb.tn, lb.tnc, lb.bc, rtl, o.show_tn_book_intro, o.show_bc_book_intro);
    if parts.is_empty() {
        parts.push(Part::new(h1(&usfm.national_book_name), rtl));
    }
    for (n, chapter) in usfm.chapters.iter() {
        let n = *n;
        parts.extend(ctx.chapter_intros(lb.tn, lb.tnc, n, rtl, o.show_tn_chapter_intro));
        parts.extend(ctx.usfm_and_tw(&usfm.resource_type_name, &chapter.content, lb.tw, rtl));
        parts.extend(ctx.non_usfm_chapter(lb.tn, lb.tnc, lb.tq, lb.bc, n, rtl, o.use_two_column_layout_for_tn_notes, o.use_two_column_layout_for_tq_notes));
        if let Some(s) = lb.usfm2 {
            if let Some(c2) = s.chapters.get(n) {
                parts.extend(ctx.usfm_and_tw(&s.resource_type_name, &c2.content, lb.tw, rtl));
            } else {
                ctx.findings.push(
                    Finding::new(FindingCode::SecondaryChapterMissing, Severity::Warning)
                        .lang(&usfm.lang_code)
                        .book(usfm.book_code)
                        .chapter(n)
                        .resource(&s.resource_type_name),
                );
            }
        }
    }
    Ok(parts)
}

fn usfm_by_verse_book(ctx: &mut Ctx, lb: &LangBook, usfm: &UsfmBook) -> Res<Vec<Part>> {
    let o = ctx.req.options.clone();
    let rtl = usfm.lang_direction.is_rtl();
    let mut parts = ctx.book_intros(lb.tn, lb.tnc, lb.bc, rtl, o.show_tn_book_intro, o.show_bc_book_intro);
    for (n, chapter) in usfm.chapters.iter() {
        let n = *n;
        parts.extend(ctx.chapter_intros(lb.tn, lb.tnc, n, rtl, o.show_tn_chapter_intro));
        let Some(verses) = chapter.verses.as_ref().filter(|v| !v.is_empty()) else { continue };
        ctx.note_findings_for_chapter(&usfm.lang_code, usfm.book_code, n, verses, &[("tn", lb.tn), ("tnc", lb.tnc), ("tq", lb.tq)]);
        for verse in verses {
            parts.extend(ctx.usfm_and_tw_verse(&usfm.national_book_name, n, &verse.label.raw, &usfm.resource_type_name, &verse.html, lb.tw, rtl));
            parts.extend(ctx.non_usfm_verse(lb.tn, lb.tnc, lb.tq, &verse.label, n, rtl)?);
            if let Some(s) = lb.usfm2 {
                let Some(c2) = s.chapters.get(n) else {
                    if ctx.compat.secondary_missing_chapter_error {
                        return Err(CoreError::KeyError { chapter: n });
                    }
                    ctx.findings.push(
                        Finding::new(FindingCode::SecondaryChapterMissing, Severity::Warning)
                            .lang(&usfm.lang_code)
                            .book(usfm.book_code)
                            .chapter(n)
                            .resource(&s.resource_type_name),
                    );
                    continue;
                };
                if let Some(v2) = c2.verses.as_ref().and_then(|vs| vs.iter().find(|v| v.label.matches(&verse.label, ctx.compat.verse_key_padding))) {
                    parts.extend(ctx.usfm_and_tw_verse(&s.national_book_name, n, &verse.label.raw, &s.resource_type_name, &v2.html, lb.tw, rtl));
                }
            }
        }
    }
    Ok(parts)
}

fn tn_by_book(ctx: &mut Ctx, lb: &LangBook) -> Vec<Part> {
    let o = ctx.req.options.clone();
    let Some(tn) = lb.tn else { return Vec::new() };
    let rtl = tn.lang_direction.is_rtl();
    let mut parts = ctx.book_intros(Some(tn), lb.tnc, lb.bc, rtl, o.show_tn_book_intro, o.show_bc_book_intro);
    for n in tn.chapters.numbers() {
        parts.extend(ctx.chapter_intros(Some(tn), lb.tnc, n, rtl, o.show_tn_chapter_intro));
        parts.extend(ctx.non_usfm_chapter(Some(tn), lb.tnc, lb.tq, lb.bc, n, rtl, o.use_two_column_layout_for_tn_notes, o.use_two_column_layout_for_tq_notes));
    }
    parts
}

fn tq_by_book(ctx: &mut Ctx, lb: &LangBook, show_bc: bool) -> Vec<Part> {
    let o = ctx.req.options.clone();
    let Some(tq) = lb.tq else { return Vec::new() };
    let rtl = tq.lang_direction.is_rtl();
    let mut parts = ctx.book_intros(lb.tn, lb.tnc, lb.bc, rtl, false, show_bc);
    for n in tq.chapters.numbers() {
        parts.extend(ctx.chapter_intros(lb.tn, lb.tnc, n, rtl, false));
        parts.extend(ctx.non_usfm_chapter(lb.tn, lb.tnc, Some(tq), lb.bc, n, rtl, false, o.use_two_column_layout_for_tq_notes));
    }
    parts
}

fn tw_by_book(ctx: &mut Ctx, lb: &LangBook, show_bc: bool) -> Vec<Part> {
    let rtl = lb.tw.map(|t| t.lang_direction.is_rtl()).unwrap_or(false);
    let mut parts = ctx.book_intros(lb.tn, lb.tnc, lb.bc, rtl, false, show_bc);
    if let Some(bc) = lb.bc {
        for n in bc.chapters.numbers() {
            parts.extend(ctx.chapter_intros(lb.tn, lb.tnc, n, rtl, false));
            parts.extend(ctx.non_usfm_chapter(lb.tn, lb.tnc, lb.tq, lb.bc, n, rtl, false, false));
        }
    }
    parts
}

// ================================================================== blo / bvo

/// The per-book selections `filter_books_by_book_code` makes.
struct BookSel<'a> {
    usfm: Vec<&'a UsfmBook>,
    tn: Vec<&'a NoteBook>,
    tnc: Vec<&'a NoteBook>,
    tq: Vec<&'a NoteBook>,
    bc: Vec<&'a BcBook>,
}

fn select_book<'a>(ctx: &Ctx<'a>, book: BookCode) -> BookSel<'a> {
    BookSel {
        usfm: ctx.books.usfm.iter().filter(|b| b.book_code == book).collect(),
        tn: ctx.books.tn.iter().filter(|b| b.book_code == book).collect(),
        tnc: ctx.books.tnc.iter().filter(|b| b.book_code == book).collect(),
        tq: ctx.books.tq.iter().filter(|b| b.book_code == book).collect(),
        bc: ctx.books.bc.iter().filter(|b| b.book_code == book).collect(),
    }
}

fn chapter_range(ctx: &mut Ctx, book: BookCode, sel: &BookSel) -> u32 {
    let table = book.chapter_count();
    let present = sel
        .usfm
        .iter()
        .flat_map(|b| b.chapters.numbers())
        .chain(sel.tn.iter().chain(&sel.tnc).chain(&sel.tq).flat_map(|b| b.chapters.numbers()))
        .chain(sel.bc.iter().flat_map(|b| b.chapters.numbers()))
        .max()
        .unwrap_or(0);
    if present > table {
        ctx.findings.push(
            Finding::new(FindingCode::ChapterBeyondTable, Severity::Warning)
                .book(book)
                .chapter(present)
                .detail(format!("static table says {table}")),
        );
    }
    if ctx.compat.static_chapter_table {
        table
    } else {
        table.max(present)
    }
}

fn first_dir(sel: &BookSel) -> bool {
    sel.usfm.first().map(|b| b.lang_direction.is_rtl()).unwrap_or(false)
}

/// Direction a language's parts get: the first USFM book's (as built) or the language's own.
fn lang_rtl(ctx: &Ctx, sel: &BookSel, lang: &str) -> bool {
    if ctx.compat.first_book_direction {
        return first_dir(sel);
    }
    let own = sel
        .usfm
        .iter()
        .find(|b| b.lang_code == lang)
        .map(|b| b.lang_direction)
        .or_else(|| sel.tn.iter().chain(&sel.tnc).chain(&sel.tq).find(|b| b.lang_code == lang).map(|b| b.lang_direction))
        .or_else(|| sel.bc.iter().find(|b| b.lang_code == lang).map(|b| b.lang_direction));
    own.map(Direction::is_rtl).unwrap_or(false)
}

fn by_chapter(ctx: &mut Ctx, verse: bool) -> Res<Vec<Part>> {
    let mut parts = Vec::new();
    if verse {
        if ctx.books.usfm.is_empty() {
            return by_chapter(ctx, false);
        }
        if !matches!(ctx.req.layout, Layout::OneColumn | Layout::OneColumnCompact) {
            return Ok(parts);
        }
    }
    for book in ctx.book_codes() {
        let sel = select_book(ctx, book);
        if !sel.usfm.is_empty() {
            if verse {
                parts.extend(usfm_by_verse_chapter(ctx, book, &sel)?);
            } else {
                parts.extend(usfm_by_chapter(ctx, book, &sel)?);
            }
        } else if verse {
            // as built: with USFM present for other books, a USFM-less book emits nothing
        } else if !sel.tn.is_empty() || !sel.tnc.is_empty() {
            parts.extend(tn_by_chapter(ctx, book, &sel));
        } else if !sel.tq.is_empty() {
            parts.extend(tq_by_chapter(ctx, book, &sel));
        } else if !ctx.books.tw.is_empty() || !sel.bc.is_empty() {
            parts.extend(tw_by_chapter(ctx, &sel));
        }
    }
    Ok(parts)
}

/// Per-language USFM pair inside a book (blo/bvo select from the book's USFM list).
fn lang_usfm<'a>(ctx: &mut Ctx<'a>, sel: &BookSel<'a>, lang: &str) -> (Option<&'a UsfmBook>, Option<&'a UsfmBook>) {
    let candidates: Vec<&UsfmBook> = sel.usfm.iter().copied().filter(|b| b.lang_code == lang).collect();
    ctx.pick_usfm(&candidates)
}

fn usfm_by_chapter<'a>(ctx: &mut Ctx<'a>, book: BookCode, sel: &BookSel<'a>) -> Res<Vec<Part>> {
    let o = ctx.req.options.clone();
    let num_chapters = chapter_range(ctx, book, sel);
    for b in &sel.usfm {
        ctx.usfm_chapter_findings(b);
    }
    let all_tw = ctx.all_tw();
    let lang_codes = collect_lang_codes(&sel.usfm, &sel.tn, &sel.tnc, &sel.tq, &all_tw, &sel.bc);
    let group_rtl = first_dir(sel);
    let mut parts = ctx.book_intros_for_groups(&sel.tn, &sel.tnc, &sel.bc, group_rtl, o.show_tn_book_intro, o.show_bc_book_intro, h2_book_resource);
    // lang -> (primary, secondary) is chapter independent; resolve once so findings are not repeated
    let mut picks: Vec<(String, Option<&UsfmBook>, Option<&UsfmBook>, Option<&TwBook>, bool)> = Vec::new();
    for lang in &lang_codes {
        let (p, s) = lang_usfm(ctx, sel, lang);
        let tw = ctx.books.tw.iter().find(|t| t.lang_code == *lang);
        picks.push((lang.clone(), p, s, tw, lang_rtl(ctx, sel, lang)));
    }
    for n in 1..=num_chapters {
        for (lang, usfm, usfm2, tw, rtl) in &picks {
            let rtl = *rtl;
            if let Some(usfm) = usfm {
                parts.push(Part::new(h2_book(&usfm.national_book_name), rtl));
                if let Some(c) = usfm.chapters.get(n) {
                    parts.extend(ctx.usfm_and_tw(&usfm.resource_type_name, &c.content, *tw, rtl));
                }
            }
            parts.extend(ctx.chapter_intros_for_groups(&sel.tn, &sel.tnc, &sel.bc, n, lang, rtl));
            if ctx.compat.duplicate_bc_commentary {
                for bc in sel.bc.iter().filter(|b| b.lang_code == *lang) {
                    parts.extend(ctx.commentary_parts(Some(bc), n, rtl));
                }
            }
            for tn in sel.tn.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tn), n, rtl, o.use_two_column_layout_for_tn_notes, false));
            }
            for tnc in sel.tnc.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tnc), n, rtl, o.use_two_column_layout_for_tn_notes, true));
            }
            let tq_flag = if ctx.compat.blo_tq_uses_tn_two_column_flag { o.use_two_column_layout_for_tn_notes } else { o.use_two_column_layout_for_tq_notes };
            for tq in sel.tq.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tq), n, rtl, tq_flag, false));
            }
            if let Some(s) = usfm2 {
                match s.chapters.get(n) {
                    Some(c) => parts.extend(ctx.usfm_and_tw(&s.resource_type_name, &c.content, *tw, rtl)),
                    None if ctx.compat.secondary_missing_chapter_error => return Err(CoreError::KeyError { chapter: n }),
                    None => ctx.findings.push(
                        Finding::new(FindingCode::SecondaryChapterMissing, Severity::Warning)
                            .lang(lang)
                            .book(book)
                            .chapter(n)
                            .resource(&s.resource_type_name),
                    ),
                }
            }
        }
    }
    Ok(parts)
}

fn h2_book_resource(name: &str) -> String {
    // blo's group intros use `<h2 class='book-name'>` (BOOK_NAME_FMT_STR) is NOT used for
    // intro headings: DOC passes RESOURCE_TYPE_NAME_FMT_STR (h1) in the group helpers.
    h1(name)
}

fn tn_by_chapter(ctx: &mut Ctx, book: BookCode, sel: &BookSel) -> Vec<Part> {
    let o = ctx.req.options.clone();
    let num_chapters = chapter_range(ctx, book, sel);
    let all_tw = ctx.all_tw();
    let lang_codes = collect_lang_codes(&sel.usfm, &sel.tn, &sel.tnc, &sel.tq, &all_tw, &sel.bc);
    let tn_rtl = sel.tn.first().map(|b| b.lang_direction.is_rtl()).unwrap_or(false);
    let tnc_rtl = sel.tnc.first().map(|b| b.lang_direction.is_rtl()).unwrap_or(false);
    let group_rtl = tn_rtl || tnc_rtl;
    let mut parts = ctx.book_intros_for_groups(&sel.tn, &sel.tnc, &sel.bc, group_rtl, o.show_tn_book_intro, o.show_bc_book_intro, h2_book_resource);
    for n in 1..=num_chapters {
        for lang in &lang_codes {
            let rtl = if ctx.compat.first_book_direction { group_rtl } else { lang_rtl(ctx, sel, lang) };
            parts.extend(ctx.chapter_intros_for_groups(&sel.tn, &sel.tnc, &sel.bc, n, lang, rtl));
            for tn in sel.tn.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tn), n, rtl, o.use_two_column_layout_for_tn_notes, false));
            }
            for tnc in sel.tnc.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tnc), n, rtl, o.use_two_column_layout_for_tn_notes, true));
            }
            for tq in sel.tq.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tq), n, rtl, o.use_two_column_layout_for_tq_notes, false));
            }
        }
    }
    parts
}

fn tq_by_chapter(ctx: &mut Ctx, book: BookCode, sel: &BookSel) -> Vec<Part> {
    let o = ctx.req.options.clone();
    let num_chapters = chapter_range(ctx, book, sel);
    let all_tw = ctx.all_tw();
    let lang_codes = collect_lang_codes(&sel.usfm, &sel.tn, &sel.tnc, &sel.tq, &all_tw, &sel.bc);
    let group_rtl = sel.tq.first().map(|b| b.lang_direction.is_rtl()).unwrap_or(false);
    let mut parts = Vec::new();
    for n in 1..=num_chapters {
        for lang in &lang_codes {
            let rtl = if ctx.compat.first_book_direction { group_rtl } else { lang_rtl(ctx, sel, lang) };
            parts.push(Part::new(format!("Chapter {n}"), rtl));
            for bc in sel.bc.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.commentary_parts(Some(bc), n, rtl));
            }
            for tq in sel.tq.iter().filter(|b| b.lang_code == *lang) {
                parts.extend(ctx.notes_parts(Some(tq), n, rtl, o.use_two_column_layout_for_tq_notes, false));
            }
        }
    }
    parts
}

fn tw_by_chapter(ctx: &mut Ctx, sel: &BookSel) -> Vec<Part> {
    let o = ctx.req.options.clone();
    let rtl = ctx.books.tw.first().map(|t| t.lang_direction.is_rtl()).unwrap_or(false);
    let mut parts = Vec::new();
    for bc in &sel.bc {
        parts.extend(ctx.book_intros(None, None, Some(bc), rtl, false, o.show_bc_book_intro));
        for n in bc.chapters.numbers() {
            parts.extend(ctx.chapter_intros(None, None, n, rtl, false));
            parts.extend(ctx.non_usfm_chapter(None, None, None, Some(bc), n, rtl, false, false));
        }
    }
    parts
}

fn usfm_by_verse_chapter<'a>(ctx: &mut Ctx<'a>, book: BookCode, sel: &BookSel<'a>) -> Res<Vec<Part>> {
    let o = ctx.req.options.clone();
    let num_chapters = chapter_range(ctx, book, sel);
    for b in &sel.usfm {
        ctx.usfm_chapter_findings(b);
    }
    let group_rtl = first_dir(sel);
    let mut parts = Vec::new();
    if o.show_tn_book_intro {
        for b in sel.tn.iter().chain(sel.tnc.iter()) {
            if !b.book_intro.is_empty() {
                parts.push(Part::new(h1(&b.resource_type_name), group_rtl));
                parts.push(Part::new(demote_headings_by_one(&b.book_intro), group_rtl).sep(ctx.sep()));
            }
        }
    }
    if o.show_bc_book_intro {
        for b in &sel.bc {
            if !b.book_intro.is_empty() {
                parts.push(Part::new(h1(&b.resource_type_name), group_rtl));
                parts.push(Part::new(demote_headings_by_one(&b.book_intro), group_rtl).sep(ctx.sep()));
            }
        }
    }
    let all_tw = ctx.all_tw();
    let lang_codes = collect_lang_codes(&sel.usfm, &sel.tn, &sel.tnc, &sel.tq, &all_tw, &sel.bc);
    let mut picks: Vec<(String, Option<&UsfmBook>, Option<&UsfmBook>, Option<&TwBook>, bool)> = Vec::new();
    for lang in &lang_codes {
        let (p, s) = lang_usfm(ctx, sel, lang);
        let tw = ctx.books.tw.iter().find(|t| t.lang_code == *lang);
        picks.push((lang.clone(), p, s, tw, lang_rtl(ctx, sel, lang)));
    }
    for n in 1..=num_chapters {
        for (lang, usfm, usfm2, tw, rtl) in &picks {
            let rtl = *rtl;
            let tn = sel.tn.iter().find(|b| b.lang_code == *lang).copied();
            let tnc = sel.tnc.iter().find(|b| b.lang_code == *lang).copied();
            let tq = sel.tq.iter().find(|b| b.lang_code == *lang).copied();
            let usfm_chapter = usfm.and_then(|b| b.chapters.get(n));
            let usfm2_chapter = usfm2.and_then(|b| b.chapters.get(n));
            if o.show_bc_chapter_commentary {
                for bc in sel.bc.iter().filter(|b| b.lang_code == *lang) {
                    parts.extend(ctx.commentary_parts(Some(bc), n, rtl));
                }
            }
            if o.show_tn_chapter_intro {
                for b in sel.tn.iter().filter(|b| b.lang_code == *lang) {
                    let intro = Ctx::tn_chapter_intro(Some(b), n);
                    if !intro.is_empty() {
                        parts.push(Part::new(h1(&b.resource_type_name), rtl));
                        parts.push(Part::new(intro, rtl).sep(ctx.sep()));
                    }
                }
                for b in sel.tnc.iter().filter(|b| b.lang_code == *lang) {
                    let intro = ctx.tnc_chapter_intro(Some(b), n);
                    if !intro.is_empty() {
                        parts.push(Part::new(h1(&b.resource_type_name), rtl));
                        parts.push(Part::new(intro, rtl).sep(ctx.sep()));
                    }
                }
            }
            let (Some(usfm), Some(chapter)) = (usfm, usfm_chapter) else { continue };
            let Some(primary_verses) = chapter.verses.as_ref().filter(|v| !v.is_empty()) else { continue };
            let secondary_verses = usfm2_chapter.and_then(|c| c.verses.as_ref()).filter(|v| !v.is_empty());
            ctx.note_findings_for_chapter(lang, book, n, primary_verses, &[("tn", tn), ("tnc", tnc), ("tq", tq)]);
            for verse in primary_verses {
                parts.extend(ctx.usfm_and_tw_verse(&usfm.national_book_name, n, &verse.label.raw, &usfm.resource_type_name, &verse.html, *tw, rtl));
                for (nb, _) in [(tn, 0), (tnc, 1), (tq, 2)] {
                    let Some(nb) = nb else { continue };
                    let Some(nch) = nb.chapters.get(n) else { continue };
                    if let Some(v) = nch.verses.iter().find(|v| v.label.matches(&verse.label, ctx.compat.verse_key_padding)) {
                        parts.push(Part::new(h1(&nb.resource_type_name), rtl));
                        parts.push(Part::new(v.html.clone(), rtl).sep(ctx.sep()));
                    }
                }
                if let (Some(s), Some(sv)) = (usfm2, secondary_verses) {
                    if let Some(v2) = sv.iter().find(|v| v.label.matches(&verse.label, ctx.compat.verse_key_padding)) {
                        parts.extend(ctx.usfm_and_tw_verse(&s.national_book_name, n, &verse.label.raw, &s.resource_type_name, &v2.html, *tw, rtl));
                    }
                }
            }
        }
    }
    Ok(parts)
}

/// Print rule (captain 2026-11-25): printing with "include definitions" selected links out.
pub fn effective_tw_mode(req: &ValidRequest, compat: BugCompat) -> TwMode {
    if req.tw_mode.includes_definitions() && req.options.layout_for_print && !compat.include_print_dead_links {
        TwMode::Link
    } else {
        req.tw_mode
    }
}

// ================================================================== entry

/// `assemble_content`: the interleaver for the request's strategy, then (include mode,
/// not print) one Translation Words definitions section per language.
pub fn build(req: &ValidRequest, books: &Books, compat: BugCompat) -> Result<Output, CoreError> {
    let rule = tw::default_rule(compat.tw_python_word_boundary);
    build_with(req, books, compat, rule.as_ref())
}

/// [`build`] with the Translation Words word rule chosen by the caller (the shell may supply
/// kitchen's `words/1` through `doc-canon`).
pub fn build_with(req: &ValidRequest, books: &Books, compat: BugCompat, rule: &dyn doc_model::text::WordRule) -> Result<Output, CoreError> {
    let mut ctx = Ctx { req, books, compat, findings: Vec::new(), rule };
    let mut parts = match req.strategy {
        Strategy::Lbo => by_book(&mut ctx, false)?,
        Strategy::Lvo => by_book(&mut ctx, true)?,
        Strategy::Blo => by_chapter(&mut ctx, false)?,
        Strategy::Bvo => by_chapter(&mut ctx, true)?,
        Strategy::Stet => Vec::new(),
    };
    if !books.tw.is_empty() && req.tw_mode.includes_definitions() && req.options.layout_for_print && !compat.include_print_dead_links {
        ctx.findings.push(
            Finding::new(FindingCode::TwIncludeForcedToLinkOut, Severity::Warning)
                .detail("include definitions was selected with print layout; Translation Words are linked out instead"),
        );
    }
    if !books.tw.is_empty() && req.tw_mode.includes_definitions() && !req.options.layout_for_print {
        let mut seen: Vec<&str> = Vec::new();
        for tw_book in &books.tw {
            if seen.contains(&tw_book.lang_code.as_str()) {
                continue;
            }
            seen.push(&tw_book.lang_code);
            let section_book = if req.tw_mode == TwMode::IncludeUsed {
                let html: Vec<&str> = books
                    .usfm
                    .iter()
                    .filter(|u| u.lang_code == tw_book.lang_code)
                    .flat_map(|u| u.chapters.iter().map(|(_, c)| c.content.as_str()))
                    .collect();
                if html.is_empty() {
                    ctx.findings.push(Finding::new(FindingCode::TwUsedWordsWithoutScripture, Severity::Warning).lang(&tw_book.lang_code));
                    tw_book.clone()
                } else {
                    tw::used_entries(tw_book, &html, ctx.rule)
                }
            } else {
                tw_book.clone()
            };
            parts.push(Part::new(tw::definitions_section(&section_book, &req.resources, &books.usfm), tw_book.lang_direction.is_rtl()));
            parts.push(ctx.hr_trick());
        }
    }
    if rule.name() != "python_b" {
        // captain 2026-11-25: the kitchen rule is applied to every script; for the scripts without
        // spaces between words it carries a finding (no rule is right there without a dictionary)
        for tw_book in &books.tw {
            if tw::is_no_space_lang(&tw_book.lang_code) {
                ctx.findings.push(
                    Finding::new(FindingCode::TwWordRuleNoSpaceScript, Severity::Warning)
                        .lang(&tw_book.lang_code)
                        .resource(&tw_book.resource_type_name)
                        .detail(format!("word rule {} cannot segment a script written without spaces; Translation Words matches may be missing or spurious", rule.name())),
                );
            }
        }
    }
    let mut findings: Vec<Finding> = Vec::new();
    for f in ctx.findings {
        if !findings.contains(&f) {
            findings.push(f);
        }
    }
    Ok(Output { parts, findings })
}
