//! The name port: the only way the core asks "what is this book called in this language?"
//! from outside the resource itself. The implementation lives in the adapter crate
//! (`doc-canon`, the single place that imports scripture-kitchen); the core never names a
//! kitchen type.
//!
//! Policy (captain): names are used as the resource provides them (keep-as-written);
//! kitchen is the fallback; every decision is returned as a finding.

use crate::model::BookCode;

/// A name from outside the resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidedName {
    pub text: String,
    /// The locale the provider actually answered in (`"fr"` for `fr`; `"en"` when it fell back).
    pub locale: String,
    /// True when the provider had no name in the requested language and answered in a
    /// fallback locale (English).
    pub is_fallback: bool,
}

pub trait NameProvider {
    /// The long national name of `book` in `lang` (a DOC language code such as `pt-br`).
    fn national_name(&self, lang: &str, book: BookCode) -> Option<ProvidedName>;
    /// Chapter count of `book` in the provider's default (`ulb`) numbering.
    fn chapter_count(&self, book: BookCode) -> Option<u32>;
}

/// A provider with no data: the as-built behaviour (no fallback exists).
pub struct NoNames;
impl NameProvider for NoNames {
    fn national_name(&self, _lang: &str, _book: BookCode) -> Option<ProvidedName> {
        None
    }
    fn chapter_count(&self, _book: BookCode) -> Option<u32> {
        None
    }
}
