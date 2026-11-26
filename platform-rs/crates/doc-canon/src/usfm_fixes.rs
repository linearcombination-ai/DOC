//! DOC's on-the-fly USFM repairs (`usfm_error_detection_and_fixes.py`, applied by `parsing.split_usfm_by_chapters`), ported rule for rule.
//!
//! DOC fixes only resources it knows to be defective: with `CHECK_ALL_BOOKS_FOR_LANGUAGE` (on in DOC's `.env`) every book of a language that has any
//! known defective resource, else exactly the listed (language, type, book) triples. The fixes work on one chapter's text (what follows the
//! `\c <n>` marker) and are regular expressions with look-behind, so this module uses `fancy-regex`. Where Python behaviour differs from Rust's
//! (Unicode digits in `int()`, a pattern built from text that is not a valid regular expression) the difference is noted at the line.

use std::sync::LazyLock;

use fancy_regex::{Captures, Regex};

use crate::usfm_fixes_tables::RESOURCES_WITH_USFM_DEFECTS;

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("bad built-in pattern {pattern}: {e}"))
}

static CHAPTER: LazyLock<Regex> = LazyLock::new(|| rx(r"\\c\s+\d+"));
static CHAPTER_LABEL: LazyLock<Regex> = LazyLock::new(|| rx(r"\\cl\s+[^\n]+"));
static DOT_AFTER_VERSE: LazyLock<Regex> = LazyLock::new(|| rx(r"(\\v\s*\d+)\s*\.\s*(\S)"));
static MARKER_WITHOUT_V: LazyLock<Regex> = LazyLock::new(|| rx(r"\\(\d+)\s*\.?\s*(\S+)"));
static SPACE_BEFORE_NUMBER: LazyLock<Regex> = LazyLock::new(|| rx(r"(?<!\\c\s)(?<!\\v\s)(?<!\\v\s\d)(?<!\\q\d\s)(?<!\\li)(?<!\\li\d)(?<=\S)(?<!\\\S)(\d+)(?=\s)"));
static SPACE_AFTER_NUMBER: LazyLock<Regex> = LazyLock::new(|| rx(r"(\s+|^)(\d+)(\S+)"));
static SPACE_BEFORE_VERSE: LazyLock<Regex> = LazyLock::new(|| rx(r"(\S)(\\v\s+\d+)"));
static STANDALONE: LazyLock<Regex> = LazyLock::new(|| rx(r"(?<!\\)(?<!\\c\s)(?<!\\v\s)(?<!\\v\s\d)(?<!\\q\d\s)(?<!\\li)(?<!\\li\d)\b(\d+)\b(?=\s|$|[^\d\w])"));
static STANDALONE_PERIOD: LazyLock<Regex> = LazyLock::new(|| rx(r"(?<!\\)(?<!\\c\s)(?<!\\v\s)(?<!\\v\s\d)(?<!\\q\d\s)(?<!\\li)(?<!\\li\d)\b(\d+)\.\s*(.*?)(?=\s|$|[^\d\w])"));
static BACKSLASH_N: LazyLock<Regex> = LazyLock::new(|| rx(r"\\n"));
static VV: LazyLock<Regex> = LazyLock::new(|| rx(r"\\v\\v"));
static SV: LazyLock<Regex> = LazyLock::new(|| rx(r"\\s\\v"));
static SPACE_AFTER_SECTION: LazyLock<Regex> = LazyLock::new(|| rx(r"\\s\s+(\d+)"));
static QV: LazyLock<Regex> = LazyLock::new(|| rx(r"\\q\\v\s+(\d+)"));
static CC: LazyLock<Regex> = LazyLock::new(|| rx(r"(\\c \d+)\s+(\\c \d+)"));
static VERSE_IN_CONTEXT: LazyLock<Regex> = LazyLock::new(|| rx(r"\\v \d+"));
static WORD_NUMBER: LazyLock<Regex> = LazyLock::new(|| rx(r"\b\d+\b"));
static WORD_NUMBER_PERIOD: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(\d+)\."));

/// `ensure_no_chapter_labels`: DOC removes every `\cl` line before it splits the USFM into chapters unless the request asks for chapter labels.
pub fn remove_chapter_labels(usfm: &str) -> String {
    let mut out = String::with_capacity(usfm.len());
    let mut last = 0;
    for m in CHAPTER_LABEL.find_iter(usfm).filter_map(Result::ok) {
        out.push_str(&usfm[last..m.start()]);
        last = m.end();
    }
    out.push_str(&usfm[last..]);
    out
}

/// Whether DOC checks (and so repairs) this resource: `check_all_books_for_language` as in DOC's `.env` is true.
pub fn needs_fixing(lang_code: &str, resource_type: &str, book_code: &str, check_all_books_for_language: bool) -> bool {
    if check_all_books_for_language {
        RESOURCES_WITH_USFM_DEFECTS.iter().any(|(l, _, _)| *l == lang_code)
    } else {
        RESOURCES_WITH_USFM_DEFECTS.iter().any(|(l, t, b)| *l == lang_code && *t == resource_type && *b == book_code)
    }
}

/// `split_usfm_by_chapters` + `fix_usfm` on each chapter, put back together as DOC feeds the pieces on: frontmatter, then for each chapter its
/// marker, a newline and the repaired text. A text with no chapter marker is returned as it is.
pub fn fix_book(usfm: &str) -> String {
    let markers: Vec<(usize, usize)> = CHAPTER.find_iter(usfm).filter_map(Result::ok).map(|m| (m.start(), m.end())).collect();
    if markers.is_empty() {
        return usfm.to_string();
    }
    let mut out = String::with_capacity(usfm.len() + 64);
    out.push_str(&usfm[..markers[0].0]);
    for (i, (start, end)) in markers.iter().enumerate() {
        let next = markers.get(i + 1).map_or(usfm.len(), |m| m.0);
        let chapter = usfm[*end..next].trim_start();
        out.push_str(&usfm[*start..*end]);
        out.push('\n');
        if !chapter.is_empty() {
            out.push_str(&fix_usfm(chapter));
        }
        out.push('\n');
    }
    out
}

/// `fix_usfm`: every rule in order; a rule that detects its defect applies its fix.
pub fn fix_usfm(content: &str) -> String {
    let mut c = content.to_string();
    if found(&DOT_AFTER_VERSE, &c) {
        c = replace_groups(&DOT_AFTER_VERSE, &c, |g| format!("{} {}", g[1], g[2]));
    }
    if found(&MARKER_WITHOUT_V, &c) {
        c = replace_groups(&MARKER_WITHOUT_V, &c, |g| format!("\\v {} {}", g[1], g[2]));
    }
    if found(&SPACE_BEFORE_NUMBER, &c) {
        c = fix_missing_space_before_number(&c);
    }
    if found(&SPACE_AFTER_NUMBER, &c) {
        c = fix_missing_space_after_number(&c);
    }
    if found(&SPACE_BEFORE_VERSE, &c) {
        c = replace_groups(&SPACE_BEFORE_VERSE, &c, |g| format!("{} {}", g[1], g[2]));
    }
    if found(&STANDALONE, &c) {
        c = fix_standalone(&c, &STANDALONE, &WORD_NUMBER, false);
    }
    if found(&STANDALONE_PERIOD, &c) {
        c = fix_standalone(&c, &STANDALONE_PERIOD, &WORD_NUMBER_PERIOD, true);
    }
    if found(&BACKSLASH_N, &c) {
        c = c.replace("\\n", "\\v");
    }
    if found(&VV, &c) {
        c = c.replace("\\v\\v", "\\v");
    }
    if found(&SV, &c) {
        c = c.replace("\\s\\v", "\\s");
    }
    if found(&SPACE_AFTER_SECTION, &c) {
        c = replace_groups(&SPACE_AFTER_SECTION, &c, |g| format!("\\s{}", g[1]));
    }
    if found(&QV, &c) {
        c = replace_groups(&QV, &c, |g| format!("\\q{}", g[1]));
    }
    if found(&CC, &c) {
        c = replace_groups(&CC, &c, |g| format!("{}\n", g[1]));
    }
    c
}

fn found(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

/// `re.sub` with a template built from the groups (`g[0]` is the whole match).
fn replace_groups(re: &Regex, text: &str, build: impl Fn(&[String]) -> String) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut last = 0;
    for caps in re.captures_iter(text).filter_map(Result::ok) {
        let Some(whole) = caps.get(0) else { continue };
        out.push_str(&text[last..whole.start()]);
        out.push_str(&build(&groups(&caps)));
        last = whole.end();
    }
    out.push_str(&text[last..]);
    out
}

fn groups(caps: &Captures<'_>) -> Vec<String> {
    (0..caps.len()).map(|i| caps.get(i).map_or(String::new(), |m| m.as_str().to_string())).collect()
}

/// Python's `str.isdigit()` for one character: a decimal digit of any script, or a superscript digit.
fn py_isdigit(c: char) -> bool {
    c.is_ascii_digit() || (!c.is_ascii() && (is_decimal(c) || "\u{b2}\u{b3}\u{b9}\u{2070}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}".contains(c)))
}

fn is_decimal(c: char) -> bool {
    unicode_general_category::get_general_category(c) == unicode_general_category::GeneralCategory::DecimalNumber
}

/// The value of a decimal digit of any script (Unicode keeps them in runs of ten); `None` for anything else.
fn digit_value(c: char) -> Option<u32> {
    if let Some(d) = c.to_digit(10) {
        return Some(d);
    }
    if !is_decimal(c) {
        return None;
    }
    let mut zero = c as u32;
    while char::from_u32(zero - 1).is_some_and(is_decimal) {
        zero -= 1;
    }
    Some((c as u32 - zero) % 10)
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isdigit)
}

/// `fix_missing_space_before_number`: the first match decides; a digit (or a digit after a thousands separator) before the number means it was not missing a space.
fn fix_missing_space_before_number(content: &str) -> String {
    let Ok(Some(m)) = SPACE_BEFORE_NUMBER.captures(content) else { return content.to_string() };
    let Some(whole) = m.get(0) else { return content.to_string() };
    let before: Vec<char> = content[..whole.start()].chars().rev().take(2).collect();
    let first_group = m.get(1).map_or("", |g| g.as_str());
    let (cb, cbb) = (before.first().copied(), before.get(1).copied());
    if let Some(cb) = cb {
        if (py_isdigit(cb) && all_digits(first_group)) || ((cb == '.' || cb == ',') && cbb.is_some_and(py_isdigit) && all_digits(first_group)) {
            return content.to_string();
        }
    }
    replace_groups(&SPACE_BEFORE_NUMBER, content, |g| format!(" {}", g[1]))
}

/// `fix_missing_space_after_number`: for each (whitespace, number, text) found in the original, insert the space everywhere that digit-and-text occurs.
/// DOC builds the search pattern from the matched text without escaping it, so a regex metacharacter in it acts as one; a pattern that is not a valid
/// regular expression raises in Python (the job would fail); here that match is skipped.
fn fix_missing_space_after_number(content: &str) -> String {
    let mut c = content.to_string();
    let found: Vec<(String, String)> = SPACE_AFTER_NUMBER.captures_iter(content).filter_map(Result::ok).map(|m| (m.get(2).map_or(String::new(), |g| g.as_str().to_string()), m.get(3).map_or(String::new(), |g| g.as_str().to_string()))).collect();
    for (number, text) in found {
        let first = text.chars().next();
        if all_digits(&text) || first.is_some_and(|f| [':', '-', ')', '(', '[', ']', ',', '.'].contains(&f)) {
            continue;
        }
        let Ok(pattern) = Regex::new(&format!("{number}{text}")) else { continue };
        let replacement = format!("{number} {text}");
        let mut out = String::with_capacity(c.len() + 8);
        let mut last = 0;
        for m in pattern.find_iter(&c).filter_map(Result::ok) {
            out.push_str(&c[last..m.start()]);
            out.push_str(&replacement);
            last = m.end();
        }
        out.push_str(&c[last..]);
        c = out;
    }
    c
}

/// Python's `int()` of a run of digits (any script).
fn int_of(s: &str) -> u128 {
    s.chars().filter_map(digit_value).fold(0u128, |acc, d| acc.saturating_mul(10).saturating_add(u128::from(d)))
}

/// `fix_standalone_verse_numbers` and `fix_standalone_verse_number_and_period` (`with_text`): a bare number is a verse marker that lost its `\v` unless
/// a real `\v <n>` stands within 50 characters of it or there are three or more such numbers, unless the numbers ascend.
fn fix_standalone(content: &str, pattern: &Regex, counter: &Regex, with_text: bool) -> String {
    let Ok(Some(first)) = pattern.find(content) else { return content.to_string() };
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let char_index = |byte: usize| chars.partition_point(|(b, _)| *b < byte);
    let start = char_index(first.start()).saturating_sub(50);
    let end = (char_index(first.end()) + 50).min(chars.len());
    let context: String = chars[start..end].iter().map(|(_, c)| *c).collect();
    let tail_start = chars.get(7).map_or(content.len(), |(b, _)| *b);
    let tail = &content[tail_start..];
    let numbers: Vec<u128> = if with_text {
        counter.captures_iter(tail).filter_map(Result::ok).map(|m| int_of(m.get(1).map_or("", |g| g.as_str()))).collect()
    } else {
        counter.find_iter(tail).filter_map(Result::ok).map(|m| int_of(m.as_str())).collect()
    };
    let ascending = numbers.windows(2).all(|w| w[0] < w[1]);
    let verse_near = found(&VERSE_IN_CONTEXT, &context);
    if (!verse_near && numbers.len() < 3) || ascending {
        return if with_text {
            replace_groups(pattern, content, |g| format!("\\v {} {}", g[1], g[2]))
        } else {
            replace_groups(pattern, content, |g| format!("\\v {}", g[1]))
        };
    }
    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_and_missing_marker() {
        assert_eq!(fix_usfm("\\v 1. In the beginning\n"), "\\v 1 In the beginning\n");
        assert_eq!(fix_usfm("\\1 Hello world"), "\\v 1 Hello world");
    }

    #[test]
    fn standalone_numbers_become_verses_when_they_ascend() {
        let fixed = fix_usfm("\\p\n1 first words 2 second words 3 third words");
        assert!(fixed.contains("\\v 2") && fixed.contains("\\v 3"), "{fixed}");
    }

    #[test]
    fn double_chapter_markers_collapse_and_chapters_are_fixed_separately() {
        let out = fix_book("\\id JUD\n\\c 1\n\\v 1. First\n\\c 2\n\\v 1. Second\n");
        assert!(out.contains("\\v 1 First") && out.contains("\\v 1 Second"), "{out}");
        assert_eq!(fix_usfm("\\c 1 \\c 2"), "\\c 1\n");
    }

    #[test]
    fn chapter_labels_come_out_before_the_repairs_look_at_numbers() {
        assert_eq!(remove_chapter_labels("\\c 1\n\\cl PASAL 1\n1 Ari\n"), "\\c 1\n\n1 Ari\n");
    }

    #[test]
    fn language_list() {
        assert!(needs_fixing("gwg", "reg", "mat", true));
        assert!(!needs_fixing("en", "ulb", "mat", true));
        assert!(needs_fixing("gwg", "reg", "php", false));
        assert!(!needs_fixing("gwg", "reg", "mat", false));
    }
}
