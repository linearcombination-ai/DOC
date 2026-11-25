//! The JSON seam. `call(op, input_json, names)` returns a JSON string: `{"ok": ...}` or
//! `{"error": "..."}`; the core never panics on bad input.
//!
//! Protocol for the book-list endpoints (the core never touches a disk):
//!   1. `plan`     rows of one language          -> repositories to acquire
//!   2. (shell acquires them and lists each directory to depth 2)
//!   3. `wanted`   listing                       -> files whose text the core wants to read
//!   4. `finish_book_codes` / `finish_resource_types`  listing + texts -> the endpoint's answer

use doc_model::names::NameProvider;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::books::{self, Listing, Texts};
use crate::catalog::{self, QueryKind, Repo};
use crate::findings::Findings;
use crate::flags::Flags;
use crate::locator::{self, Located};
use crate::tables_gen::{BOOK_CHAPTERS, BOOK_NAMES, USFM_RESOURCE_TYPES};

const DEFAULT_EN_RG_DIR: &str = "backend/passages/data/en_rg";

#[derive(Deserialize)]
struct NormalizeIn {
    rows: Vec<Value>,
    kind: String,
    #[serde(default)]
    flags: Flags,
}

#[derive(Deserialize)]
struct LangListIn {
    primary: Vec<Repo>,
    non_primary: Vec<Repo>,
    #[serde(default)]
    flags: Flags,
}

#[derive(Deserialize)]
struct PlanIn {
    /// `book_codes`, `book_codes_usfm_only` or `resource_types`.
    op: String,
    lang: String,
    /// The catalog rows of `lang` only, in catalog (sorted) order.
    rows: Vec<Repo>,
    en_rg_dir: Option<String>,
}

#[derive(Deserialize)]
struct FinishIn {
    lang: String,
    #[serde(default)]
    usfm_only: bool,
    #[serde(default)]
    books: String,
    repos: Vec<LocatedIn>,
    listing: Listing,
    #[serde(default)]
    texts: Value,
    #[serde(default)]
    flags: Flags,
}

#[derive(Deserialize)]
struct LocatedIn {
    url: String,
    dir: String,
    resource_type: String,
    fetch: bool,
}

impl From<LocatedIn> for Located {
    fn from(l: LocatedIn) -> Located {
        Located { url: l.url, dir: l.dir, resource_type: l.resource_type, fetch: l.fetch }
    }
}

fn parse<'a, T: Deserialize<'a>>(input: &'a str) -> Result<T, String> {
    serde_json::from_str(input).map_err(|e| format!("bad input: {e}"))
}

fn ok(v: Value) -> String {
    json!({ "ok": v }).to_string()
}

pub fn call(op: &str, input: &str, provider: &dyn NameProvider) -> String {
    match dispatch(op, input, provider) {
        Ok(v) => ok(v),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn dispatch(op: &str, input: &str, provider: &dyn NameProvider) -> Result<Value, String> {
    match op {
        "version" => Ok(json!({ "core": env!("CARGO_PKG_VERSION"), "kitchen": null })),
        "chapters_in_books" => Ok(chapters_in_books()),
        "flags" => Ok(json!({ "as_built": Flags::as_built(), "fixed": Flags::fixed() })),
        "normalize_catalog" => {
            let i: NormalizeIn = parse(input)?;
            let kind = match i.kind.as_str() {
                "primary" => QueryKind::Primary,
                "non_primary" => QueryKind::NonPrimary,
                other => return Err(format!("unknown catalog kind {other}")),
            };
            let mut findings = Findings::new();
            let n = catalog::normalize(&i.rows, kind, i.flags.skip_bad_catalog_rows, &mut findings);
            Ok(json!({ "repos": n.repos, "validation_failure": n.as_built_validation_failure, "findings": findings }))
        }
        "lang_codes_and_names" => {
            let mut i: LangListIn = parse(input)?;
            let mut findings = Findings::new();
            if i.flags.augment_leak {
                catalog::augment(&mut i.primary);
            }
            let list = catalog::lang_codes_and_names(&i.primary, &i.non_primary, i.flags.tolerate_empty_nonprimary, &mut findings);
            Ok(json!({ "result": list, "findings": findings }))
        }
        "plan" => {
            let mut i: PlanIn = parse(input)?;
            let en_rg = i.en_rg_dir.as_deref().unwrap_or(DEFAULT_EN_RG_DIR);
            if i.op == "resource_types" {
                // filter_excluded_resource_types runs before the augmentation
                i.rows.retain(|r| !(r.language.ietf_code == "en" && r.resource_type == "udb"));
            }
            let added = catalog::augment(&mut i.rows);
            let usfm = |t: &str| USFM_RESOURCE_TYPES.contains(&t);
            let repos = if i.op == "resource_types" {
                locator::repos_to_clone(&i.lang, &i.rows, en_rg, &|t| usfm(t) || matches!(t, "tq" | "tn" | "tn-condensed" | "bc"))
            } else {
                locator::repos_to_clone(&i.lang, &i.rows, en_rg, &|t| usfm(t) || matches!(t, "tq" | "tn"))
            };
            Ok(json!({ "repos": repos, "augmented": added, "findings": [] }))
        }
        "wanted" => {
            let f: FinishIn = parse(input)?;
            let tree = books::build_tree(&f.listing, &Texts::new());
            let repos: Vec<Located> = f.repos.into_iter().map(Into::into).collect();
            Ok(json!({ "reads": books::wanted_reads(&f.lang, &repos, &tree, &f.flags) }))
        }
        "finish_book_codes" => {
            let f: FinishIn = parse(input)?;
            let tree = books::build_tree(&f.listing, &books::texts_from_json(&f.texts));
            let repos: Vec<Located> = f.repos.into_iter().map(Into::into).collect();
            let mut findings = Findings::new();
            let out = books::book_list(&f.lang, f.usfm_only, &repos, &tree, &f.flags, provider, &mut findings);
            Ok(json!({ "result": out.result, "swallowed": out.swallowed, "findings": findings }))
        }
        "finish_resource_types" => {
            let f: FinishIn = parse(input)?;
            let tree = books::build_tree(&f.listing, &Texts::new());
            let repos: Vec<Located> = f.repos.into_iter().map(Into::into).collect();
            let mut findings = Findings::new();
            let codes = books::parse_book_codes(&f.books);
            let (result, swallowed) = books::resource_types(&f.lang, &codes, &repos, &tree, &f.flags, &mut findings);
            Ok(json!({ "result": result, "swallowed": swallowed, "findings": findings }))
        }
        other => Err(format!("unknown op {other}")),
    }
}

/// `chapters_in_books()`: `{code: [1..n]}` in BOOK_CHAPTERS order (the key order is observable).
fn chapters_in_books() -> Value {
    let mut map = serde_json::Map::new();
    for (code, n) in BOOK_CHAPTERS {
        map.insert((*code).to_string(), Value::from((1..=*n).collect::<Vec<u32>>()));
    }
    let _ = BOOK_NAMES;
    Value::Object(map)
}
