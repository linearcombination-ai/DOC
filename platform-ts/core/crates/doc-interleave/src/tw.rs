//! C13 Translation Words: the per-chapter word list (link out or in-document anchors) and
//! the appended definitions section (include mode).
//!
//! Matching reproduces DOC's `re.search(r"\b" + re.escape(word) + r"\b", html, IGNORECASE)`
//! over the chapter's *HTML* (tags included), with Python's notion of a word character.

use doc_model::model::{Part, TwBook, UsfmBook};
use doc_request::request::{ResourceRequest, TwMode};
use doc_model::text::{h1, prepare, PythonBoundary, WordRule};

/// UAX #29 word segmentation (`unicode-segmentation`): a match may start and end wherever the
/// standard puts a word boundary. Apostrophes inside words (`don't`) do not split; hyphens do.
pub struct Uax29;

impl WordRule for Uax29 {
    fn name(&self) -> &'static str {
        "uax29"
    }
    fn boundaries(&self, hay: &[char]) -> Vec<bool> {
        use unicode_segmentation::UnicodeSegmentation;
        let text: String = hay.iter().collect();
        let mut ok = vec![false; hay.len() + 1];
        // byte offset -> char index
        let mut char_at = std::collections::HashMap::new();
        let mut b = 0usize;
        for (i, c) in hay.iter().enumerate() {
            char_at.insert(b, i);
            b += c.len_utf8();
        }
        char_at.insert(b, hay.len());
        for (start, seg) in text.split_word_bound_indices() {
            if let Some(i) = char_at.get(&start) {
                ok[*i] = true;
            }
            if let Some(i) = char_at.get(&(start + seg.len())) {
                ok[*i] = true;
            }
        }
        ok
    }
}

/// `Path(path).stem`.
pub fn path_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// `(localized word, file stem)` for every TW entry found in `content`, de-duplicated
/// by word, in the TW book's own order (DOC sorts it by display word).
pub fn words_in(tw: &TwBook, content: &str, rule: &dyn WordRule) -> Vec<(String, String)> {
    let hay: Vec<char> = content.chars().collect();
    let prepared = prepare(rule, &hay);
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for entry in &tw.name_content_pairs {
        let needle: Vec<char> = entry.localized_word.chars().collect();
        if prepared.contains(rule, &needle) && !seen.contains(&entry.localized_word.as_str()) {
            seen.push(&entry.localized_word);
            out.push((entry.localized_word.clone(), path_stem(&entry.path).to_string()));
        }
    }
    out
}

/// `translation_words_content`: heading part and list part, or nothing.
pub fn content_parts(tw: &TwBook, content: &str, mode: TwMode, rule: &dyn WordRule) -> Vec<Part> {
    let words = words_in(tw, content, rule);
    if words.is_empty() {
        return Vec::new();
    }
    let rtl = tw.lang_direction.is_rtl();
    let spans: Vec<String> = words
        .iter()
        .map(|(word, stem)| match mode {
            TwMode::Link => format!(
                "<span><a href='https://bibleineverylanguage.org/resources/languages/{}?resource-type=tw'>{}</a></span>",
                tw.lang_code, word
            ),
            TwMode::Include | TwMode::IncludeUsed => format!("<span><a href='#{}-{}'>{}</a></span>", tw.lang_code, stem, word),
        })
        .collect();
    vec![
        Part::new(h1(&tw.resource_type_name), rtl),
        Part::new(format!("<div>{}</div>", spans.join(", ")), rtl),
    ]
}

/// `translation_words_section_for_book` (with `limit_words=False`, the only way DOC
/// calls it): the heading, then every definition with `<h3>` anchors.
pub fn definitions_section(tw: &TwBook, requests: &[ResourceRequest], _usfm: &[UsfmBook]) -> String {
    let mut out = String::new();
    if !tw.name_content_pairs.is_empty() {
        let mut langs: Vec<&str> = Vec::new();
        for r in requests {
            if !langs.contains(&r.lang_code.as_str()) {
                langs.push(&r.lang_code);
            }
        }
        if langs.len() > 1 {
            out.push_str(&format!("<h2>{} ({})</h2>", tw.resource_type_name, tw.lang_code));
        } else {
            out.push_str(&format!("<h2>{}</h2>", tw.resource_type_name));
        }
    }
    for entry in &tw.name_content_pairs {
        let from = format!("<h3>{}", entry.localized_word);
        let to = format!("<h3 id=\"{}-{}\">{}", tw.lang_code, path_stem(&entry.path), entry.localized_word);
        out.push_str(&entry.content.replace(&from, &to));
    }
    out
}

/// `TwMode::IncludeUsed`: keep only the entries whose word occurs in `scripture_html`
/// (chapter HTML of the requested USFM books of the language), by the same matching rule
/// as the per-chapter lists.
pub fn used_entries(tw: &TwBook, scripture_html: &[&str], rule: &dyn WordRule) -> TwBook {
    let hays: Vec<_> = scripture_html.iter().map(|h| prepare(rule, &h.chars().collect::<Vec<char>>())).collect();
    let mut kept = tw.clone();
    kept.name_content_pairs.retain(|e| {
        let needle: Vec<char> = e.localized_word.chars().collect();
        hays.iter().any(|h| h.contains(rule, &needle))
    });
    kept
}

/// Languages whose scripts put no spaces between words. No word rule without a dictionary is right
/// for them (P3 report section 3.2), so applying a Unicode rule to them carries a finding.
pub const NO_SPACE_LANGS: &[&str] = &["th", "km", "lo", "my"];

/// True for a language code (`th`, `km`, `lo`, `my`, with or without a region subtag) that is written
/// without spaces between words.
pub fn is_no_space_lang(lang: &str) -> bool {
    let primary = lang.split(['-', '_']).next().unwrap_or(lang).to_ascii_lowercase();
    NO_SPACE_LANGS.contains(&primary.as_str())
}

/// The rule `compat` asks for when the caller supplies none. The shell's default is kitchen's
/// `words/1` (captain, 2026-11-25: "do it the kitchen way"), supplied by `doc-canon::run_json`;
/// this crate cannot name kitchen, so without a caller-supplied rule `fixed` falls back to UAX #29.
pub fn default_rule(python_boundary: bool) -> Box<dyn WordRule> {
    if python_boundary {
        Box::new(PythonBoundary)
    } else {
        Box::new(Uax29)
    }
}
