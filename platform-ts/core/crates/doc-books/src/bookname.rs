//! The name DOC shows for a USFM book in a document's title: `get_localized_book_name` followed by `maybe_correct_book_name`.

use serde::Deserialize;
use serde_json::Value;

use crate::books::{self, Listing};
use crate::findings::{Finding, Findings};
use crate::locator;
use crate::tables_gen::USFM_RESOURCE_TYPES;
use crate::{manifest, names};

/// What the shell hands over for one USFM resource: the repository listed to depth 2 and the texts of its manifest files and `front/title.txt`
/// (the same shapes the book-list protocol uses).
#[derive(Debug, Default, Deserialize)]
pub struct NameInput {
    pub url: String,
    pub dir: String,
    #[serde(default)]
    pub listing: Listing,
    #[serde(default)]
    pub texts: Value,
    pub en_rg_dir: Option<String>,
}

const DEFAULT_EN_RG_DIR: &str = "backend/passages/data/en_rg";

/// The USFM's own markers (its frontmatter: for a Writer repository, the stitched text, whose title comes from `front/title.txt`), else the manifest,
/// else `front/title.txt` of a repository whose name has more than two parts. Empty when none gives a name: the caller then uses the English name.
pub fn usfm_book_name(lang_code: &str, resource_type: &str, book_code: &str, source: &str, input: &NameInput) -> (String, Findings) {
    let en_rg = input.en_rg_dir.as_deref().unwrap_or(DEFAULT_EN_RG_DIR);
    let tree = books::build_tree(&input.listing, &books::texts_from_json(&input.texts));
    let mut findings: Findings = Vec::new();
    let mut note = |f: books::Failure| findings.push(f.as_finding(lang_code));
    let mut name = names::maybe_localized_book_name(names::frontmatter(source), lang_code, resource_type);
    if name.is_empty() {
        match manifest::book_names(&tree, &input.dir) {
            Ok(list) => {
                if let Some((_, manifest::Title::Text(t))) = list.iter().find(|(c, _)| c == book_code) {
                    name = t.clone();
                }
            }
            Err(f) => note(f),
        }
        if name.is_empty() {
            let last = locator::get_last_segment(&input.url, lang_code, en_rg);
            if last.split('_').count() > 2 && USFM_RESOURCE_TYPES.contains(&resource_type) {
                match books::title_file_name_in(&tree, &input.dir) {
                    Ok(t) => name = t,
                    Err(f) => note(f),
                }
            }
        }
    }
    (names::maybe_correct_book_name(lang_code, &name), findings)
}

/// A finding is only returned for a name that could not be read; kept here so callers need not import [`Finding`].
pub type NameFindings = Vec<Finding>;
