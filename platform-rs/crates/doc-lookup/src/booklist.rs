//! The book list of a language (`get_book_codes_for_lang_`, `resource_lookup.py:822-933`, and its
//! helpers), over a [`Workspace`] of cloned repositories.
//!
//! `AsBuilt` reproduces the function including its failure behaviour: every exception DOC
//! catches (`except Exception: logger.exception(...)` around the whole language) becomes
//! [`Abort`], which the caller turns into `[]`. The cumulative `usfm_only` condition (it looks at
//! the localized names collected from the repositories processed *so far*) is reproduced.
//! `Fixed` isolates each repository, reads the owner prefix the way the document path does, keeps
//! names as written, and returns a [`Finding`] for everything it skipped or substituted.

use std::collections::HashMap;

use doc_contract::wire::BookEntry;
use doc_contract::{Finding, FindingCode, Mode, Severity};
use doc_model::names::NameProvider;
use doc_model::tree::{path_name, path_stem};

use crate::locator::RepoRef;
use crate::pytext::{
    book_name_from_frontmatter, maybe_correct_book_name, normalize_localized_book_name,
    py_strip, remove_xml_incompatible_chars, usfm_frontmatter,
};
use crate::reader::Workspace;
use crate::tables_gen::{BOOK_NAMES, USFM_RESOURCE_TYPES};
use crate::{Abort, Outcome};

/// How the book list treats the names translators wrote (`Fixed` mode): `Written` keeps them (the captain's rule, the default; a finding says
/// what DOC's rule would have shown), `Doc` applies DOC's normalisation (`2ኛ ቆሮንቶስ` becomes `2 ኛ ቆሮንቶስ`), which DOC's own UI test reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NameRule {
    #[default]
    Written,
    Doc,
}

/// `USE_LOCALIZED_BOOK_NAME` (true in DOC's `.env`).
pub const USE_LOCALIZED_BOOK_NAME: bool = true;

pub fn english_name(code: &str) -> Option<&'static str> {
    BOOK_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

pub fn book_index(code: &str) -> Option<usize> {
    BOOK_NAMES.iter().position(|(c, _)| *c == code)
}

pub fn is_usfm_type(resource_type: &str) -> bool {
    USFM_RESOURCE_TYPES.contains(&resource_type)
}

/// `find_usfm_files`: `.usfm`, else `.txt`, else `.txt` one level down; `title.txt` excluded.
pub fn find_usfm_files(ws: &Workspace, dir: &str) -> Vec<String> {
    let mut files = ws.glob(&format!("{dir}**/*.usfm"));
    if files.is_empty() {
        files = ws.glob(&format!("{dir}**/*.txt"));
        if files.is_empty() {
            files = ws.glob(&format!("{dir}**/**/*.txt"));
        }
    }
    files.retain(|f| !f.ends_with("title.txt"));
    files
}

/// A manifest value that is not a string is kept so the failure happens where DOC's does: when
/// the value is used.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Title {
    Text(String),
    NotText(&'static str),
}

fn yaml_kind(y: &yaml_rust2::Yaml) -> &'static str {
    use yaml_rust2::Yaml;
    match y {
        Yaml::Real(_) => "float",
        Yaml::Integer(_) => "int",
        Yaml::String(_) => "str",
        Yaml::Boolean(_) => "bool",
        Yaml::Array(_) => "list",
        Yaml::Hash(_) => "dict",
        Yaml::Alias(_) => "alias",
        Yaml::Null => "NoneType",
        Yaml::BadValue => "missing",
    }
}

/// `book_codes_and_names_from_manifest`: errors are what DOC's code would raise.
fn manifest_names(ws: &Workspace, dir: &str, mode: Mode, repo: &str, findings: &mut Vec<Finding>) -> Result<Vec<(String, Title)>, Abort> {
    let mut candidates = ws.glob(&format!("{dir}/**/manifest.yaml"));
    if candidates.is_empty() {
        candidates = ws.glob(&format!("{dir}/manifest.yaml"));
    }
    if candidates.is_empty() {
        candidates = ws.glob(&format!("{dir}/**/manifest.json"));
    }
    if candidates.is_empty() {
        candidates = ws.glob(&format!("{dir}/manifest.json"));
    }
    let Some(candidate) = candidates.first() else {
        return Ok(Vec::new());
    };
    let fail = |msg: String, findings: &mut Vec<Finding>| -> Result<Vec<(String, Title)>, Abort> {
        match mode {
            Mode::AsBuilt => Err(Abort(format!("manifest {candidate}: {msg}"))),
            Mode::Fixed => {
                findings.push(Finding::new(FindingCode::ManifestUnreadable, Severity::Warning, format!("{candidate}: {msg}")).repo(repo));
                Ok(Vec::new())
            }
        }
    };
    let text = match ws.read(candidate) {
        Ok(Some(t)) => t,
        Ok(None) => return fail("file vanished".into(), findings),
        Err(e) => return fail(e.to_string(), findings),
    };
    let mut out: Vec<(String, Title)> = Vec::new();
    if candidate.ends_with(".yaml") {
        use yaml_rust2::{Yaml, YamlLoader};
        // PyYAML's scanner skips a byte order mark at the start of the stream; yaml-rust2 does not
        let docs = match YamlLoader::load_from_str(text.strip_prefix('\u{feff}').unwrap_or(&text)) {
            Ok(d) => d,
            Err(e) => return fail(format!("YAML error: {e}"), findings),
        };
        if docs.len() != 1 {
            return fail(format!("{} YAML documents (safe_load needs exactly one)", docs.len()), findings);
        }
        let projects = match &docs[0] {
            Yaml::Hash(_) => &docs[0]["projects"],
            other => return fail(format!("top level is {} (TypeError)", yaml_kind(other)), findings),
        };
        let Yaml::Array(items) = projects else {
            return fail(format!("projects is {}", yaml_kind(projects)), findings);
        };
        for book in items {
            let (ident, title) = (&book["identifier"], &book["title"]);
            if !matches!(book, Yaml::Hash(_)) || matches!(ident, Yaml::BadValue) || matches!(title, Yaml::BadValue) {
                match mode {
                    Mode::AsBuilt => return Err(Abort(format!("manifest {candidate}: project without identifier/title (KeyError)"))),
                    Mode::Fixed => {
                        findings.push(Finding::new(FindingCode::ManifestUnreadable, Severity::Warning, format!("{candidate}: project entry without identifier/title")).repo(repo));
                        continue;
                    }
                }
            }
            let key = match ident {
                Yaml::String(s) => s.clone(),
                Yaml::Integer(i) => i.to_string(),
                _ => continue,
            };
            let value = match title {
                Yaml::String(s) => Title::Text(s.clone()),
                other => Title::NotText(yaml_kind(other)),
            };
            out.push((key, value));
        }
    } else {
        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(j) => j,
            Err(e) => return fail(format!("JSON error: {e}"), findings),
        };
        let project = &json["project"];
        let (id, name) = (project.get("id"), project.get("name"));
        match (id, name) {
            (Some(serde_json::Value::String(id)), Some(serde_json::Value::String(name))) => out.push((id.clone(), Title::Text(name.clone()))),
            (Some(serde_json::Value::String(id)), Some(_)) => out.push((id.clone(), Title::NotText("non-string"))),
            _ => return fail("project.id / project.name missing".into(), findings),
        }
    }
    Ok(out)
}

/// `get_book_name_from_title_file`.
fn title_file_name(ws: &Workspace, dir: &str, mode: Mode, repo: &str, findings: &mut Vec<Finding>) -> Result<String, Abort> {
    let path = format!("{dir}/front/title.txt");
    if !ws.exists(&path) {
        return Ok(String::new());
    }
    let raw = match ws.read(&path) {
        Ok(Some(t)) => t,
        Ok(None) => return Ok(String::new()),
        Err(e) => {
            return match mode {
                Mode::AsBuilt => Err(Abort(format!("{path}: {e}"))),
                Mode::Fixed => {
                    findings.push(Finding::new(FindingCode::RepoUnreadable, Severity::Warning, format!("{path}: {e}")).repo(repo));
                    Ok(String::new())
                }
            }
        }
    };
    match mode {
        // the entire file, stripped
        Mode::AsBuilt => Ok(py_strip(&remove_xml_incompatible_chars(&raw)).to_string()),
        // kitchen's rule (bttwriter stitch): the first non-empty line
        Mode::Fixed => {
            let cleaned = remove_xml_incompatible_chars(&raw);
            let lines: Vec<&str> = cleaned.lines().map(py_strip).filter(|l| !l.is_empty()).collect();
            if lines.len() > 1 {
                findings.push(
                    Finding::new(FindingCode::TitleFileMultiline, Severity::Info, format!("{} non-empty lines; the first is used", lines.len()))
                        .repo(repo),
                );
            }
            Ok(lines.first().map(|l| (*l).to_string()).unwrap_or_default())
        }
    }
}

/// The name DOC shows for a USFM book in a document (`get_localized_book_name` then `maybe_correct_book_name`, `parsing.py:591`): the
/// book's own markers (`source` is its USFM, stitched for a Writer repository), else the repository's manifest, else `front/title.txt` of a
/// repository whose name has more than two parts. Empty when none gives a name; the caller then uses the English name of the book code.
/// The marker name is normalised the way DOC does for a document title (DOC's tests read it); what could not be read comes back as findings.
pub fn usfm_book_name(ws: &Workspace, repo: &RepoRef, lang: &str, book: &str, source: &str) -> (String, Vec<Finding>) {
    let mut findings = Vec::new();
    let mut name = book_name_from_frontmatter(&usfm_frontmatter(source), lang, &repo.resource_type, true);
    if name.is_empty() {
        if let Ok(list) = manifest_names(ws, &repo.segment, Mode::Fixed, &repo.url, &mut findings) {
            if let Some((_, Title::Text(t))) = list.iter().find(|(c, _)| c == book) {
                name = t.clone();
            }
        }
        if name.is_empty() && repo.segment.split('_').count() > 2 && USFM_RESOURCE_TYPES.contains(&repo.resource_type.as_str()) {
            if let Ok(t) = title_file_name(ws, &repo.segment, Mode::Fixed, &repo.url, &mut findings) {
                name = t;
            }
        }
    }
    (maybe_correct_book_name(lang, &name), findings)
}

/// `get_book_names_from_usfm_metadata`: `(code, name)` in file order, a repeated code keeping its
/// first position and last value (a Python dict).
fn usfm_metadata_names(ws: &Workspace, repo: &RepoRef, lang: &str, mode: Mode, normalize: bool, findings: &mut Vec<Finding>) -> Result<Vec<(String, String)>, Abort> {
    let mut out: Vec<(String, String)> = Vec::new();
    for file in find_usfm_files(ws, &repo.segment) {
        let stem = path_stem(&file).to_lowercase();
        let Some(code) = stem.split('-').nth(1).map(str::to_string) else {
            match mode {
                Mode::AsBuilt => return Err(Abort(format!("{file}: no '-' in file name (IndexError)"))),
                Mode::Fixed => {
                    findings.push(Finding::new(FindingCode::UnknownBookCode, Severity::Warning, format!("{} has no book code in its name", path_name(&file))).repo(&repo.url));
                    continue;
                }
            }
        };
        let text = match ws.read(&file) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => match mode {
                Mode::AsBuilt => return Err(Abort(format!("{file}: {e}"))),
                Mode::Fixed => {
                    findings.push(Finding::new(FindingCode::RepoUnreadable, Severity::Warning, format!("{}: {e}", path_name(&file))).repo(&repo.url).book(&code));
                    continue;
                }
            },
        };
        let front = usfm_frontmatter(&text);
        let name = book_name_from_frontmatter(&front, lang, &repo.resource_type, normalize || mode == Mode::AsBuilt);
        match out.iter_mut().find(|(c, _)| *c == code) {
            Some(slot) => slot.1 = name,
            None => out.push((code, name)),
        }
    }
    Ok(out)
}

fn non_localized_names(ws: &Workspace, repo: &RepoRef, mode: Mode, findings: &mut Vec<Finding>) -> Result<Vec<(String, String)>, Abort> {
    let comps = &repo.components;
    let mut out = Vec::new();
    if comps.len() > 2 {
        let code = &comps[1];
        if let Some(name) = english_name(code) {
            out.push((code.clone(), name.to_string()));
        }
    } else if comps.len() == 2 && (repo.resource_type == "tn" || repo.resource_type == "tq") {
        match ws.scandir(&repo.segment) {
            Some(entries) => {
                for (name, is_dir) in entries {
                    if is_dir {
                        if let Some(en) = english_name(&name) {
                            out.push((name.to_lowercase(), en.to_string()));
                        }
                    }
                }
            }
            None => match mode {
                Mode::AsBuilt => return Err(Abort(format!("scandir {}: FileNotFoundError", repo.segment))),
                Mode::Fixed => {
                    // the repository was not fetched: already reported as RepoUnavailable
                    let _ = findings;
                }
            },
        }
    }
    Ok(out)
}

fn upsert(map: &mut Vec<(String, String)>, code: String, name: String) {
    match map.iter_mut().find(|(c, _)| *c == code) {
        Some(slot) => slot.1 = name,
        None => map.push((code, name)),
    }
}

/// The book list of `lang`: `repos` is `repos_to_clone`'s full list (including `rg`).
pub fn book_codes_for_lang(
    lang: &str,
    repos: &[RepoRef],
    ws: &Workspace,
    usfm_only: bool,
    mode: Mode,
    names: &dyn NameProvider,
    rule: NameRule,
) -> Result<Outcome<Vec<BookEntry>>, Abort> {
    let normalize = rule == NameRule::Doc;
    let mut findings: Vec<Finding> = Vec::new();
    let mut localized: Vec<(String, String)> = Vec::new();
    let mut non_localized: Vec<(String, String)> = Vec::new();
    let mut resource_named: HashMap<String, bool> = HashMap::new(); // fixed: code -> name came from a resource

    for repo in repos {
        let usfm = is_usfm_type(&repo.resource_type);
        if USE_LOCALIZED_BOOK_NAME && repo.components.len() == 2 && usfm {
            let meta = usfm_metadata_names(ws, repo, lang, mode, normalize, &mut findings)?;
            let manifest = manifest_names(ws, &repo.segment, mode, &repo.url, &mut findings)?;
            for (code, name) in meta {
                let manifest_name = manifest.iter().rev().find(|(c, _)| *c == code).map(|(_, t)| t.clone());
                let chosen: String = if !name.is_empty() {
                    name
                } else {
                    match manifest_name {
                        None => String::new(),
                        Some(Title::Text(t)) => t,
                        Some(Title::NotText(kind)) => match mode {
                            Mode::AsBuilt => return Err(Abort(format!("manifest title for {code} is {kind} (AttributeError)"))),
                            Mode::Fixed => {
                                findings.push(Finding::new(FindingCode::ManifestUnreadable, Severity::Warning, format!("title of {code} is {kind}")).repo(&repo.url).book(&code));
                                String::new()
                            }
                        },
                    }
                };
                let final_name = match mode {
                    Mode::AsBuilt => maybe_correct_book_name(lang, &normalize_localized_book_name(&chosen)),
                    Mode::Fixed if normalize => maybe_correct_book_name(lang, &normalize_localized_book_name(&chosen)),
                    Mode::Fixed => maybe_correct_book_name(lang, py_strip(&chosen)),
                };
                if mode == Mode::Fixed && !final_name.is_empty() {
                    resource_named.insert(code.clone(), true);
                    note_normalisation(&mut findings, lang, &code, py_strip(&chosen), &final_name, &repo.url, normalize);
                }
                localized.push((code, final_name));
            }
        } else if USE_LOCALIZED_BOOK_NAME && repo.components.len() > 2 && usfm {
            let title = title_file_name(ws, &repo.segment, mode, &repo.url, &mut findings)?;
            let code = repo.components[1].clone();
            let final_name = match mode {
                Mode::AsBuilt => maybe_correct_book_name(lang, &normalize_localized_book_name(&title)),
                Mode::Fixed if normalize => maybe_correct_book_name(lang, &normalize_localized_book_name(&title)),
                Mode::Fixed => maybe_correct_book_name(lang, &title),
            };
            if mode == Mode::Fixed && !final_name.is_empty() {
                resource_named.insert(code.clone(), true);
                note_normalisation(&mut findings, lang, &code, &title, &final_name, &repo.url, normalize);
            }
            localized.push((code, final_name));
        }
        if !usfm_only || localized.is_empty() || localized.iter().any(|(_, n)| n.is_empty()) {
            non_localized.extend(non_localized_names(ws, repo, mode, &mut findings)?);
        }
    }

    let mut localized_map: Vec<(String, String)> = Vec::new();
    for (c, n) in localized {
        upsert(&mut localized_map, c, n);
    }
    let mut non_localized_map: Vec<(String, String)> = Vec::new();
    for (c, n) in non_localized {
        upsert(&mut non_localized_map, c, n);
    }
    let mut codes: Vec<String> = Vec::new();
    for (c, _) in localized_map.iter().chain(non_localized_map.iter()) {
        if !codes.contains(c) {
            codes.push(c.clone());
        }
    }
    let mut merged: Vec<(usize, String, String)> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for code in &codes {
        let local = localized_map.iter().find(|(c, _)| c == code).map(|(_, n)| n.as_str()).unwrap_or("");
        let mut name = local.to_string();
        let from_resource = !name.is_empty();
        if name.is_empty() {
            name = non_localized_map.iter().find(|(c, _)| c == code).map(|(_, n)| n.clone()).unwrap_or_default();
        }
        match book_index(code) {
            Some(i) => {
                if mode == Mode::Fixed && !from_resource {
                    name = fallback_name(lang, code, name, names, &mut findings);
                }
                merged.push((i, code.clone(), name));
            }
            None => match mode {
                Mode::AsBuilt => unknown.push(code.clone()),
                Mode::Fixed => findings.push(
                    Finding::new(FindingCode::UnknownBookCode, Severity::Warning, format!("{code:?} is not a canonical book code; the entry was skipped"))
                        .lang(lang)
                        .book(code),
                ),
            },
        }
    }
    if mode == Mode::AsBuilt && !unknown.is_empty() {
        unknown.sort();
        return Err(Abort(format!("KeyError: {:?} (not in BOOK_ID_MAP)", unknown[0])));
    }
    merged.sort_by_key(|(i, _, _)| *i);
    let value = merged.into_iter().map(|(_, c, n)| BookEntry(c, n)).collect();
    Ok(Outcome { value, findings })
}

/// Fixed mode: a book without a name in any resource: kitchen's national name if it has one,
/// else DOC's English name; both are reported.
fn fallback_name(lang: &str, code: &str, existing: String, names: &dyn NameProvider, findings: &mut Vec<Finding>) -> String {
    let Some(book) = doc_model::model::BookCode::parse(code) else {
        return existing;
    };
    if let Some(provided) = names.national_name(lang, book) {
        if !provided.is_fallback && !provided.text.is_empty() {
            findings.push(
                Finding::new(FindingCode::NameFromKitchen, Severity::Info, format!("no name in the resource; kitchen {:?} gives {:?}", provided.locale, provided.text))
                    .lang(lang)
                    .book(code),
            );
            return provided.text;
        }
    }
    findings.push(
        Finding::new(FindingCode::NameFellBackToEnglish, Severity::Warning, "no name in the resource or in kitchen for this language; the English name is shown")
            .lang(lang)
            .book(code),
    );
    if existing.is_empty() { english_name(code).unwrap_or("").to_string() } else { existing }
}

/// Fixed mode: say when DOC's normalisation would have rewritten what the translator wrote (the name is kept), or did rewrite it
/// (`NameRule::Doc`: the name DOC shows is returned and the finding keeps what was written), and when the name equals the English one
/// in a non-English language.
fn note_normalisation(findings: &mut Vec<Finding>, lang: &str, code: &str, written: &str, shown: &str, repo: &str, applied: bool) {
    let normalised = normalize_localized_book_name(written);
    if normalised != written {
        let detail = if applied { format!("DOC's rule changed {written:?} to {shown:?}") } else { format!("kept {written:?}; DOC would show {normalised:?}") };
        findings.push(Finding::new(FindingCode::NameNormalisationWouldChange, Severity::Info, detail).lang(lang).repo(repo).book(code));
    }
    let name = shown;
    let is_english_lang = lang == "en" || lang.starts_with("en-");
    if !is_english_lang && english_name(code).is_some_and(|en| en.eq_ignore_ascii_case(name)) {
        findings.push(
            Finding::new(FindingCode::NameLooksEnglish, Severity::Info, format!("{name:?} is the English name"))
                .lang(lang)
                .repo(repo)
                .book(code),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locator::repos_to_clone;
    use crate::catalog::{Language, RepoEntry};
    use crate::reader::{MemRepo, RepoReader};
    use doc_model::names::NoNames;
    use std::sync::Arc;

    fn entry(url: &str, rt: &str, lang: &str) -> RepoEntry {
        RepoEntry {
            repo_url: url.into(),
            resource_type: rt.into(),
            language: Language { english_name: "L".into(), ietf_code: lang.into(), national_name: "L".into(), direction: "ltr".into() },
        }
    }

    fn setup(entries: Vec<(&str, &str, MemRepo)>, lang: &str, mode: Mode) -> (Vec<RepoRef>, Workspace) {
        let catalog: Vec<RepoEntry> = entries.iter().map(|(u, rt, _)| entry(u, rt, lang)).collect();
        let (refs, _) = repos_to_clone(lang, &catalog, mode);
        let ws = Workspace::new(
            refs.iter().zip(entries).map(|(r, (_, _, repo))| (r.segment.clone(), Arc::new(repo) as Arc<dyn RepoReader>)).collect(),
        );
        (refs, ws)
    }

    #[test]
    fn whole_language_usfm_names_are_normalised_as_built() {
        let repo = MemRepo::new()
            .with("41-MAT.usfm", "\\id MAT\n\\h MATEO\n\\toc1 x\n\\c 1\n\\v 1 t")
            .with("42-MRK.usfm", "\\id MRK\n\\toc2 marcos\n\\c 1");
        let (refs, ws) = setup(vec![("https://h/WA-Catalog/es-419_ulb", "ulb", repo)], "es-419", Mode::AsBuilt);
        let out = book_codes_for_lang("es-419", &refs, &ws, false, Mode::AsBuilt, &NoNames, NameRule::Written).unwrap();
        assert_eq!(out.value, vec![BookEntry("mat".into(), "Mateo".into()), BookEntry("mrk".into(), "Marcos".into())]);
    }

    #[test]
    fn owner_prefix_empties_the_language_as_built_but_not_fixed() {
        let ok = MemRepo::new().with("front/title.txt", "Luka\n").with("01.usfm", "x");
        let prefixed = MemRepo::new().with("front/title.txt", "Matayo");
        let entries = vec![
            ("https://h/o/ln_luk_text_reg", "reg", ok.clone()),
            ("https://h/DCS-Mirror/faustin-azaza_ln_mat_text_reg", "reg", prefixed.clone()),
        ];
        let (refs, ws) = setup(entries.clone(), "ln", Mode::AsBuilt);
        let err = book_codes_for_lang("ln", &refs, &ws, false, Mode::AsBuilt, &NoNames, NameRule::Written).unwrap_err();
        assert!(err.0.contains("KeyError"), "{err:?}");
        let (refs, ws) = setup(entries, "ln", Mode::Fixed);
        let out = book_codes_for_lang("ln", &refs, &ws, false, Mode::Fixed, &NoNames, NameRule::Written).unwrap();
        assert_eq!(out.value, vec![BookEntry("mat".into(), "Matayo".into()), BookEntry("luk".into(), "Luka".into())]);
    }

    #[test]
    fn manifest_with_a_byte_order_mark_is_read_as_pyyaml_reads_it() {
        let manifest = "\u{feff}---\n\ndublin_core:\n  conformsto: 'rc0.2'\nprojects:\n  - identifier: 'mat'\n    title: 'Matthieu'\n";
        let repo = MemRepo::new().with("manifest.yaml", manifest).with("41-MAT.usfm", "\\id MAT\n\\c 1\n");
        let (refs, ws) = setup(vec![("https://h/WA-Catalog/fr_ulb", "ulb", repo)], "fr", Mode::AsBuilt);
        let out = book_codes_for_lang("fr", &refs, &ws, true, Mode::AsBuilt, &NoNames, NameRule::Written).unwrap();
        assert_eq!(out.value, vec![BookEntry("mat".into(), "Matthieu".into())]);
    }

    #[test]
    fn fixed_keeps_names_as_written() {
        let repo = MemRepo::new().with("front/title.txt", "Wimbo Wa Sulemani\nsecond line");
        let (refs, ws) = setup(vec![("https://h/o/sw_sng_text_reg", "reg", repo)], "sw", Mode::Fixed);
        let fixed = book_codes_for_lang("sw", &refs, &ws, false, Mode::Fixed, &NoNames, NameRule::Written).unwrap();
        assert_eq!(fixed.value, vec![BookEntry("sng".into(), "Wimbo Wa Sulemani".into())]);
        assert!(fixed.findings.iter().any(|f| f.code == FindingCode::NameNormalisationWouldChange));
        let (refs, ws) = setup(vec![("https://h/o/sw_sng_text_reg", "reg", MemRepo::new().with("front/title.txt", "Wimbo Wa Sulemani"))], "sw", Mode::AsBuilt);
        let built = book_codes_for_lang("sw", &refs, &ws, false, Mode::AsBuilt, &NoNames, NameRule::Written).unwrap();
        assert_eq!(built.value, vec![BookEntry("sng".into(), "Wimbo wa sulemani".into())]);
    }

    /// The captain's rule keeps what translators wrote; DOC's own UI test reads DOC's normalised form (`2 ኛ ቆሮንቶስ`, with a space), so a switch applies
    /// DOC's rule and the finding keeps the written form.
    #[test]
    fn the_doc_rule_normalises_and_reports_what_was_written() {
        let repo = MemRepo::new().with("front/title.txt", "2ኛ ቆሮንቶስ");
        let (refs, ws) = setup(vec![("https://h/o/am_2co_text_reg", "reg", repo)], "am", Mode::Fixed);
        let written = book_codes_for_lang("am", &refs, &ws, false, Mode::Fixed, &NoNames, NameRule::Written).unwrap();
        assert_eq!(written.value, vec![BookEntry("2co".into(), "2ኛ ቆሮንቶስ".into())]);
        let doc = book_codes_for_lang("am", &refs, &ws, false, Mode::Fixed, &NoNames, NameRule::Doc).unwrap();
        assert_eq!(doc.value, vec![BookEntry("2co".into(), "2 ኛ ቆሮንቶስ".into())]);
        assert!(doc.findings.iter().any(|f| f.code == FindingCode::NameNormalisationWouldChange && f.detail.contains("DOC's rule changed")));
    }
}
