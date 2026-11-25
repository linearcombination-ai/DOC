//! The Python string semantics DOC's name code depends on (`strip`, `lower().capitalize()`,
//! `isspace`, the XML-character filter) and DOC's USFM front-matter extraction.

use fancy_regex::Regex;
use std::sync::OnceLock;

/// `str.isspace()` for one character (differs from Rust's by U+001C..U+001F).
pub fn is_py_space(c: char) -> bool {
    matches!(c, '\u{1c}'..='\u{1f}') || c.is_whitespace()
}

/// `str.strip()`.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

/// `re.sub(r"\s+", " ", text).strip()` (`normalize_spaces`).
pub fn normalize_spaces(text: &str) -> String {
    text.split(is_py_space).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// `remove_xml_incompatible_chars`: drop C0 controls other than tab, LF, CR, and U+FFFE/U+FFFF
/// (lone surrogates cannot occur in a Rust string).
pub fn remove_xml_incompatible_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            !matches!(c as u32, 0..=8 | 0xb | 0xc | 0xe..=0x1f | 0xfffe | 0xffff)
        })
        .collect()
}

/// The titlecase mapping of one character as Python's `str.capitalize()` applies to the first
/// character. Differs from uppercase only for the digraph and ligature characters and the Georgian
/// letters, listed here.
fn title_first(c: char) -> String {
    match c as u32 {
        0x01c4..=0x01c6 => "\u{01c5}".into(),
        0x01c7..=0x01c9 => "\u{01c8}".into(),
        0x01ca..=0x01cc => "\u{01cb}".into(),
        0x01f1..=0x01f3 => "\u{01f2}".into(),
        0x00df => "Ss".into(),
        0xfb00 => "Ff".into(),
        0xfb01 => "Fi".into(),
        0xfb02 => "Fl".into(),
        0xfb03 => "Ffi".into(),
        0xfb04 => "Ffl".into(),
        0xfb05 | 0xfb06 => "St".into(),
        0x10d0..=0x10fa | 0x10fd..=0x10ff => c.to_string(),
        _ => c.to_uppercase().collect(),
    }
}

/// `s.lower().capitalize()`: titlecase of the first character, the rest lower-cased with
/// Python's final-sigma rule (Rust's `to_lowercase` implements the same Unicode rule, computed
/// over the whole string so the context matches).
pub fn lower_capitalize(s: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    // Python's capitalize() lower-cases the rest of the ORIGINAL string, then title-cases the
    // first character, so lower() first is the same except for the first character itself.
    let lower_all = s.to_lowercase();
    let skip: usize = first.to_lowercase().map(char::len_utf8).sum();
    let rest = lower_all.get(skip..).unwrap_or("");
    format!("{}{}", title_first(first), rest)
}

fn is_i_like(c: char) -> bool {
    // re.IGNORECASE on `i`: i, I, and the equivalents sre knows (U+0131 dotless i, U+0130 dotted I)
    matches!(c, 'i' | 'I' | '\u{0130}' | '\u{0131}')
}

/// `normalize_localized_book_name` (`doc.utils.text_utils`), reproduced with its quirks:
/// the roman-numeral guess (`Ii...` is taken as the numeral 2), the inserted space after a
/// leading digit, and the lower-casing of everything after the first character.
pub fn normalize_localized_book_name(localized_book_name: &str) -> String {
    let name = py_strip(localized_book_name);
    let chars: Vec<char> = name.chars().collect();
    // ^(1|2|3|i{1,3}) with IGNORECASE
    let numeral_len = match chars.first() {
        Some('1' | '2' | '3') => 1,
        Some(&c) if is_i_like(c) => chars.iter().take(3).take_while(|&&x| is_i_like(x)).count(),
        _ => 0,
    };
    if numeral_len > 0 {
        let raw: String = chars[..numeral_len].iter().collect();
        let upper = raw.to_uppercase();
        let next_char = chars.get(numeral_len).copied();
        let single_i_not_numeral = upper == "I"
            && !matches!(next_char, Some(n) if is_py_space(n) || n.is_uppercase());
        if !single_i_not_numeral {
            let number = match upper.as_str() {
                "I" => "1".to_string(),
                "II" => "2".to_string(),
                "III" => "3".to_string(),
                other => other.to_string(),
            };
            let rest_raw: String = chars[numeral_len..].iter().collect();
            let rest = py_strip(&rest_raw);
            let joined = if rest.is_empty() {
                number
            } else {
                format!("{} {}", number, lower_capitalize(rest))
            };
            return normalize_spaces(&joined);
        }
    }
    normalize_spaces(&lower_capitalize(name))
}

/// `maybe_correct_book_name(lang, name)`.
pub fn maybe_correct_book_name(lang: &str, name: &str) -> String {
    crate::tables_gen::BOOK_NAME_CORRECTION_TABLE
        .iter()
        .find(|((l, n), fixed)| *l == lang && *n == name && !fixed.is_empty())
        .map(|(_, fixed)| (*fixed).to_string())
        .unwrap_or_else(|| name.to_string())
}

fn marker_regex(marker: &str) -> Option<&'static Regex> {
    static H: OnceLock<Option<Regex>> = OnceLock::new();
    static MT: OnceLock<Option<Regex>> = OnceLock::new();
    static TOC1: OnceLock<Option<Regex>> = OnceLock::new();
    static TOC2: OnceLock<Option<Regex>> = OnceLock::new();
    let (cell, pattern) = match marker {
        "h" => (&H, r"(?m)\\h[ \t]+(.*?)(?=\s+\\|\n|$)"),
        "mt" => (&MT, r"(?m)\\mt[ \t]+(.*?)(?=\s+\\|\n|$)"),
        "toc1" => (&TOC1, r"(?m)\\toc1[ \t]+(.*?)(?=\s+\\|\n|$)"),
        _ => (&TOC2, r"(?m)\\toc2[ \t]+(.*?)(?=\s+\\|\n|$)"),
    };
    cell.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

/// `extract_usfm_frontmatter`: the value of the first `\h`, `\mt`, `\toc1`, `\toc2` marker in the
/// front matter, XML-incompatible characters removed, empty values omitted.
pub fn extract_usfm_frontmatter(frontmatter: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for key in ["h", "mt", "toc1", "toc2"] {
        let Some(re) = marker_regex(key) else { continue };
        if let Ok(Some(caps)) = re.captures(frontmatter) {
            if let Some(m) = caps.get(1) {
                let value = py_strip(&remove_xml_incompatible_chars(m.as_str())).to_string();
                if !value.is_empty() {
                    out.push((key, value));
                }
            }
        }
    }
    out
}

/// `maybe_localized_book_name(frontmatter, lang, resource_type)`.
pub fn maybe_localized_book_name(frontmatter: &str, language: &str, resource_type: &str) -> String {
    book_name_from_frontmatter(frontmatter, language, resource_type, true)
}

/// The same marker rule, optionally without DOC's normalisation (`Fixed` keeps the name as written).
pub fn book_name_from_frontmatter(frontmatter: &str, language: &str, resource_type: &str, normalize: bool) -> String {
    let data = extract_usfm_frontmatter(frontmatter);
    let lang = language.to_lowercase();
    let res = resource_type.to_lowercase();
    let special = lang == "fr" && res == "f10";
    let order: &[&str] = if special { &["toc2"] } else { &["h", "mt", "toc1", "toc2"] };
    let mut name = String::new();
    for marker in order {
        if let Some((_, value)) = data.iter().find(|(k, _)| k == marker) {
            name = value.clone();
            break;
        }
    }
    if normalize && !name.is_empty() && !special {
        name = normalize_localized_book_name(&name);
    }
    name
}

/// The front matter of a USFM file: the text before the first `\c <digits>` marker, stripped
/// (`split_usfm_by_chapters`'s first element). Fixed mode reads the marker `\h`/`\toc` values
/// from the same slice.
pub fn usfm_frontmatter(usfm: &str) -> String {
    static CHAPTER: OnceLock<Option<Regex>> = OnceLock::new();
    let re = CHAPTER.get_or_init(|| Regex::new(r"\\c\s+\d+").ok());
    let end = re
        .as_ref()
        .and_then(|r| r.find(usfm).ok().flatten())
        .map(|m| m.start())
        .unwrap_or(usfm.len());
    py_strip(&usfm[..end]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_doc_doctests() {
        for (input, want) in [
            ("1Peter", "1 Peter"), ("I peter", "1 Peter"), ("III John", "3 John"), ("II john", "2 John"),
            ("IIjohn", "2 John"), ("john", "John"), ("John", "John"), ("Isaías", "Isaías"),
            ("Wimbo Wa Sulemani", "Wimbo wa sulemani"), ("IROTE", "1 Rote"), ("IBRANI", "1 Brani"),
            ("1ኛ ቆሮንቶስ", "1 ኛ ቆሮንቶስ"), ("EMONISELI", "Emoniseli"), ("", ""),
        ] {
            assert_eq!(normalize_localized_book_name(input), want, "input {input:?}");
        }
    }

    #[test]
    fn final_sigma_and_titlecase() {
        assert_eq!(lower_capitalize("ΟΔΥΣΣΕΥΣ"), "Οδυσσευς");
        assert_eq!(lower_capitalize("ǆungla"), "ǅungla");
    }

    #[test]
    fn frontmatter_rules() {
        let fm = "\\id MAT\n\\h Mateo\n\\toc1 El Evangelio\n\\toc2 Mateo\n\\mt1 X";
        let got = extract_usfm_frontmatter(fm);
        assert_eq!(got, vec![("h", "Mateo".to_string()), ("toc1", "El Evangelio".to_string()), ("toc2", "Mateo".to_string())]);
        assert_eq!(maybe_localized_book_name(fm, "es-419", "ulb"), "Mateo");
        assert_eq!(maybe_localized_book_name("\\toc2 LUKA", "fr", "f10"), "LUKA");
        assert_eq!(usfm_frontmatter("\\id X\n\\h Y\n\\c 1\n\\v 1 a"), "\\id X\n\\h Y");
    }

    #[test]
    fn empty_marker_does_not_capture_next_line() {
        let got = extract_usfm_frontmatter("\\h \n\\c 1");
        assert!(got.is_empty());
    }
}
