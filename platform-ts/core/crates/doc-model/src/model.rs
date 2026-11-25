//! The typed-book seam: what DOC's parsers hand to the interleavers, and what the
//! interleavers hand back. All of it is plain data (serde), so the core is JSON in,
//! JSON out.
//!
//! Stage-1 defects made unrepresentable here:
//! * D1 (verse key padding): [`VerseLabel`] is parsed once at the boundary, so "01"
//!   and "1" are the same [`VerseKey`] (the as-built string comparison is kept behind
//!   `BugCompat::verse_key_padding`).
//! * D7 (`KeyError`/unknown book): [`BookCode`] only exists for canon books; an
//!   unknown code fails at the boundary, not three calls later.

use serde::{Deserialize, Serialize};

use crate::tables;

/// A canon book, as an index into [`tables::BOOKS`] (so `Ord` is canonical order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BookCode(u8);

impl BookCode {
    pub fn parse(code: &str) -> Option<BookCode> {
        tables::BOOKS
            .iter()
            .position(|(c, _, _)| *c == code)
            .and_then(|i| u8::try_from(i).ok())
            .map(BookCode)
    }
    pub fn as_str(self) -> &'static str {
        tables::BOOKS[usize::from(self.0)].0
    }
    pub fn english_name(self) -> &'static str {
        tables::BOOKS[usize::from(self.0)].1
    }
    /// DOC's static chapter count (`BOOK_CHAPTERS`).
    pub fn chapter_count(self) -> u32 {
        tables::BOOKS[usize::from(self.0)].2
    }
}

impl TryFrom<String> for BookCode {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        BookCode::parse(&value).ok_or_else(|| format!("{value} is not a valid book code"))
    }
}
impl Serialize for BookCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for BookCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        BookCode::try_from(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        self == Direction::Rtl
    }
}

/// A verse label parsed once. `raw` keeps the producer's text for as-built equality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseLabel {
    pub raw: String,
    pub key: VerseKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerseKey {
    Single(u32),
    /// A verse bridge such as `16-17`.
    Span(u32, u32),
    Other(String),
}

impl VerseLabel {
    pub fn parse(raw: &str) -> VerseLabel {
        let trimmed = raw.trim();
        let number = |s: &str| -> Option<u32> {
            if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
                s.parse().ok()
            } else {
                None
            }
        };
        let key = if let Some(n) = number(trimmed) {
            VerseKey::Single(n)
        } else if let Some((Some(a), Some(b))) = trimmed.split_once('-').map(|(a, b)| (number(a), number(b))) {
            VerseKey::Span(a, b)
        } else {
            VerseKey::Other(trimmed.to_string())
        };
        VerseLabel { raw: raw.to_string(), key }
    }
    /// Label equality: the as-built string comparison, or the parsed key.
    pub fn matches(&self, other: &VerseLabel, as_built_string_keys: bool) -> bool {
        if as_built_string_keys {
            self.raw == other.raw
        } else {
            self.key == other.key
        }
    }
}

/// One verse: label plus HTML. JSON form `["01", "<p>..."]`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(from = "(String, String)", into = "(String, String)")]
pub struct Verse {
    pub label: VerseLabel,
    pub html: String,
}
impl From<Verse> for (String, String) {
    fn from(v: Verse) -> Self {
        (v.label.raw, v.html)
    }
}
impl From<(String, String)> for Verse {
    fn from((raw, html): (String, String)) -> Self {
        Verse { label: VerseLabel::parse(&raw), html }
    }
}

/// Chapters in the producer's insertion order (DOC's `dict[int, ...]`).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Chapters<T>(pub Vec<(u32, T)>);

impl<T> Chapters<T> {
    pub fn get(&self, n: u32) -> Option<&T> {
        self.0.iter().find(|(k, _)| *k == n).map(|(_, v)| v)
    }
    pub fn contains(&self, n: u32) -> bool {
        self.0.iter().any(|(k, _)| *k == n)
    }
    pub fn iter(&self) -> impl Iterator<Item = &(u32, T)> {
        self.0.iter()
    }
    pub fn numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().map(|(k, _)| *k)
    }
}
impl<T> Default for Chapters<T> {
    fn default() -> Self {
        Chapters(Vec::new())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UsfmChapter {
    pub content: String,
    /// `None` when USFM conversion failed for the chapter (DOC: `verses=None`).
    pub verses: Option<Vec<Verse>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UsfmBook {
    pub lang_code: String,
    #[serde(default)]
    pub lang_name: String,
    #[serde(default)]
    pub localized_lang_name: String,
    pub book_code: BookCode,
    pub national_book_name: String,
    pub resource_type_name: String,
    #[serde(default)]
    pub lang_direction: Direction,
    pub chapters: Chapters<UsfmChapter>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct NoteChapter {
    #[serde(default)]
    pub intro_html: String,
    #[serde(default)]
    pub verses: Vec<Verse>,
}

/// Translation Notes, Condensed Translation Notes and Translation Questions share a
/// shape (TQ has no intros).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct NoteBook {
    pub lang_code: String,
    #[serde(default)]
    pub lang_name: String,
    pub book_code: BookCode,
    pub resource_type_name: String,
    #[serde(default)]
    pub lang_direction: Direction,
    #[serde(default)]
    pub book_intro: String,
    pub chapters: Chapters<NoteChapter>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TwEntry {
    pub localized_word: String,
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TwBook {
    pub lang_code: String,
    #[serde(default)]
    pub lang_name: String,
    pub resource_type_name: String,
    #[serde(default)]
    pub lang_direction: Direction,
    pub name_content_pairs: Vec<TwEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BcChapter {
    pub commentary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BcBook {
    pub lang_code: String,
    #[serde(default)]
    pub lang_name: String,
    pub book_code: BookCode,
    pub resource_type_name: String,
    #[serde(default)]
    pub lang_direction: Direction,
    #[serde(default)]
    pub book_intro: String,
    pub chapters: Chapters<BcChapter>,
}

/// The six lists DOC's `parsing.books` returns (request order preserved).
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Books {
    #[serde(default)]
    pub usfm: Vec<UsfmBook>,
    #[serde(default)]
    pub tn: Vec<NoteBook>,
    #[serde(default)]
    pub tnc: Vec<NoteBook>,
    #[serde(default)]
    pub tq: Vec<NoteBook>,
    #[serde(default)]
    pub tw: Vec<TwBook>,
    #[serde(default)]
    pub bc: Vec<BcBook>,
}

/// DOC's `DocumentPart`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Part {
    pub content: String,
    pub is_rtl: bool,
    pub contained_in_two_column_section: bool,
    pub add_page_break: bool,
    pub use_section_visual_separator: bool,
}

impl Part {
    pub fn new(content: impl Into<String>, is_rtl: bool) -> Part {
        Part {
            content: content.into(),
            is_rtl,
            contained_in_two_column_section: false,
            add_page_break: false,
            use_section_visual_separator: false,
        }
    }
    pub fn sep(mut self, on: bool) -> Part {
        self.use_section_visual_separator = on;
        self
    }
    pub fn two_col(mut self, on: bool) -> Part {
        self.contained_in_two_column_section = on;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// What a shell may show, log or ignore. The first renderer ignores them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    /// A requested (language, type, book) had no repository (reported by the shell).
    UnresolvedRequest,
    /// A requested resource produced no book object although the lookup succeeded.
    RequestedResourceAbsent,
    UsfmChapterEmpty,
    /// Secondary USFM lacks a chapter the primary has (DOC: `KeyError` in blo/bvo).
    SecondaryChapterMissing,
    /// A language has 0 or 3+ USFM books for a book: as built, scripture is dropped.
    UsfmArityUnsupported,
    /// Notes verse labels that match no USFM verse label (D1, bridges, versification).
    NotesNeverMatched,
    /// Notes (TN/TNC/TQ) have a chapter that USFM lacks, in a verse strategy.
    NotesChapterWithoutScripture,
    /// A source chapter number beyond the static `BOOK_CHAPTERS` table.
    ChapterBeyondTable,
    /// `national_book_name` equals the English name for a non-English language.
    BookNameLooksEnglish,
    /// "Include definitions" was chosen with a print layout: Translation Words are linked out.
    TwIncludeForcedToLinkOut,
    /// `include_used` was chosen but no scripture is part of the request: all definitions kept.
    TwUsedWordsWithoutScripture,
    /// A TN cross-reference could not become a working link: the note's resource was not
    /// requested, or the note file does not exist. It is printed as plain text.
    XrefTargetMissing,
    /// A cross-reference link points at an id that the finished document does not define
    /// (the target was dropped by the interleave strategy); it is printed as plain text.
    XrefDeadInOutput,
    /// The USFM parser (onion) reported a defect class in the book (detail: kitchen's lint code and count).
    UsfmLint,
    /// A `\c` designator is malformed (`\c 12b`): the chapter has no usable number.
    UsfmChapterNumberMalformed,
    /// The book has no `\id` (DOC's Writer assembly never writes one).
    UsfmBookIdMissing,
    /// A verse number is written glued to the verse text (`\v 14Ɖeka`): the designator and the first
    /// word cannot be told apart by a parser. (P3b adapter lint, see `doc-canon::kitchen_gaps`.)
    UsfmVerseNumberGlued,
    /// D36 repair: a verse was inferred from a bare number in a chapter that has no `\v` (detail: chapter:verse).
    UsfmVerseInferred,
    /// D36: a chapter without `\v` has numbers that might be verse numbers but the pattern is ambiguous; the text is kept as written.
    UsfmVersesAmbiguous,
    /// The Translation Words rule in force cannot segment this script (Thai, Khmer, Lao, Myanmar write
    /// no spaces between words); matches may be missing or spurious. P3b: attached whenever kitchen's
    /// `words/1` (the default rule) is applied to such a language.
    TwWordRuleNoSpaceScript,
}

/// CSS class carried by every working cross-reference link (one selector switches them all).
pub const XREF_CLASS: &str = "doc-xref";
/// The print rule for cross-reference links (captain, 2026-11-25: printed links carry no
/// underline and no color; toner). The shell adds this to the print stylesheet; for docx the
/// links already pass through DOC's `PlainHyperlinkChar0` override.
pub const XREF_PRINT_CSS: &str = "a.doc-xref, a.doc-xref:visited { color: inherit; text-decoration: none; }";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: FindingCode,
    pub severity: Severity,
    pub lang: Option<String>,
    pub book: Option<String>,
    pub chapter: Option<u32>,
    pub resource: Option<String>,
    pub detail: Option<String>,
}

impl Finding {
    pub fn new(code: FindingCode, severity: Severity) -> Finding {
        Finding { code, severity, lang: None, book: None, chapter: None, resource: None, detail: None }
    }
    pub fn lang(mut self, v: &str) -> Finding {
        self.lang = Some(v.to_string());
        self
    }
    pub fn book(mut self, v: BookCode) -> Finding {
        self.book = Some(v.as_str().to_string());
        self
    }
    pub fn chapter(mut self, v: u32) -> Finding {
        self.chapter = Some(v);
        self
    }
    pub fn resource(mut self, v: &str) -> Finding {
        self.resource = Some(v.to_string());
        self
    }
    pub fn detail(mut self, v: impl Into<String>) -> Finding {
        self.detail = Some(v.into());
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Output {
    pub parts: Vec<Part>,
    pub findings: Vec<Finding>,
}
