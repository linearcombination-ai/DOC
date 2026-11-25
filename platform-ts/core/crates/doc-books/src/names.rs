//! DOC's book-name rules, ported one for one (DOC `text_utils.py`, `parsing.py`, `url_utils.py`):
//! the USFM front-matter marker search, the marker order, `normalize_localized_book_name`, the
//! correction table and the XML-incompatible-character filter.

use crate::pystr;
use crate::tables_gen::BOOK_NAME_CORRECTIONS;

/// `DEFAULT_BOOK_NAME_LOOKUP_ORDER`.
const DEFAULT_ORDER: &[&str] = &["h", "mt", "toc1", "toc2"];

/// `XML_INCOMPATIBLE_CHARS_RE` without the surrogate range (a Rust `String` cannot hold one;
/// the shell reports a source that does not decode as UTF-8 as a read failure).
pub fn remove_xml_incompatible_chars(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'))
        .collect()
}

fn is_decimal_digit(c: char) -> bool {
    matches!(
        unicode_general_category::get_general_category(c),
        unicode_general_category::GeneralCategory::DecimalNumber
    )
}

/// `split_usfm_by_chapters(...)[0]`: the text before the first `\c <digits>`, stripped.
pub fn frontmatter(usfm: &str) -> &str {
    let bytes = usfm.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = usfm[from..].find("\\c") {
        let at = from + rel;
        let after = &usfm[at + 2..];
        let ws: usize = after.chars().take_while(|c| pystr::is_space(*c)).map(char::len_utf8).sum();
        if ws > 0 && after[ws..].chars().next().is_some_and(is_decimal_digit) {
            return pystr::strip(&usfm[..at]);
        }
        from = at + 2;
        if from >= bytes.len() {
            break;
        }
    }
    pystr::strip(usfm)
}

/// The lookahead `(?=\s+\\|\n|$)` at byte offset `q` of `s`.
fn value_ends_at(s: &str, q: usize) -> bool {
    let rest = &s[q..];
    let Some(first) = rest.chars().next() else { return true };
    if first == '\n' {
        return true;
    }
    let ws: usize = rest.chars().take_while(|c| pystr::is_space(*c)).map(char::len_utf8).sum();
    ws > 0 && rest[ws..].starts_with('\\')
}

/// One entry of `extract_usfm_frontmatter`: `\<marker>[ \t]+(.*?)(?=\s+\\|\n|$)` (MULTILINE,
/// first match), cleaned and stripped; `None` when absent or empty after cleaning.
fn marker_value(front: &str, marker: &str) -> Option<String> {
    let pat = format!("\\{marker}");
    for (at, _) in front.match_indices(&pat) {
        let after = at + pat.len();
        let blanks: usize = front[after..].chars().take_while(|c| matches!(c, ' ' | '\t')).count();
        if blanks == 0 {
            continue; // `\mt1`, `\toc10`... do not match
        }
        let start = after + blanks; // spaces and tabs are one byte each
        let mut q = start;
        while !value_ends_at(front, q) {
            q += front[q..].chars().next().map_or(1, char::len_utf8);
        }
        let raw = &front[start..q];
        let value = pystr::strip(&remove_xml_incompatible_chars(raw)).to_string();
        return if value.is_empty() { None } else { Some(value) };
    }
    None
}

/// `maybe_localized_book_name(frontmatter, language, resource_type)`.
pub fn maybe_localized_book_name(front: &str, language: &str, resource_type: &str) -> String {
    let key = (language.to_lowercase(), resource_type.to_lowercase());
    let skip_normalization = key.0 == "fr" && key.1 == "f10";
    let order: &[&str] = if skip_normalization { &["toc2"] } else { DEFAULT_ORDER };
    let mut name = String::new();
    for marker in order {
        if let Some(v) = marker_value(front, marker) {
            name = v;
            break;
        }
    }
    if !name.is_empty() && !skip_normalization {
        name = normalize_localized_book_name(&name);
    }
    name
}

/// `normalize_localized_book_name` (lossy by design: it is the as-built behaviour).
pub fn normalize_localized_book_name(localized: &str) -> String {
    let name = pystr::strip(localized);
    // re.match(r"^(1|2|3|i{1,3})", name, IGNORECASE)
    let chars: Vec<char> = name.chars().collect();
    let numeral_len = match chars.first() {
        Some('1' | '2' | '3') => 1,
        Some(c) if pystr::matches_i_ignorecase(*c) => chars.iter().take(3).take_while(|c| pystr::matches_i_ignorecase(**c)).count(),
        _ => 0,
    };
    if numeral_len > 0 {
        let numeral_raw: String = chars[..numeral_len].iter().collect();
        let numeral_upper = pystr::upper(&numeral_raw);
        let next_char = chars.get(numeral_len).copied();
        let treat_as_word = numeral_upper == "I"
            && !(next_char.is_some_and(pystr::is_space) || next_char.is_some_and(pystr::is_upper_char));
        if !treat_as_word {
            let number = match numeral_upper.as_str() {
                "I" => "1".to_string(),
                "II" => "2".to_string(),
                "III" => "3".to_string(),
                other => other.to_string(),
            };
            let rest_raw: String = chars[numeral_len..].iter().collect();
            let rest = pystr::strip(&rest_raw);
            let joined = if rest.is_empty() { number } else { format!("{number} {}", pystr::lower_capitalize(rest)) };
            return pystr::normalize_spaces(&joined);
        }
    }
    pystr::normalize_spaces(&pystr::lower_capitalize(name))
}

/// `maybe_correct_book_name(lang, name)`.
pub fn maybe_correct_book_name(lang: &str, name: &str) -> String {
    for ((l, n), fixed) in BOOK_NAME_CORRECTIONS {
        if *l == lang && *n == name {
            return if fixed.is_empty() { name.to_string() } else { (*fixed).to_string() };
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctests_from_doc() {
        assert_eq!(normalize_localized_book_name("1Peter"), "1 Peter");
        assert_eq!(normalize_localized_book_name("I peter"), "1 Peter");
        assert_eq!(normalize_localized_book_name("III John"), "3 John");
        assert_eq!(normalize_localized_book_name("II john"), "2 John");
        assert_eq!(normalize_localized_book_name("IIjohn"), "2 John");
        assert_eq!(normalize_localized_book_name("john"), "John");
        assert_eq!(normalize_localized_book_name("Isaías"), "Isaías");
        // from the book-name scout's RAN table
        assert_eq!(normalize_localized_book_name("Wimbo Wa Sulemani"), "Wimbo wa sulemani");
        assert_eq!(normalize_localized_book_name("IROTE"), "1 Rote");
        assert_eq!(normalize_localized_book_name("1ኛ ቆሮንቶስ"), "1 ኛ ቆሮንቶስ");
        assert_eq!(normalize_localized_book_name("Iyobu"), "Iyobu");
    }

    #[test]
    fn markers() {
        let front = "\\id MAT\n\\h  Mateo\n\\toc1 Evangelio\n\\mt1 Title\n\\mt Largo \\x\n";
        assert_eq!(maybe_localized_book_name(front, "es", "ulb"), "Mateo");
        assert_eq!(maybe_localized_book_name("\\h \n\\c 1", "es", "ulb"), "");
        assert_eq!(maybe_localized_book_name("\\toc2 Genèse", "fr", "f10"), "Genèse");
        assert_eq!(frontmatter("\\id GEN\n\\h Gen\n\\c 1\n\\v 1 x"), "\\id GEN\n\\h Gen");
        assert_eq!(frontmatter("\\id GEN \\cl x\n"), "\\id GEN \\cl x");
    }
}
