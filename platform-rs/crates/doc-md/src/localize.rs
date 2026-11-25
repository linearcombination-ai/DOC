//! C11 `localize`: DOC's `localize_non_usfm_book_names` as a pure function.
//!
//! As built it rewrites the English book name in the TN/TNC/TQ HTML (headings AND free
//! text) with the national name taken from a chosen USFM book of the language, else from
//! a language-wide list of names that the SHELL provides (`book_codes_for_lang_from_usfm_only`
//! clones and parses repositories in DOC, so it is an input here).
//!
//! Policy (captain, 2026-11-24/25): names are used as the resource provides them
//! (keep-as-written); kitchen is the fallback and findings are returned. This module
//! reproduces the as-built replacement and returns a [`NameFinding`] for every decision
//! a content team would want to see; the kitchen fallback lives behind the adapter crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::links::{PyError, Res};
use doc_model::model::{BookCode, NoteBook};
use doc_model::names::NameProvider;
use doc_model::tables;
use doc_model::text::word_sub_ci;

/// `(lang_code, book_code, national_book_name)` of a chosen USFM book.
#[derive(Clone, Debug, Deserialize)]
pub struct UsfmName(pub String, pub String, pub String);

#[derive(Clone, Debug, Deserialize, Default)]
pub struct LocalizerInput {
    pub usfm_books: Vec<UsfmName>,
    /// `book_codes_for_lang_from_usfm_only(lang)`: shell-provided, only for languages
    /// whose notes lack a chosen USFM book.
    #[serde(default)]
    pub usfm_names_by_lang: BTreeMap<String, Vec<(String, String)>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameFindingCode {
    /// The name came from a chosen USFM book of the same language.
    FromChosenUsfm,
    /// The name came from the shell's language-wide list.
    FromLanguageList,
    /// No national name was available: the English book name stays in the notes.
    KeptEnglish,
    /// The national name is identical to the English one for a non-English language.
    NameLooksEnglish,
    /// The resource gave no name; the provider (kitchen) supplied one (fixed mode only).
    FromProvider,
    /// The resource's name differs from the provider's (resource kept, as written).
    DiffersFromProvider,
    /// A national name containing a backslash would be a `re.sub` template in DOC.
    ReplacementHasBackslash,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NameFinding {
    pub code: NameFindingCode,
    pub lang: String,
    pub book: String,
    pub name: Option<String>,
}

fn english(code: &str) -> Option<&'static str> {
    tables::BOOKS.iter().find(|(c, _, _)| *c == code).map(|(_, n, _)| *n)
}

fn apply(text: &str, needle: &str, replacement: &str) -> String {
    word_sub_ci(text, needle, replacement)
}

/// In-place localisation of TN, TNC and TQ books (`tq` has no intros).
pub fn localize(
    input: &LocalizerInput,
    tn: &mut [NoteBook],
    tnc: &mut [NoteBook],
    tq: &mut [NoteBook],
) -> Res<Vec<NameFinding>> {
    localize_with(input, tn, tnc, tq, &doc_model::names::NoNames, false)
}

/// As [`localize`], with a name provider consulted when the resource has no name
/// (`provider_fallback`, a `fixed`-mode behaviour: DOC as built keeps the English name)
/// and for the cross-check findings (which never change the output).
pub fn localize_with(
    input: &LocalizerInput,
    tn: &mut [NoteBook],
    tnc: &mut [NoteBook],
    tq: &mut [NoteBook],
    provider: &dyn NameProvider,
    provider_fallback: bool,
) -> Res<Vec<NameFinding>> {
    let mut findings = Vec::new();
    // key (lang, book) -> (english name, replacement)
    let mut map: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    for UsfmName(lang, book, national) in &input.usfm_books {
        let Some(en) = english(book) else { continue };
        map.insert((lang.clone(), book.clone()), (en.to_string(), national.clone()));
        findings.push(NameFinding { code: NameFindingCode::FromChosenUsfm, lang: lang.clone(), book: book.clone(), name: Some(national.clone()) });
        if let Some(code) = doc_model::model::BookCode::parse(book) {
            if let Some(p) = provider.national_name(lang, code) {
                if !p.is_fallback && p.text != *national {
                    findings.push(NameFinding { code: NameFindingCode::DiffersFromProvider, lang: lang.clone(), book: book.clone(), name: Some(p.text) });
                }
            }
        }
        if lang != "en" && national == en {
            findings.push(NameFinding { code: NameFindingCode::NameLooksEnglish, lang: lang.clone(), book: book.clone(), name: Some(national.clone()) });
        }
    }
    let mut run = |books: &mut [NoteBook], has_book_intro: bool, has_chapter_intro: bool, findings: &mut Vec<NameFinding>| -> Res<()> {
        for book in books.iter_mut() {
            let key = (book.lang_code.clone(), book.book_code.as_str().to_string());
            let entry = match map.get(&key) {
                Some(e) => e.clone(),
                None => {
                    let Some(en) = english(&key.1) else { continue };
                    let listed = input
                        .usfm_names_by_lang
                        .get(&book.lang_code)
                        .and_then(|v| v.iter().find(|(c, _)| c == &key.1))
                        .map(|(_, n)| crate::links::py_strip(n).to_string())
                        .filter(|n| !n.is_empty());
                    let (text, code) = match listed {
                        Some(n) => (Some(n), NameFindingCode::FromLanguageList),
                        None if provider_fallback => (
                            BookCode::parse(&key.1)
                                .and_then(|c| provider.national_name(&key.0, c))
                                .filter(|p| !p.is_fallback)
                                .map(|p| p.text),
                            NameFindingCode::FromProvider,
                        ),
                        None => (None, NameFindingCode::KeptEnglish),
                    };
                    let Some(text) = text else {
                        findings.push(NameFinding { code: NameFindingCode::KeptEnglish, lang: key.0.clone(), book: key.1.clone(), name: None });
                        continue;
                    };
                    if text.contains('\\') {
                        findings.push(NameFinding { code: NameFindingCode::ReplacementHasBackslash, lang: key.0.clone(), book: key.1.clone(), name: Some(text.clone()) });
                        return Err(PyError::new("re.error", "bad escape in the replacement template"));
                    }
                    findings.push(NameFinding { code, lang: key.0.clone(), book: key.1.clone(), name: Some(text.clone()) });
                    let e = (en.to_string(), text);
                    map.insert(key.clone(), e.clone());
                    e
                }
            };
            let (needle, replacement) = entry;
            if replacement.contains('\\') {
                return Err(PyError::new("re.error", "bad escape in the replacement template"));
            }
            if has_book_intro {
                book.book_intro = apply(&book.book_intro, &needle, &replacement);
            }
            for (_, chapter) in book.chapters.0.iter_mut() {
                if has_chapter_intro {
                    chapter.intro_html = apply(&chapter.intro_html, &needle, &replacement);
                }
                for v in chapter.verses.iter_mut() {
                    v.html = apply(&v.html, &needle, &replacement);
                }
            }
        }
        Ok(())
    };
    run(tn, true, true, &mut findings)?;
    run(tnc, true, true, &mut findings)?;
    run(tq, false, false, &mut findings)?;
    Ok(findings)
}
