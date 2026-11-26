//! doc-canon: the thin adapter. Nothing outside this crate names a kitchen type.
//!
//! * [`KitchenCanon`] implements `doc_model::names::NameProvider` on `canon::name`,
//!   `canon::locale_for` and the `ulb` scheme.
//! * [`Overrides`] is a TEMPORARY BRIDGE (`bridge/doc_book_name_corrections.TEMPORARY-BRIDGE.tsv`):
//!   DOC's hand corrections, consulted before kitchen only until kitchen carries them. Per the
//!   captain they are upstreamed to kitchen for review, not kept as a DOC file.
//! * Kitchen's English-of-last-resort answer is reported as `is_fallback`, never as a name.
//!
//! Kitchen surface used (the conformance suite in `tests/conformance.rs` pins it):
//! `locale_for`, `name`, `Form::Short`, `scheme("ulb")`, `Scheme::chapters`, `BookKey`.

use std::collections::BTreeMap;

use canon::{BookKey, Form};
use doc_model::model::BookCode;
use doc_model::names::{NameProvider, ProvidedName};

/// DOC-owned name corrections: `lang<TAB>book<TAB>name` lines.
#[derive(Default, Clone, Debug)]
pub struct Overrides(BTreeMap<(String, String), String>);

impl Overrides {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn parse(tsv: &str) -> Overrides {
        let mut map = BTreeMap::new();
        for line in tsv.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
            let mut it = line.split('\t');
            if let (Some(l), Some(b), Some(n)) = (it.next(), it.next(), it.next()) {
                map.insert((l.to_string(), b.to_string()), n.to_string());
            }
        }
        Overrides(map)
    }
}

pub struct KitchenCanon {
    overrides: Overrides,
}

impl KitchenCanon {
    pub fn new() -> KitchenCanon {
        KitchenCanon { overrides: Overrides::default() }
    }
    pub fn with_overrides(overrides: Overrides) -> KitchenCanon {
        KitchenCanon { overrides }
    }
    /// The kitchen version this adapter was written against (surfaced in findings and logs).
    pub const KITCHEN_VERSION: &'static str = "v0.1.24 (0090c0beb19cfbd8e08b963a48ae2caeeb339821)";

    /// The canon locale a DOC language code resolves to, if kitchen holds names for it.
    pub fn locale_for(lang: &str) -> Option<&'static str> {
        canon::locale_for(lang)
    }
}

impl Default for KitchenCanon {
    fn default() -> Self {
        KitchenCanon::new()
    }
}

fn key(book: BookCode) -> Option<BookKey> {
    let code = book.as_str().to_ascii_uppercase();
    let bytes: [u8; 3] = code.as_bytes().try_into().ok()?;
    Some(BookKey::new(bytes))
}

impl NameProvider for KitchenCanon {
    fn national_name(&self, lang: &str, book: BookCode) -> Option<ProvidedName> {
        if let Some(name) = self.overrides.0.get(&(lang.to_string(), book.as_str().to_string())) {
            return Some(ProvidedName { text: name.clone(), locale: lang.to_string(), is_fallback: false });
        }
        let locale = canon::locale_for(lang);
        let asked: Vec<&str> = locale.into_iter().collect();
        let named = canon::name(key(book)?, &asked, Form::Short)?;
        // a language kitchen holds nothing for, or a book it lacks in that language, comes
        // back in English: that is a fallback, not a national name
        let is_fallback = locale.is_none() || named.locale != locale.unwrap_or("");
        Some(ProvidedName { text: named.text.to_string(), locale: named.locale.to_string(), is_fallback })
    }

    fn chapter_count(&self, book: BookCode) -> Option<u32> {
        canon::scheme("ulb")?.chapters(key(book)?).map(u32::from)
    }
}

/// kitchen's name fold (case, diacritics, spacing), exposed so measurement tools never name `canon`.
pub mod kitchen_gaps;
pub mod look;
pub mod repairs;
pub mod stitch;
pub mod usfm;
pub mod usfm_fixes;
mod usfm_fixes_tables;

pub fn fold(s: &str) -> String {
    canon::fold(s)
}

/// kitchen's `words/1` (`mise::words`) as the Translation Words boundary rule: a match may start
/// and end anywhere except strictly inside a word. A word is a maximal run of letters, marks and
/// digits holding at least one letter, extended through one joiner (apostrophe, hyphen, middle dot)
/// with a letter on both sides. Case folding is kitchen's simple fold. Offsets here are `char`
/// indexes; kitchen's own are UTF-8 bytes (UTF-16 only at its wasm wall), converted below.
pub struct KitchenWords;

impl doc_model::text::WordRule for KitchenWords {
    fn name(&self) -> &'static str {
        "kitchen_words_1"
    }
    fn fold(&self, c: char) -> char {
        mise::words::fold(c)
    }
    fn boundaries(&self, hay: &[char]) -> Vec<bool> {
        let text: String = hay.iter().collect();
        // byte offset to char index
        let mut char_at = vec![0usize; text.len() + 1];
        let mut b = 0usize;
        for (i, c) in hay.iter().enumerate() {
            for k in 0..c.len_utf8() {
                char_at[b + k] = i;
            }
            b += c.len_utf8();
        }
        char_at[b] = hay.len();
        let mut inside = vec![false; hay.len() + 1];
        mise::words::for_each_word(&text, |from, to| {
            let (f, t) = (char_at[from as usize], char_at[to as usize]);
            for flag in inside.iter_mut().take(t).skip(f + 1) {
                *flag = true;
            }
        });
        inside.into_iter().map(|x| !x).collect()
    }
}


// ---------------------------------------------------------------------------------------------
// The shell entry point with kitchen's word rule as the default (captain, 2026-11-25).

/// `doc_interleave::run_json` with the Translation Words rule the captain chose: kitchen's `words/1`
/// ("do it the kitchen way"). Precedence: an explicit `tw_word_rule` in the input (`python`, `uax29`,
/// `kitchen`); otherwise Python's `\b` when the input asks for `bug_compat: "as_built"` or sets the
/// `tw_python_word_boundary` flag (so the oracle comparisons stay exact); otherwise kitchen's rule.
/// For Thai, Khmer, Lao and Myanmar the rule is applied and a `TwWordRuleNoSpaceScript` finding is
/// returned (done in `doc-interleave`, which knows the languages).
pub fn run_json(input: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(input) else {
        return doc_interleave::run_json(input);
    };
    let explicit = v.get("tw_word_rule").and_then(|x| x.as_str()).map(str::to_string);
    if explicit.is_none() {
        let compat = v.get("bug_compat");
        let python = match compat {
            Some(serde_json::Value::String(s)) => s == "as_built",
            Some(serde_json::Value::Object(o)) => o.get("tw_python_word_boundary").and_then(|x| x.as_bool()).unwrap_or(false),
            _ => false,
        };
        if python {
            v["tw_word_rule"] = serde_json::Value::String("python".into());
        }
    }
    doc_interleave::run_json_with(&v.to_string(), Some(&KitchenWords))
}
