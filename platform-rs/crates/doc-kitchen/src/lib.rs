//! doc-kitchen: `doc_model::names::NameProvider` on kitchen's `canon` crate. Same adapter as the
//! P3 `doc-canon` crate's `KitchenCanon`, without that crate's USFM, stitching and interleaver
//! dependencies (onion, bttwriter, mise words), which P4 does not need: building them would cost
//! several minutes of CI time for nothing. The conformance test pins the kitchen surface used:
//! `locale_for`, `name`, `Form::Short`, `scheme("ulb")`, `BookKey`.

use canon::{BookKey, Form};
use doc_model::model::BookCode;
use doc_model::names::{NameProvider, ProvidedName};

pub struct KitchenCanon;

impl KitchenCanon {
    /// The kitchen version the adapter was written against (logged at startup).
    pub const KITCHEN_VERSION: &'static str = "v0.1.24 (0090c0beb19cfbd8e08b963a48ae2caeeb339821)";

    /// The canon locale a DOC language code resolves to, if kitchen holds names for it.
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
        // a language kitchen holds nothing for, or a book it lacks in that language, comes back in
        // English: that is a fallback, not a national name
        let is_fallback = locale.is_none() || named.locale != locale.unwrap_or("");
        Some(ProvidedName { text: named.text.to_string(), locale: named.locale.to_string(), is_fallback })
    }

    fn chapter_count(&self, book: BookCode) -> Option<u32> {
        canon::scheme("ulb")?.chapters(key(book)?).map(u32::from)
    }
}

#[cfg(test)]
mod conformance {
    use super::*;

    #[test]
    fn kitchen_surface_used_by_the_adapter() {
        let mat = BookCode::parse("mat").unwrap();
        let es = KitchenCanon.national_name("es-419", mat).unwrap();
        assert_eq!(es.text, "Mateo");
        assert!(!es.is_fallback);
        let none = KitchenCanon.national_name("zai", mat).unwrap();
        assert!(none.is_fallback, "a language kitchen holds no table for answers in English");
        assert_eq!(KitchenCanon.chapter_count(mat), Some(28));
        assert_eq!(KitchenCanon::locale_for("pt-PT"), Some("pt-br"));
    }
}
