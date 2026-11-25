//! The book-list path (`get_book_codes_for_lang_`) and the resource-type path
//! (`get_resource_types`), as pure functions over a listing of the acquired repositories.
//!
//! Every place DOC's code raises is an explicit [`Failure`] here. Under the as-built flags a
//! failure empties the whole answer (DOC catches it one level up and returns `[]`); under the
//! fixed flags only the offending repository, file or code is skipped and a finding says so.

use std::collections::HashMap;

use doc_model::names::NameProvider;
use doc_model::tree::{Tree, TreeData};
use serde::Deserialize;
use serde_json::Value;

use crate::findings::{Finding, Findings};
use crate::flags::Flags;
use crate::locator::{update_repo_components, Located};
use crate::manifest;
use crate::names;
use crate::tables_gen::{BOOK_NAMES, RESOURCE_TYPE_CODES_AND_NAMES, USFM_RESOURCE_TYPES};

/// What the shell saw on disk: for each repository directory, whether it exists and the entries
/// to depth 2 (the deepest the glob patterns and `scandir` calls below reach).
#[derive(Debug, Default, Deserialize)]
pub struct Listing {
    pub repos: HashMap<String, RepoListing>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RepoListing {
    pub present: bool,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    /// Path relative to the assets directory, `/`-separated.
    pub p: String,
    /// True for a directory.
    pub d: bool,
}

/// The file contents the core asked for: `null` means the file did not decode as UTF-8.
pub type Texts = HashMap<String, Option<String>>;

/// Where DOC's code would have raised.
#[derive(Debug)]
pub struct Failure {
    pub kind: &'static str,
    pub detail: String,
    pub repo: Option<String>,
    pub book: Option<String>,
}

impl Failure {
    fn new(kind: &'static str, detail: impl Into<String>) -> Failure {
        Failure { kind, detail: detail.into(), repo: None, book: None }
    }
    pub fn manifest(kind: &'static str, detail: String, repo: &str) -> Failure {
        Failure { kind, detail, repo: Some(repo.to_string()), book: None }
    }
    fn at(mut self, repo: &str) -> Failure {
        self.repo = Some(repo.to_string());
        self
    }
    fn book(mut self, book: &str) -> Failure {
        self.book = Some(book.to_string());
        self
    }
    fn as_finding(&self, lang: &str) -> Finding {
        let mut f = Finding::new(self.kind, "error").lang(lang).detail(self.detail.clone());
        if let Some(r) = &self.repo {
            f = f.repo(r);
        }
        if let Some(b) = &self.book {
            f = f.book(b);
        }
        f
    }
}

pub fn build_tree(listing: &Listing, texts: &Texts) -> Tree {
    let mut tree = Tree::default();
    let mut files = Vec::new();
    for (dir, repo) in &listing.repos {
        if !repo.present {
            continue;
        }
        tree.add_empty_dir(dir);
        for e in &repo.entries {
            if e.d {
                tree.add_empty_dir(&e.p);
            } else {
                files.push(e.p.clone());
            }
        }
    }
    tree.add_dir("", &TreeData { files, texts: texts.clone() });
    tree
}

/// `find_usfm_files(dir)`: `.usfm` one level down, else `.txt` one level down, else `.txt` two
/// levels down; `title.txt` excluded. (`{}**/*.usfm`: `**` is `*` without `recursive=True`.)
pub fn find_usfm_files(tree: &Tree, dir: &str) -> Vec<String> {
    let mut files = tree.glob(&format!("{dir}**/*.usfm"));
    if files.is_empty() {
        files = tree.glob(&format!("{dir}**/*.txt"));
        if files.is_empty() {
            files = tree.glob(&format!("{dir}**/**/*.txt"));
        }
    }
    files.retain(|f| !f.ends_with("title.txt"));
    files
}

/// The two- or more-component split of a repo's directory name, as the list path sees it.
fn components(located: &Located, flags: &Flags) -> Vec<String> {
    let comps: Vec<String> = located.dir.split('_').map(str::to_string).collect();
    if flags.owner_prefix_in_list_path && located.url.contains("DCS-Mirror") {
        update_repo_components(comps)
    } else {
        comps
    }
}

fn is_usfm_type(t: &str) -> bool {
    USFM_RESOURCE_TYPES.contains(&t)
}

fn english(code: &str) -> Option<&'static str> {
    BOOK_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

fn book_index(code: &str) -> Option<usize> {
    BOOK_NAMES.iter().position(|(c, _)| *c == code)
}

fn read<'t>(tree: &'t Tree, path: &str, repo: &str) -> Result<Option<&'t str>, Failure> {
    tree.read(path).map_err(|e| Failure::new("UnreadableFile", e).at(repo))
}

/// `Path(file).stem.lower().split("-")[1]`.
fn book_code_from_file_name(file: &str, repo: &str) -> Result<String, Failure> {
    let stem = doc_model::tree::path_stem(file).to_lowercase();
    stem.split('-').nth(1).map(str::to_string).ok_or_else(|| Failure::new("UsfmFileNameWithoutBookCode", format!("{file}: no `-` in the stem, DOC raises IndexError")).at(repo))
}

/// The files `finish_book_codes` will read, in the order it reads them.
pub fn wanted_reads(lang: &str, repos: &[Located], tree: &Tree, flags: &Flags) -> Vec<String> {
    let mut reads = Vec::new();
    for located in repos {
        let comps = components(located, flags);
        if !is_usfm_type(&located.resource_type) {
            continue;
        }
        if comps.len() == 2 {
            reads.extend(find_usfm_files(tree, &located.dir));
            reads.extend(manifest::candidate(tree, &located.dir).map(|c| c.path));
        } else if comps.len() > 2 {
            reads.push(format!("{}/front/title.txt", located.dir));
        }
    }
    let _ = lang;
    reads.sort();
    reads.dedup();
    reads
}

/// `get_book_names_from_usfm_metadata`: code to name, in file order, later duplicates replace.
fn names_from_usfm(tree: &Tree, located: &Located, lang: &str) -> Result<Vec<(String, String)>, Failure> {
    let mut out: Vec<(String, String)> = Vec::new();
    for file in find_usfm_files(tree, &located.dir) {
        let code = book_code_from_file_name(&file, &located.dir)?;
        let text = read(tree, &file, &located.dir)?.unwrap_or("");
        let name = names::maybe_localized_book_name(names::frontmatter(text), lang, &located.resource_type);
        match out.iter_mut().find(|(c, _)| *c == code) {
            Some(slot) => slot.1 = name,
            None => out.push((code, name)),
        }
    }
    Ok(out)
}

/// `get_book_name_from_title_file`: the whole title file, cleaned.
fn title_file_name(tree: &Tree, located: &Located) -> Result<String, Failure> {
    let path = format!("{}/front/title.txt", located.dir);
    if !tree.exists(&path) {
        return Ok(String::new());
    }
    let raw = read(tree, &path, &located.dir)?.unwrap_or("");
    Ok(crate::pystr::strip(&names::remove_xml_incompatible_chars(raw)).to_string())
}

/// `get_non_localized_book_names`.
fn non_localized(tree: &Tree, located: &Located, comps: &[String]) -> Result<Vec<(String, String)>, Failure> {
    let mut out = Vec::new();
    if comps.len() > 2 {
        if let Some(name) = english(&comps[1]) {
            out.push((comps[1].clone(), name.to_string()));
        }
    } else if comps.len() == 2 && (located.resource_type == "tn" || located.resource_type == "tq") {
        let entries = tree.scandir(&located.dir).ok_or_else(|| Failure::new("RepoDirectoryMissing", format!("scandir({}) raised FileNotFoundError", located.dir)).at(&located.dir))?;
        for (name, is_dir) in entries {
            if is_dir && english(&name).is_some() {
                let lower = name.to_lowercase();
                if let Some(n) = english(&lower) {
                    out.push((lower, n.to_string()));
                }
            }
        }
    }
    Ok(out)
}

/// Normalise a manifest value the way DOC would feed it on: a non-string chosen name makes
/// `.strip()` raise in DOC.
fn chosen_name(meta: &str, manifest_value: Option<&manifest::Title>, repo: &str, code: &str) -> Result<String, Failure> {
    if !meta.is_empty() {
        return Ok(meta.to_string());
    }
    match manifest_value {
        None => Ok(String::new()),
        Some(manifest::Title::Text(t)) => Ok(t.clone()),
        Some(manifest::Title::Other(what)) => Err(Failure::new("ManifestTitleNotText", format!("manifest title for {code} is {what}; DOC calls .strip() on it")).at(repo).book(code)),
    }
}

pub struct BookList {
    /// `None`: DOC's code would have raised and been swallowed one level up (as-built flags).
    pub result: Option<Vec<(String, String)>>,
    pub swallowed: Option<String>,
}

fn run_repo(tree: &Tree, lang: &str, usfm_only: bool, located: &Located, flags: &Flags, state: &mut ListState) -> Result<(), Failure> {
    let comps = components(located, flags);
    if is_usfm_type(&located.resource_type) && comps.len() == 2 {
        let meta = names_from_usfm(tree, located, lang)?;
        let manifest_names = manifest::book_names(tree, &located.dir)?;
        for (code, name) in meta {
            let manifest_name = manifest_names.iter().rev().find(|(c, _)| *c == code).map(|(_, t)| t);
            let chosen = chosen_name(&name, manifest_name, &located.dir, &code)?;
            state.localized.push((code, names::maybe_correct_book_name(lang, &names::normalize_localized_book_name(&chosen))));
        }
    } else if is_usfm_type(&located.resource_type) && comps.len() > 2 {
        let book_name = title_file_name(tree, located)?;
        state.localized.push((comps[1].clone(), names::maybe_correct_book_name(lang, &names::normalize_localized_book_name(&book_name))));
    }
    if !usfm_only || state.localized.is_empty() || state.localized.iter().any(|(_, n)| n.is_empty()) {
        state.non_localized.extend(non_localized(tree, located, &comps)?);
    }
    Ok(())
}

#[derive(Default)]
struct ListState {
    localized: Vec<(String, String)>,
    non_localized: Vec<(String, String)>,
}

/// `get_book_codes_for_lang_` over an acquired set of repositories.
#[allow(clippy::too_many_arguments)]
pub fn book_list(lang: &str, usfm_only: bool, repos: &[Located], tree: &Tree, flags: &Flags, provider: &dyn NameProvider, findings: &mut Findings) -> BookList {
    let mut state = ListState::default();
    for located in repos {
        if flags.owner_prefix_in_list_path && located.url.contains("DCS-Mirror") {
            let plain: Vec<String> = located.dir.split('_').map(str::to_string).collect();
            let updated = components(located, flags);
            if updated != plain {
                findings.push(Finding::new("OwnerPrefixHandled", "info").lang(lang).repo(&located.dir).detail(format!("directory name read as {updated:?} instead of {plain:?}; as built the list path takes component 1 of the unstripped name as the book code")));
            }
        }
        if let Err(failure) = run_repo(tree, lang, usfm_only, located, flags, &mut state) {
            if flags.tolerate_repo_errors {
                findings.push(failure.as_finding(lang));
                continue;
            }
            findings.push(failure.as_finding(lang));
            return BookList { result: None, swallowed: Some(format!("{}: {}", failure.kind, failure.detail)) };
        }
    }
    let localized_map = last_wins(&state.localized);
    let non_localized_map = last_wins(&state.non_localized);
    let mut codes: Vec<String> = Vec::new();
    for (c, _) in localized_map.iter().chain(non_localized_map.iter()) {
        if !codes.contains(c) {
            codes.push(c.clone());
        }
    }
    let mut merged: Vec<(usize, String, String)> = Vec::new();
    for code in codes {
        let Some(idx) = book_index(&code) else {
            let f = Finding::new("UnknownBookCode", "error").lang(lang).book(&code).detail(format!("`{code}` is not in BOOK_ID_MAP; DOC's sort raises KeyError({code:?}) and the whole list becomes empty"));
            findings.push(f);
            if flags.skip_unknown_book_codes {
                continue;
            }
            return BookList { result: None, swallowed: Some(format!("KeyError({code:?})")) };
        };
        let mut name = localized_map.iter().find(|(c, _)| *c == code).map(|(_, n)| n.clone()).unwrap_or_default();
        if name.is_empty() {
            name = non_localized_map.iter().find(|(c, _)| *c == code).map(|(_, n)| n.clone()).unwrap_or_default();
            if flags.kitchen_names {
                let localized_empty = localized_map.iter().find(|(c, _)| *c == code).is_none_or(|(_, n)| n.is_empty());
                if localized_empty {
                    if let Some(book) = doc_model::model::BookCode::parse(&code) {
                        match provider.national_name(lang, book) {
                            Some(p) if !p.is_fallback => {
                                findings.push(Finding::new("NameFromKitchen", "info").lang(lang).book(&code).detail(format!("the resource gives no name; kitchen's {} name {:?} used instead of English {:?}", p.locale, p.text, name)));
                                name = p.text;
                            }
                            _ => findings.push(Finding::new("NameEnglishFallback", "info").lang(lang).book(&code).detail("neither the resource nor kitchen has a name in this language; English shown (as built)")),
                        }
                    }
                }
            }
        }
        merged.push((idx, code, name));
    }
    merged.sort_by_key(|(idx, _, _)| *idx);
    BookList { result: Some(merged.into_iter().map(|(_, c, n)| (c, n)).collect()), swallowed: None }
}

/// Python `{code: name for code, name in pairs}`: first position kept, last value wins.
fn last_wins(pairs: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (c, n) in pairs {
        match out.iter_mut().find(|(k, _)| k == c) {
            Some(slot) => slot.1 = n.clone(),
            None => out.push((c.clone(), n.clone())),
        }
    }
    out
}

/// `book_codes` argument of the resource-types endpoint: `str.split(",")`, `all` expands.
pub fn parse_book_codes(books: &str) -> Vec<String> {
    let codes: Vec<String> = books.split(',').map(str::to_string).collect();
    if codes.first().is_some_and(|c| c == "all") {
        return BOOK_NAMES.iter().map(|(c, _)| (*c).to_string()).collect();
    }
    codes
}

fn is_bc_book_dir(name: &str) -> Option<String> {
    // re.search(r"^\d{2,}-[0-9a-z]{3}$", name), then name.split("-")[1].lower()
    let (digits, code) = name.split_once('-')?;
    let ok_digits = digits.chars().count() >= 2 && digits.chars().all(|c| matches!(unicode_general_category::get_general_category(c), unicode_general_category::GeneralCategory::DecimalNumber));
    let ok_code = code.len() == 3 && code.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase());
    (ok_digits && ok_code).then(|| code.to_lowercase())
}

/// `get_resource_types` + the endpoint's `unique_tuples` and sort by name.
pub fn resource_types(lang: &str, book_codes: &[String], repos: &[Located], tree: &Tree, flags: &Flags, findings: &mut Findings) -> (Option<Vec<(String, String)>>, Option<String>) {
    let mut found: Vec<(String, String)> = Vec::new();
    for located in repos {
        let rt = located.resource_type.as_str();
        let book_assets = match rt {
            "tq" | "tn" | "tn-condensed" | "bc" => match tree.scandir(&located.dir) {
                Some(entries) => entries
                    .into_iter()
                    .filter(|(name, is_dir)| {
                        *is_dir
                            && !name.starts_with('.')
                            && if rt == "bc" { is_bc_book_dir(name).is_some_and(|c| book_codes.contains(&c)) } else { book_codes.contains(&name.to_lowercase()) }
                    })
                    .count(),
                None => {
                    let failure = Failure::new("RepoDirectoryMissing", format!("scandir({}) raised FileNotFoundError", located.dir)).at(&located.dir);
                    findings.push(failure.as_finding(lang));
                    if flags.tolerate_repo_errors {
                        continue;
                    }
                    return (None, Some(format!("{}: {}", failure.kind, failure.detail)));
                }
            },
            t if is_usfm_type(t) => find_usfm_files(tree, &located.dir).len(),
            _ => 0,
        };
        if book_assets > 0 || rt == "tw" {
            if let Some((_, name)) = RESOURCE_TYPE_CODES_AND_NAMES.iter().find(|(k, _)| *k == rt) {
                found.push((rt.to_string(), (*name).to_string()));
            }
        }
    }
    let mut unique: Vec<(String, String)> = Vec::new();
    for item in found {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique.sort_by(|a, b| a.1.cmp(&b.1));
    (Some(unique), None)
}

/// Used by the API layer to turn a JSON value into a map of texts.
pub fn texts_from_json(v: &Value) -> Texts {
    v.as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string))).collect())
        .unwrap_or_default()
}
