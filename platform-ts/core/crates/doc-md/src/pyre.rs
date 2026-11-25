//! Python-`re` compatible regular expressions on top of `fancy-regex`.
//!
//! DOC's link transformers are written against Python's `re`. The differences that
//! matter, and how they are handled:
//!
//! * `\w` in Python is `str.isalnum()` or `_` (letters and numbers; combining marks are
//!   NOT word characters). Rust's `\w` includes marks, so `\w` is rewritten to
//!   `[\p{L}\p{N}_]`.
//! * `\s` in Python also matches U+001C..U+001F; rewritten to `[\s\x1c-\x1f]`.
//! * `$` (no MULTILINE) matches at the end and before a final `\n`; rewritten to the
//!   look-ahead `(?=\n?\z)`.
//! * A `{` that does not start a valid quantifier is a literal in Python and an error
//!   in Rust; it is escaped.
//! * Lookarounds (`(?!obs)` in the TN scripture link) need a backtracking engine, hence
//!   `fancy-regex`; patterns without them run on the linear-time `regex` engine inside.

use fancy_regex::Regex;

/// Rewrite a Python pattern into `fancy-regex` syntax (see module docs).
pub fn translate(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut i = 0;
    let mut in_class = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            match (n, in_class) {
                ('w', false) => out.push_str(r"[\p{L}\p{N}_]"),
                ('w', true) => out.push_str(r"\p{L}\p{N}_"),
                ('W', false) => out.push_str(r"[^\p{L}\p{N}_]"),
                ('s', false) => out.push_str(r"[\s\x1c-\x1f]"),
                ('s', true) => out.push_str(r"\s\x1c-\x1f"),
                _ => {
                    out.push('\\');
                    out.push(n);
                }
            }
            i += 2;
            continue;
        }
        if in_class {
            if c == ']' {
                in_class = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        match c {
            '[' => {
                in_class = true;
                out.push('[');
                i += 1;
                // a `^` right after `[` and a `]` right after that are class syntax
                if i < chars.len() && chars[i] == '^' {
                    out.push('^');
                    i += 1;
                }
                if i < chars.len() && chars[i] == ']' {
                    out.push_str("\\]");
                    i += 1;
                }
            }
            '$' => {
                out.push_str(r"(?=\n?\z)");
                i += 1;
            }
            '{' => {
                if is_quantifier(&chars, i) {
                    out.push('{');
                } else {
                    out.push_str(r"\{");
                }
                i += 1;
            }
            '}' => {
                // a closing brace is only syntax inside a quantifier we already passed
                if out_is_in_quantifier(&out) {
                    out.push('}');
                } else {
                    out.push_str(r"\}");
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn is_quantifier(chars: &[char], at: usize) -> bool {
    // {n} {n,} {n,m} {,m}
    let mut j = at + 1;
    let mut digits_a = 0;
    while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
        digits_a += 1;
    }
    let mut digits_b = 0;
    let mut comma = false;
    if j < chars.len() && chars[j] == ',' {
        comma = true;
        j += 1;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
            digits_b += 1;
        }
    }
    j < chars.len() && chars[j] == '}' && (digits_a > 0 || (comma && digits_b > 0))
}

fn out_is_in_quantifier(out: &str) -> bool {
    // true when the last unescaped `{` in `out` has only digits/comma after it
    match out.rfind('{') {
        Some(p) if !out[..p].ends_with('\\') => out[p + 1..].chars().all(|c| c.is_ascii_digit() || c == ','),
        _ => false,
    }
}

#[derive(Debug)]
pub struct PyRegex {
    re: Regex,
    names: Vec<(String, usize)>,
}

/// One match with owned group texts (group 0 is the whole match).
#[derive(Clone, Debug)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    groups: Vec<Option<String>>,
    names: Vec<(String, usize)>,
}

impl Match {
    pub fn whole(&self) -> &str {
        self.groups[0].as_deref().unwrap_or("")
    }
    pub fn group(&self, i: usize) -> Option<&str> {
        self.groups.get(i).and_then(|g| g.as_deref())
    }
    /// Named group text; `""` when the group did not participate (Python would give `None`).
    pub fn name(&self, n: &str) -> &str {
        self.names
            .iter()
            .find(|(k, _)| k == n)
            .and_then(|(_, i)| self.group(*i))
            .unwrap_or("")
    }
}

impl PyRegex {
    pub fn new(pattern: &str, ignore_case: bool) -> Result<PyRegex, String> {
        let translated = translate(pattern);
        let full = if ignore_case { format!("(?i){translated}") } else { translated };
        let re = Regex::new(&full).map_err(|e| format!("re.error: {e}"))?;
        let names = re
            .capture_names()
            .enumerate()
            .filter_map(|(i, n)| n.map(|n| (n.to_string(), i)))
            .collect();
        Ok(PyRegex { re, names })
    }

    fn to_match(&self, caps: &fancy_regex::Captures<'_>) -> Match {
        let whole = caps.get(0);
        Match {
            start: whole.map_or(0, |m| m.start()),
            end: whole.map_or(0, |m| m.end()),
            groups: (0..caps.len()).map(|i| caps.get(i).map(|m| m.as_str().to_string())).collect(),
            names: self.names.clone(),
        }
    }

    /// `pattern.finditer(text)` collected (Python iterates the original string).
    pub fn finditer(&self, text: &str) -> Result<Vec<Match>, String> {
        let mut out = Vec::new();
        for caps in self.re.captures_iter(text) {
            let caps = caps.map_err(|e| format!("re.error: {e}"))?;
            out.push(self.to_match(&caps));
        }
        Ok(out)
    }

    /// `pattern.search(text)`.
    pub fn search(&self, text: &str) -> Result<Option<Match>, String> {
        match self.re.captures(text) {
            Ok(Some(c)) => Ok(Some(self.to_match(&c))),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("re.error: {e}")),
        }
    }

    /// `pattern.sub(replacement, text)` where `replacement` has no backslashes.
    pub fn sub_plain(&self, text: &str, replacement: &str) -> Result<String, String> {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for m in self.finditer(text)? {
            out.push_str(&text[last..m.start]);
            out.push_str(replacement);
            last = m.end;
        }
        out.push_str(&text[last..]);
        Ok(out)
    }
}
