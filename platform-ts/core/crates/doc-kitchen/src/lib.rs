//! doc-kitchen: the thin adapter. Nothing outside this crate names a kitchen type.
//!
//! [`KitchenCanon`] implements `doc_model::names::NameProvider` over `canon::name`,
//! `canon::locale_for` and the `ulb` scheme (the same adapter as the P3 `doc-canon`, cut down to
//! names so the shell's wasm does not carry the USFM engine). Kitchen's English-of-last-resort
//! answer is reported as `is_fallback`, never as a name.
//!
//! Kitchen surface used: `locale_for`, `name`, `Form::Short`, `scheme("ulb")`, `Scheme::chapters`, `BookKey`.

use canon::{BookKey, Form};
use doc_model::model::BookCode;
use doc_model::names::{NameProvider, ProvidedName};

pub struct KitchenCanon;

impl KitchenCanon {
    /// The kitchen version this adapter was written against (surfaced in logs and the report).
    pub const KITCHEN_VERSION: &'static str = "v0.1.24 (0090c0beb19cfbd8e08b963a48ae2caeeb339821)";

    pub fn locale_for(lang: &str) -> Option<&'static str> {
        canon::locale_for(lang)
    }
}

fn key(book: BookCode) -> Option<BookKey> {
    let code = book.as_str().to_ascii_uppercase();
    let bytes: [u8; 3] = code.as_bytes().try_into().ok()?;
    Some(BookKey::new(bytes))
}

impl NameProvider for KitchenCanon {
    fn national_name(&self, lang: &str, book: BookCode) -> Option<ProvidedName> {
        let locale = canon::locale_for(lang);
        let asked: Vec<&str> = locale.into_iter().collect();
        let named = canon::name(key(book)?, &asked, Form::Short)?;
        let is_fallback = locale.is_none() || named.locale != locale.unwrap_or("");
        Some(ProvidedName { text: named.text.to_string(), locale: named.locale.to_string(), is_fallback })
    }

    fn chapter_count(&self, book: BookCode) -> Option<u32> {
        canon::scheme("ulb")?.chapters(key(book)?).map(u32::from)
    }
}
