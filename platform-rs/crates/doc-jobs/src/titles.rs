//! DOC's `get_languages_title_page_strings` (`document_generator.py:906`), ported: the title page of a document names its first two
//! languages, each as `"<English name> (<national name>): <resource type names> for <book names>"`, and says "Old Testament",
//! "New Testament" or "Old and New Testaments" for a language whose books cover a whole testament. DOC's Playwright tests read these
//! strings from the finished HTML.

use doc_model::tables::{BOOKS, BOOK_NUMBERS};

/// What the title needs to know about one resolved resource request (DOC's `found_resource_lookup_dtos`, in request order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleDto {
    pub lang_code: String,
    pub lang_name: String,
    pub localized_lang_name: String,
    pub resource_type_name: String,
    pub book_code: String,
}

/// `usfm_names` holds `(lang_code, lower-case book code, national book name)` of the parsed USFM books.
pub fn title_page_strings(dtos: &[TitleDto], usfm_names: &[(String, String, String)]) -> (String, String) {
    let mut lang_codes: Vec<&str> = Vec::new();
    for d in dtos {
        if !lang_codes.contains(&d.lang_code.as_str()) {
            lang_codes.push(&d.lang_code);
        }
    }
    let testament = |ot: bool| -> Vec<&'static str> { BOOK_NUMBERS.iter().filter(|(_, n)| n.parse::<u32>().map(|n| (n <= 39) == ot).unwrap_or(false)).map(|(c, _)| *c).collect() };
    let details = |lang: &str| -> String {
        let own: Vec<&TitleDto> = dtos.iter().filter(|d| d.lang_code == lang).collect();
        let Some(first) = own.first() else { return String::new() };
        let mut names: Vec<(String, String)> = Vec::new(); // (lower-case code, display name) in first-seen order
        let mut resource_names: Vec<&str> = Vec::new();
        for d in &own {
            let code = d.book_code.to_lowercase();
            if !names.iter().any(|(c, _)| *c == code) {
                let name = usfm_names
                    .iter()
                    .find(|(l, c, _)| l == lang && *c == code)
                    .map(|(_, _, n)| n.clone())
                    .or_else(|| BOOKS.iter().find(|(c, _, _)| *c == code).map(|(_, n, _)| (*n).to_string()))
                    .unwrap_or_else(|| code.clone());
                names.push((code, name));
            }
            if !resource_names.contains(&d.resource_type_name.as_str()) {
                resource_names.push(&d.resource_type_name);
            }
        }
        let have = |codes: &[&str]| codes.iter().all(|c| names.iter().any(|(n, _)| n == c));
        let (ot, nt) = (testament(true), testament(false));
        let (full_ot, full_nt) = (have(&ot), have(&nt));
        let mut items: Vec<String> = Vec::new();
        let mut remaining: Vec<&(String, String)> = names.iter().collect();
        if full_ot && full_nt {
            items.push("Old and New Testaments".into());
        } else {
            if full_ot {
                items.push("Old Testament".into());
                remaining.retain(|(c, _)| !ot.contains(&c.as_str()));
            }
            if full_nt {
                items.push("New Testament".into());
                remaining.retain(|(c, _)| !nt.contains(&c.as_str()));
            }
            remaining.sort_by_key(|(c, _)| BOOKS.iter().position(|(b, _, _)| b == c).unwrap_or(999));
            items.extend(remaining.iter().map(|(_, n)| n.clone()));
        }
        format!("{} ({}): {} for {}", first.lang_name, first.localized_lang_name, resource_names.join(", "), items.join(", "))
    };
    (lang_codes.first().map(|l| details(l)).unwrap_or_default(), lang_codes.get(1).map(|l| details(l)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto(lang: &str, name: &str, local: &str, rt: &str, book: &str) -> TitleDto {
        TitleDto { lang_code: lang.into(), lang_name: name.into(), localized_lang_name: local.into(), resource_type_name: rt.into(), book_code: book.into() }
    }

    /// The strings DOC's Playwright tests read from the HTML (`frontend/tests/e2e/doc_test_3.ts` and `doc_test_4.ts`).
    #[test]
    fn two_languages_keep_clicked_order() {
        let dtos = [dto("fr", "French", "Français", "Unlocked Literal Bible", "mat"), dto("ceb", "Cebuano", "Cebuano", "Unlocked Literal Bible", "mat")];
        let names = [("fr".to_string(), "mat".to_string(), "Matthieu".to_string()), ("ceb".to_string(), "mat".to_string(), "Mateo".to_string())];
        let (one, two) = title_page_strings(&dtos, &names);
        assert_eq!(one, "French (Français): Unlocked Literal Bible for Matthieu");
        assert_eq!(two, "Cebuano (Cebuano): Unlocked Literal Bible for Mateo");
    }

    #[test]
    fn resource_names_join_and_books_follow_canonical_order() {
        let dtos = [
            dto("en", "English", "English", "Unlocked Literal Bible", "jud"),
            dto("en", "English", "English", "Translation Notes", "jud"),
            dto("en", "English", "English", "Unlocked Literal Bible", "col"),
        ];
        let (one, two) = title_page_strings(&dtos, &[]);
        assert_eq!(one, "English (English): Unlocked Literal Bible, Translation Notes for Colossians, Jude");
        assert_eq!(two, "");
    }

    #[test]
    fn a_whole_testament_is_named_as_one() {
        let mut dtos: Vec<TitleDto> = BOOK_NUMBERS.iter().filter(|(_, n)| n.parse::<u32>().unwrap_or(0) >= 40).map(|(c, _)| dto("en", "English", "English", "Unlocked Literal Bible", c)).collect();
        dtos.push(dto("en", "English", "English", "Unlocked Literal Bible", "gen"));
        let (one, _) = title_page_strings(&dtos, &[]);
        assert_eq!(one, "English (English): Unlocked Literal Bible for New Testament, Genesis");
    }
}
