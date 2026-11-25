//! Python `str` semantics that DOC's name rules depend on, so the port is exact rather than
//! "close": `str.isspace`/`strip`/`\s`, `str.capitalize`, `str.upper`/`lower`, and the
//! IGNORECASE behaviour of the literal `i` in `re.match(r"^(1|2|3|i{1,3})", ..., IGNORECASE)`.

/// `str.isspace()` for one character (also the class of `\s` in a Python `str` pattern).
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// `str.strip()`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `re.sub(r"\s+", " ", text).strip()`.
pub fn normalize_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if is_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            in_space = false;
            out.push(c);
        }
    }
    strip(&out).to_string()
}

/// Python's titlecase mapping where it differs from the uppercase mapping (the cases that can
/// start a name): the Latin digraphs, `ß` and the Latin ligatures. Everything else is uppercase.
fn title_first(c: char, out: &mut String) {
    match c {
        '\u{1c4}' | '\u{1c5}' | '\u{1c6}' => out.push('\u{1c5}'),
        '\u{1c7}' | '\u{1c8}' | '\u{1c9}' => out.push('\u{1c8}'),
        '\u{1ca}' | '\u{1cb}' | '\u{1cc}' => out.push('\u{1cb}'),
        '\u{1f1}' | '\u{1f2}' | '\u{1f3}' => out.push('\u{1f2}'),
        'ß' => out.push_str("Ss"),
        '\u{fb00}' => out.push_str("Ff"),
        '\u{fb01}' => out.push_str("Fi"),
        '\u{fb02}' => out.push_str("Fl"),
        '\u{fb03}' => out.push_str("Ffi"),
        '\u{fb04}' => out.push_str("Ffl"),
        '\u{fb05}' | '\u{fb06}' => out.push_str("St"),
        _ => out.extend(c.to_uppercase()),
    }
}

/// `str.capitalize()`: first character titlecased, the rest lower-cased.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return String::new() };
    let mut out = String::with_capacity(s.len());
    title_first(first, &mut out);
    out.push_str(&chars.as_str().to_lowercase());
    out
}

/// `str.lower()` then `capitalize()`, as DOC writes it.
pub fn lower_capitalize(s: &str) -> String {
    capitalize(&s.to_lowercase())
}

/// `str.upper()`.
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// Does `c` match the literal `i` under `re.IGNORECASE` with a Unicode `str` pattern?
/// (`I`, `i`, and the two Turkish i's that sre treats as equivalent.)
pub fn matches_i_ignorecase(c: char) -> bool {
    matches!(c, 'i' | 'I' | '\u{130}' | '\u{131}')
}

/// `str.isupper()` for a single character.
pub fn is_upper_char(c: char) -> bool {
    c.is_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_matches_python() {
        assert_eq!(capitalize("john"), "John");
        assert_eq!(capitalize("ÉPHÉSIENS"), "Éphésiens");
        assert_eq!(capitalize("ßa"), "Ssa");
        assert_eq!(capitalize(""), "");
        assert_eq!(lower_capitalize("WIMBO WA SULEMANI"), "Wimbo wa sulemani");
    }

    #[test]
    fn whitespace_matches_python() {
        assert!(is_space('\u{1f}'));
        assert!(!is_space('\u{200b}'));
        assert_eq!(normalize_spaces("  a \t\u{a0} b\n"), "a b");
    }
}
