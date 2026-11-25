//! C1 request validation (as a function, not only a schema), layout resolution and
//! C2 the cache key.
//!
//! `validate` reproduces `DocumentRequest.ensure_valid_document_request`
//! (`DOC:backend/doc/domain/model.py:256-363`), including its known gaps (no language
//! count rule, `2c_sl_sr_c` skips every check). It does not reproduce pydantic's lax
//! type coercion (for example `"yes"` for a bool): a wrong JSON type is a violation.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use doc_model::model::BookCode;
use doc_model::tables;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Lbo,
    Blo,
    Lvo,
    Bvo,
    /// Accepted by DOC's enum; STET is a separate product and has no interleaver here.
    Stet,
}

impl Strategy {
    pub fn code(self) -> &'static str {
        match self {
            Strategy::Lbo => "lbo",
            Strategy::Blo => "blo",
            Strategy::Lvo => "lvo",
            Strategy::Bvo => "bvo",
            Strategy::Stet => "stet",
        }
    }
    fn parse(s: &str) -> Option<Strategy> {
        Some(match s {
            "lbo" => Strategy::Lbo,
            "blo" => Strategy::Blo,
            "lvo" => Strategy::Lvo,
            "bvo" => Strategy::Bvo,
            "stet" => Strategy::Stet,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Layout {
    #[serde(rename = "1c")]
    OneColumn,
    #[serde(rename = "1c_c")]
    OneColumnCompact,
    #[serde(rename = "2c_sl_sr")]
    TwoColumnSlSr,
    #[serde(rename = "2c_sl_sr_c")]
    TwoColumnSlSrCompact,
    #[serde(rename = "stet")]
    Stet,
}

impl Layout {
    pub fn code(self) -> &'static str {
        match self {
            Layout::OneColumn => "1c",
            Layout::OneColumnCompact => "1c_c",
            Layout::TwoColumnSlSr => "2c_sl_sr",
            Layout::TwoColumnSlSrCompact => "2c_sl_sr_c",
            Layout::Stet => "stet",
        }
    }
    fn parse(s: &str) -> Option<Layout> {
        Some(match s {
            "1c" => Layout::OneColumn,
            "1c_c" => Layout::OneColumnCompact,
            "2c_sl_sr" => Layout::TwoColumnSlSr,
            "2c_sl_sr_c" => Layout::TwoColumnSlSrCompact,
            "stet" => Layout::Stet,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Ui,
    Test,
    BielUi,
}

/// Translation Words are a user choice (captain, 2026-11-24): link out or include.
/// DOC has no such request field today; it is the process-wide
/// `LINK_RATHER_THAN_INCLUDE_TW_DEFINITIONS` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TwMode {
    #[default]
    Link,
    /// Append every definition of the language's Translation Words.
    Include,
    /// Append only the definitions of words that occur in the requested scripture (the
    /// size-saving middle way; DOC's unreachable `limit_words` branch, implemented with
    /// the same matching rule as the per-chapter lists).
    #[serde(rename = "include_used")]
    IncludeUsed,
}

impl TwMode {
    pub fn code(self) -> &'static str {
        match self {
            TwMode::Link => "link",
            TwMode::Include => "include",
            TwMode::IncludeUsed => "include_used",
        }
    }
    /// The per-chapter lists point at in-document anchors (and a definitions section exists).
    pub fn includes_definitions(self) -> bool {
        self != TwMode::Link
    }
}

/// The USFM resource types DOC accepts (`USFM_RESOURCE_TYPES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Usfm,
    Tn,
    Tnc,
    Tq,
    Tw,
    Bc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRequest {
    pub lang_code: String,
    /// The type code exactly as requested (`ulb`, `tn`, ...).
    pub resource_type: String,
    pub kind: ResourceKind,
    pub book: BookCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub layout_for_print: bool,
    pub generate_docx: bool,
    pub limit_words: bool,
    pub use_chapter_labels: bool,
    pub use_prince: bool,
    pub use_two_column_layout_for_tn_notes: bool,
    pub use_two_column_layout_for_tq_notes: bool,
    pub use_section_visual_separator: bool,
    pub show_tn_book_intro: bool,
    pub show_bc_book_intro: bool,
    pub show_tn_chapter_intro: bool,
    pub show_bc_chapter_commentary: bool,
}

/// A request that passed validation. `layout` is already resolved.
#[derive(Clone, Debug)]
pub struct ValidRequest {
    pub strategy: Strategy,
    pub layout: Layout,
    pub resources: Vec<ResourceRequest>,
    pub options: Options,
    pub tw_mode: TwMode,
    pub generate_pdf: bool,
    pub generate_epub: bool,
    pub email_address: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationCode {
    /// JSON shape: missing field, wrong type, unknown enum value.
    Schema,
    NoResourceRequests,
    InvalidResourceType,
    InvalidBookCode,
    TwoColumnNeedsBlo,
    TwoColumnNeedsEvenLanguages,
    TwoColumnNeedsSameBooks,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub code: ViolationCode,
    /// The exact message DOC's validator raises, for the rule violations.
    pub message: String,
}

fn schema(msg: impl Into<String>) -> Violation {
    Violation { code: ViolationCode::Schema, message: msg.into() }
}

const MSG_NEEDS_BLO: &str = "Two column scripture left, scripture right layout is only compatible with book language order assembly strategy.";
const MSG_NEEDS_EVEN: &str = "Two column scripture left, scripture right layout requires a non-zero even number of languages. For an uneven number of languages you'll want to use the one column layout kind.";
const MSG_NEEDS_SAME: &str = "Two column scripture left, scripture right layout requires the same books for each language chosen since they are displayed side by side. If you want a different set of books for each language you'll instead need to use the one column layout.";

fn kind_of(resource_type: &str) -> Option<ResourceKind> {
    if tables::USFM_TYPES.contains(&resource_type) {
        return Some(ResourceKind::Usfm);
    }
    match resource_type {
        "tn" => Some(ResourceKind::Tn),
        "tn-condensed" => Some(ResourceKind::Tnc),
        "tq" => Some(ResourceKind::Tq),
        "tw" => Some(ResourceKind::Tw),
        "bc" => Some(ResourceKind::Bc),
        _ => None,
    }
}

fn get_bool(obj: &serde_json::Map<String, Value>, key: &str, default: bool, errs: &mut Vec<Violation>) -> bool {
    match obj.get(key) {
        None => default,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            errs.push(schema(format!("{key}: expected a boolean, got {other}")));
            default
        }
    }
}

fn plausible_email(s: &str) -> bool {
    match s.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains('@')
        }
        None => false,
    }
}

/// `select_assembly_layout_kind` (`DOC:backend/doc/domain/document_generator.py:823-862`).
pub fn resolve_layout(strategy: Strategy, requested: Option<Layout>, source: Source, for_print: bool) -> Layout {
    if source == Source::Test {
        if let Some(l) = requested {
            return l;
        }
    }
    if strategy == Strategy::Stet {
        return Layout::Stet;
    }
    if for_print {
        Layout::OneColumnCompact
    } else {
        Layout::OneColumn
    }
}

/// Validate a request JSON value. All violations of the schema stage are returned
/// together; rule violations are returned one at a time in DOC's order (DOC raises
/// the first).
pub fn validate(json: &Value) -> Result<ValidRequest, Vec<Violation>> {
    let obj = match json.as_object() {
        Some(o) => o,
        None => return Err(vec![schema("request must be a JSON object")]),
    };
    let mut errs: Vec<Violation> = Vec::new();

    let strategy = match obj.get("assembly_strategy_kind") {
        None => {
            errs.push(schema("assembly_strategy_kind: field required"));
            None
        }
        Some(Value::String(s)) => match Strategy::parse(s) {
            Some(st) => Some(st),
            None => {
                errs.push(schema("assembly_strategy_kind: Input should be 'lbo', 'blo', 'lvo', 'bvo' or 'stet'"));
                None
            }
        },
        Some(_) => {
            errs.push(schema("assembly_strategy_kind: expected a string"));
            None
        }
    };
    // Absent key = DOC's default "1c"; explicit null = no layout requested.
    let requested_layout: Option<Layout> = match obj.get("assembly_layout_kind") {
        None => Some(Layout::OneColumn),
        Some(Value::Null) => None,
        Some(Value::String(s)) => match Layout::parse(s) {
            Some(l) => Some(l),
            None => {
                errs.push(schema("assembly_layout_kind: Input should be '1c', '1c_c', '2c_sl_sr', '2c_sl_sr_c' or 'stet'"));
                None
            }
        },
        Some(_) => {
            errs.push(schema("assembly_layout_kind: expected a string"));
            None
        }
    };
    let source = match obj.get("document_request_source") {
        None => Source::Test,
        Some(Value::String(s)) => match s.as_str() {
            "ui" => Source::Ui,
            "test" => Source::Test,
            "biel_ui" => Source::BielUi,
            _ => {
                errs.push(schema("document_request_source: Input should be 'ui', 'test' or 'biel_ui'"));
                Source::Test
            }
        },
        Some(_) => {
            errs.push(schema("document_request_source: expected a string"));
            Source::Test
        }
    };
    let email_address = match obj.get("email_address") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if plausible_email(s) => Some(s.clone()),
        Some(_) => {
            errs.push(schema("email_address: not a valid email address"));
            None
        }
    };
    let tw_mode = match obj.get("tw_mode") {
        None | Some(Value::Null) => TwMode::Link,
        Some(Value::String(s)) if s == "link" => TwMode::Link,
        Some(Value::String(s)) if s == "include" => TwMode::Include,
        Some(Value::String(s)) if s == "include_used" => TwMode::IncludeUsed,
        Some(_) => {
            errs.push(schema("tw_mode: Input should be 'link', 'include' or 'include_used'"));
            TwMode::Link
        }
    };
    let options = Options {
        layout_for_print: get_bool(obj, "layout_for_print", false, &mut errs),
        generate_docx: get_bool(obj, "generate_docx", false, &mut errs),
        limit_words: get_bool(obj, "limit_words", true, &mut errs),
        use_chapter_labels: get_bool(obj, "use_chapter_labels", false, &mut errs),
        use_prince: get_bool(obj, "use_prince", false, &mut errs),
        use_two_column_layout_for_tn_notes: get_bool(obj, "use_two_column_layout_for_tn_notes", false, &mut errs),
        use_two_column_layout_for_tq_notes: get_bool(obj, "use_two_column_layout_for_tq_notes", false, &mut errs),
        use_section_visual_separator: get_bool(obj, "use_section_visual_separator", false, &mut errs),
        show_tn_book_intro: get_bool(obj, "show_tn_book_intro", true, &mut errs),
        show_bc_book_intro: get_bool(obj, "show_bc_book_intro", true, &mut errs),
        show_tn_chapter_intro: get_bool(obj, "show_tn_chapter_intro", true, &mut errs),
        show_bc_chapter_commentary: get_bool(obj, "show_bc_chapter_commentary", true, &mut errs),
    };
    let generate_pdf = get_bool(obj, "generate_pdf", true, &mut errs);
    let generate_epub = get_bool(obj, "generate_epub", false, &mut errs);

    // resource_requests: raw (lang, type, book) strings first; rules check them later.
    let mut raw: Vec<(String, String, String)> = Vec::new();
    match obj.get("resource_requests") {
        None => errs.push(schema("resource_requests: field required")),
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let field = |k: &str| item.get(k).and_then(Value::as_str).map(str::to_string);
                match (field("lang_code"), field("resource_type"), field("book_code")) {
                    (Some(l), Some(t), Some(b)) => raw.push((l, t, b)),
                    _ => errs.push(schema(format!("resource_requests[{i}]: needs string lang_code, resource_type, book_code"))),
                }
            }
        }
        Some(_) => errs.push(schema("resource_requests: expected an array")),
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    let strategy = strategy.expect("checked above");

    // ---- DOC's model_validator, in DOC's order ----
    let rule = |code: ViolationCode, msg: &str| Err(vec![Violation { code, message: msg.to_string() }]);
    if raw.is_empty() {
        return rule(ViolationCode::NoResourceRequests, "DocumentRequest has no resource requests.");
    }
    let mut resources: Vec<ResourceRequest> = Vec::with_capacity(raw.len());
    for (lang, rtype, book) in &raw {
        let Some(kind) = kind_of(rtype) else {
            return rule(ViolationCode::InvalidResourceType, &format!("{rtype} is not a valid resource type"));
        };
        let Some(book) = BookCode::parse(book) else {
            return rule(ViolationCode::InvalidBookCode, &format!("{book} is not a valid book code"));
        };
        resources.push(ResourceRequest { lang_code: lang.clone(), resource_type: rtype.clone(), kind, book });
    }
    // USFM requests grouped by language, in first-appearance order.
    let mut groups: Vec<(String, Vec<BookCode>)> = Vec::new();
    for r in resources.iter().filter(|r| r.kind == ResourceKind::Usfm) {
        match groups.iter_mut().find(|(l, _)| *l == r.lang_code) {
            Some((_, books)) => books.push(r.book),
            None => groups.push((r.lang_code.clone(), vec![r.book])),
        }
    }
    let sorted_sets: Vec<Vec<BookCode>> = groups
        .iter()
        .map(|(_, books)| {
            let mut b = books.clone();
            b.sort();
            b.dedup();
            b
        })
        .collect();
    let lengths_equal = sorted_sets.iter().all(|s| s.len() == sorted_sets[0].len());
    let sets_equal = sorted_sets.iter().all(|s| *s == sorted_sets[0]);
    // DOC counts USFM request *entries*, not distinct languages (the set() is commented out).
    let n_usfm = resources.iter().filter(|r| r.kind == ResourceKind::Usfm).count();

    if requested_layout == Some(Layout::TwoColumnSlSr) {
        if strategy != Strategy::Blo {
            return rule(ViolationCode::TwoColumnNeedsBlo, MSG_NEEDS_BLO);
        } else if n_usfm % 2 != 0 {
            return rule(ViolationCode::TwoColumnNeedsEvenLanguages, MSG_NEEDS_EVEN);
        } else if !(lengths_equal && sets_equal) {
            return rule(ViolationCode::TwoColumnNeedsSameBooks, MSG_NEEDS_SAME);
        }
    }

    let layout = resolve_layout(strategy, requested_layout, source, options.layout_for_print);
    Ok(ValidRequest { strategy, layout, resources, options, tw_mode, generate_pdf, generate_epub, email_address })
}

// ---------------------------------------------------------------- cache key

const KEY_SCHEMA_VERSION: u64 = 1;
const HASH_HEX_LENGTH: usize = 12;

/// `request_key.slugify`: lowercase ASCII letters and digits, runs of anything else
/// become one hyphen, never empty.
pub fn slugify(value: &str) -> String {
    let lowered = value.to_lowercase();
    let mut out = String::new();
    let mut pending_hyphen = false;
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(c);
        } else {
            pending_hyphen = true;
        }
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

fn slug_group<'a>(values: impl Iterator<Item = &'a str>, noun: &str, max_len: usize) -> String {
    let mut unique: Vec<String> = Vec::new();
    for v in values {
        let s = slugify(v);
        if !unique.contains(&s) {
            unique.push(s);
        }
    }
    let joined = unique.join("-");
    if joined.chars().count() <= max_len {
        joined
    } else {
        format!("{}{}", unique.len(), noun)
    }
}

/// Python's `json.dumps(..., sort_keys=True, separators=(",", ":"), ensure_ascii=True)`.
pub fn canonical_json(value: &Value) -> String {
    fn escape(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                ' '..='~' => out.push(c),
                _ => {
                    let mut buf = [0u16; 2];
                    for unit in c.encode_utf16(&mut buf) {
                        out.push_str(&format!("\\u{:04x}", unit));
                    }
                }
            }
        }
        out.push('"');
    }
    fn go(v: &Value, out: &mut String) {
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => out.push_str(&n.to_string()),
            Value::String(s) => escape(s, out),
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    go(item, out);
                }
                out.push(']');
            }
            Value::Object(map) => {
                // serde_json's Map is ordered by key (no preserve_order feature).
                let sorted: BTreeMap<&String, &Value> = map.iter().collect();
                out.push('{');
                for (i, (k, item)) in sorted.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    escape(k, out);
                    out.push(':');
                    go(item, out);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    go(value, &mut out);
    out
}

/// `document_request_key`: `<slug>_<12 hex>`. Reproduces DOC's hashes exactly for every
/// request DOC can express. `tw_mode` joins the hashed settings only when it is
/// `include` and a TW resource is requested, so every link-mode key stays DOC-compatible.
pub fn request_key(req: &ValidRequest) -> String {
    let slug = [
        slug_group(req.resources.iter().map(|r| r.lang_code.as_str()), "langs", 20),
        slug_group(req.resources.iter().map(|r| r.resource_type.as_str()), "types", 20),
        slug_group(req.resources.iter().map(|r| r.book.as_str()), "books", 30),
    ]
    .join("_");
    let o = &req.options;
    // DOC: `contains_tw` is `re.match("tw.*", type)`, a prefix test.
    let has_tw = req.resources.iter().any(|r| r.resource_type.starts_with("tw"));
    let limit_words = if has_tw { Value::Bool(o.limit_words) } else { Value::Null };
    let mut settings = serde_json::Map::new();
    settings.insert("assembly_layout_kind".into(), Value::String(req.layout.code().into()));
    settings.insert("assembly_strategy_kind".into(), Value::String(req.strategy.code().into()));
    settings.insert("generate_docx".into(), Value::Bool(o.generate_docx));
    settings.insert("layout_for_print".into(), Value::Bool(o.layout_for_print));
    settings.insert("limit_words".into(), limit_words);
    settings.insert("show_bc_book_intro".into(), Value::Bool(o.show_bc_book_intro));
    settings.insert("show_bc_chapter_commentary".into(), Value::Bool(o.show_bc_chapter_commentary));
    settings.insert("show_tn_book_intro".into(), Value::Bool(o.show_tn_book_intro));
    settings.insert("show_tn_chapter_intro".into(), Value::Bool(o.show_tn_chapter_intro));
    settings.insert("use_chapter_labels".into(), Value::Bool(o.use_chapter_labels));
    settings.insert("use_prince".into(), Value::Bool(o.use_prince));
    settings.insert("use_section_visual_separator".into(), Value::Bool(o.use_section_visual_separator));
    settings.insert("use_two_column_layout_for_tn_notes".into(), Value::Bool(o.use_two_column_layout_for_tn_notes));
    settings.insert("use_two_column_layout_for_tq_notes".into(), Value::Bool(o.use_two_column_layout_for_tq_notes));
    if has_tw && req.tw_mode.includes_definitions() {
        settings.insert("tw_mode".into(), Value::String(req.tw_mode.code().into()));
    }
    let resources: Vec<Value> = req
        .resources
        .iter()
        .map(|r| {
            Value::Array(vec![
                Value::String(r.lang_code.clone()),
                Value::String(r.resource_type.clone()),
                Value::String(r.book.as_str().to_string()),
            ])
        })
        .collect();
    let mut inner = serde_json::Map::new();
    inner.insert("resources".into(), Value::Array(resources));
    inner.insert("settings".into(), Value::Object(settings));
    let mut outer = serde_json::Map::new();
    outer.insert("version".into(), Value::Number(KEY_SCHEMA_VERSION.into()));
    outer.insert("request".into(), Value::Object(inner));
    let canonical = canonical_json(&Value::Object(outer));
    let digest = Sha256::digest(canonical.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}_{}", slug, &hex[..HASH_HEX_LENGTH])
}
