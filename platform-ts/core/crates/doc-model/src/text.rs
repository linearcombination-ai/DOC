//! Small text helpers with Python `re` semantics where DOC's output depends on them.

use unicode_general_category::{get_general_category, GeneralCategory as G};

/// Python 3 `re` `\w` for `str` patterns: `str.isalnum()` or `_`. `isalnum` is
/// letters (Lu Ll Lt Lm Lo) and numbers (Nd Nl No); combining marks (Mn, Mc) are NOT
/// word characters, which is why DOC's `\b` fails inside Indic and Khmer words.
pub fn is_py_word(c: char) -> bool {
    if c == '_' {
        return true;
    }
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::DecimalNumber
            | G::LetterNumber
            | G::OtherNumber
    )
}

/// Single-character lowercase as `re.IGNORECASE` compares (simple mapping).
fn fold(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// `re.search(r"\b" + re.escape(needle) + r"\b", haystack, re.IGNORECASE) is not None`.
/// Empty needle: `\b\b` matches iff the haystack contains a word character.
pub fn word_search_ci(haystack: &[char], needle: &[char]) -> bool {
    let boundary = |pos: usize| -> bool {
        let before = pos > 0 && is_py_word(haystack[pos - 1]);
        let after = pos < haystack.len() && is_py_word(haystack[pos]);
        before != after
    };
    if needle.is_empty() {
        return haystack.iter().any(|c| is_py_word(*c));
    }
    if needle.len() > haystack.len() {
        return false;
    }
    let first = fold(needle[0]);
    for start in 0..=(haystack.len() - needle.len()) {
        if fold(haystack[start]) != first {
            continue;
        }
        if !boundary(start) {
            continue;
        }
        let end = start + needle.len();
        if !boundary(end) {
            continue;
        }
        if (1..needle.len()).all(|k| fold(haystack[start + k]) == fold(needle[k])) {
            return true;
        }
    }
    false
}

/// `demote_headings_by_one` (`DOC:backend/doc/utils/text_utils.py:147`): regex
/// `</?h([1-6])\b` (IGNORECASE), each match's first lowercase `h<level>` is replaced by
/// `h<min(level+1, 6)>`. An upper-case `<H2` matches the regex but the lowercase
/// `replace` finds nothing, so it is left unchanged (as built).
pub fn demote_headings_by_one(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::with_capacity(content.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '/' {
                j += 1;
            }
            if j + 1 < chars.len() && (chars[j] == 'h' || chars[j] == 'H') {
                if let Some(level) = chars[j + 1].to_digit(10).filter(|d| (1..=6).contains(d)) {
                    let after = j + 2;
                    let boundary_ok = after >= chars.len() || !is_py_word(chars[after]);
                    if boundary_ok {
                        // tag text is chars[i..after]
                        out.extend(chars[i..j].iter());
                        if chars[j] == 'h' {
                            let new_level = (level + 1).min(6);
                            out.push('h');
                            out.push(char::from_digit(new_level, 10).unwrap_or('6'));
                        } else {
                            out.extend(chars[j..after].iter());
                        }
                        i = after;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub const HR: &str = "<hr/>";

pub fn h1(name: &str) -> String {
    format!("<h1 class='book-name'>{name}</h1>")
}
pub fn h2_book(name: &str) -> String {
    format!("<h2 class='book-name'>{name}</h2>")
}
pub fn verse_span(verse: &str) -> String {
    format!("<span class=\"verse\">{verse}</span>")
}

/// All non-overlapping matches of `re.compile(r"\b" + re.escape(needle) + r"\b", re.I)`
/// in `hay`, left to right, as char index ranges.
pub fn word_find_all_ci(hay: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    let boundary = |pos: usize| -> bool {
        let before = pos > 0 && is_py_word(hay[pos - 1]);
        let after = pos < hay.len() && is_py_word(hay[pos]);
        before != after
    };
    let mut out = Vec::new();
    if needle.is_empty() {
        // `\b\b` matches the empty string at every word boundary
        for pos in 0..=hay.len() {
            if boundary(pos) {
                out.push((pos, pos));
            }
        }
        return out;
    }
    let first = fold(needle[0]);
    let mut start = 0;
    while start + needle.len() <= hay.len() {
        let end = start + needle.len();
        if fold(hay[start]) == first
            && boundary(start)
            && boundary(end)
            && (1..needle.len()).all(|k| fold(hay[start + k]) == fold(needle[k]))
        {
            out.push((start, end));
            start = end;
        } else {
            start += 1;
        }
    }
    out
}

/// `re.compile(rf"\b{re.escape(needle)}\b", re.IGNORECASE).sub(replacement, text)` for a
/// replacement without backslashes.
pub fn word_sub_ci(text: &str, needle: &str, replacement: &str) -> String {
    let hay: Vec<char> = text.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    let matches = word_find_all_ci(&hay, &n);
    if matches.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (s, e) in matches {
        out.extend(hay[last..s].iter());
        out.push_str(replacement);
        last = e;
    }
    out.extend(hay[last..].iter());
    out
}

// ---------------------------------------------------------------------------------
// P3: the word rule of the Translation Words matcher
// ---------------------------------------------------------------------------------

/// Where a Translation Words match may start and end. DOC's rule is Python's `\b` (a change
/// between `\w` and `\W`), which splits Khmer, Sinhala and Indic words at every combining
/// mark and apostrophe. Three rules are implemented and measured (report section 3):
/// [`PythonBoundary`] (as built, here), UAX #29 word segmentation (`doc-interleave`,
/// `unicode-segmentation`) and kitchen's `words/1` (`doc-canon`, `mise::words`).
///
/// Offsets are indexes into a slice of `char`s (scalars). Kitchen counts UTF-8 bytes inside
/// its engines and UTF-16 code units only at its wasm wall (`mise::utf16`); an adapter that
/// receives kitchen offsets converts them, it never sees them here.
pub trait WordRule {
    fn name(&self) -> &'static str;
    /// Single-character case folding used to compare needle and haystack.
    fn fold(&self, c: char) -> char {
        fold(c)
    }
    /// `len + 1` flags: `true` where a match may start or end.
    fn boundaries(&self, hay: &[char]) -> Vec<bool>;
}

/// Python `re`'s `\b` over `\w` (as built).
pub struct PythonBoundary;

impl WordRule for PythonBoundary {
    fn name(&self) -> &'static str {
        "python_b"
    }
    fn boundaries(&self, hay: &[char]) -> Vec<bool> {
        (0..=hay.len())
            .map(|pos| {
                let before = pos > 0 && is_py_word(hay[pos - 1]);
                let after = pos < hay.len() && is_py_word(hay[pos]);
                before != after
            })
            .collect()
    }
}

/// A haystack with its boundaries computed once, searched for many needles.
pub struct Prepared {
    folded: Vec<char>,
    ok: Vec<bool>,
    has_word: bool,
    /// folded first character to the positions holding it (so a needle looks only there)
    at: std::collections::HashMap<char, Vec<usize>>,
}

pub fn prepare(rule: &dyn WordRule, hay: &[char]) -> Prepared {
    let folded: Vec<char> = hay.iter().map(|c| rule.fold(*c)).collect();
    let mut at: std::collections::HashMap<char, Vec<usize>> = std::collections::HashMap::new();
    for (i, c) in folded.iter().enumerate() {
        at.entry(*c).or_default().push(i);
    }
    Prepared { folded, ok: rule.boundaries(hay), has_word: hay.iter().any(|c| is_py_word(*c)), at }
}

impl Prepared {
    /// Start positions (char indexes) of every whole-word, case-insensitive occurrence of
    /// `needle` (folded by the same rule), left to right, overlaps included.
    pub fn positions(&self, rule: &dyn WordRule, needle: &[char]) -> Vec<usize> {
        if needle.is_empty() {
            return Vec::new();
        }
        let n = self.folded.len();
        if needle.len() > n {
            return Vec::new();
        }
        let folded: Vec<char> = needle.iter().map(|c| rule.fold(*c)).collect();
        let Some(cands) = self.at.get(&folded[0]) else { return Vec::new() };
        cands
            .iter()
            .copied()
            .filter(|&start| {
                start + needle.len() <= n
                    && self.ok[start]
                    && self.ok[start + needle.len()]
                    && (1..needle.len()).all(|k| self.folded[start + k] == folded[k])
            })
            .collect()
    }

    /// Whole-word, case-insensitive occurrence of `needle`. An empty needle matches iff the
    /// haystack holds a word character (as `\b\b` does).
    pub fn contains(&self, rule: &dyn WordRule, needle: &[char]) -> bool {
        if needle.is_empty() {
            return self.has_word;
        }
        !self.positions(rule, needle).is_empty()
    }
}
