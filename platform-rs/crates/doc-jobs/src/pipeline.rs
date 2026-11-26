//! The document job: DOC's `generate_docx_document` as stages, each with its own deadline, each setting
//! the legacy progress string first. Stage failures are classified: what can heal (catalog or clone
//! outage, a hung or crashed docx worker, a full disk) is transient; what cannot (an unknown resource,
//! input the core rejects) is permanent.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use doc_app::App;
use doc_lookup::booklist;
use doc_lookup::locator::{self, RepoRef};
use doc_lookup::pytext::remove_xml_incompatible_chars;
use doc_lookup::reader::{RepoReader, Workspace};
use doc_lookup::tables_gen::RESOURCE_TYPE_CODES_AND_NAMES;
use doc_md::links::{ReaderCompat, Req};
use doc_md::readers::{self, Dto, ReaderSettings};
use doc_model::tree::{Tree, TreeData};
use doc_ports::{Acquirer, Catalog, Deadline, Freshness, RepoRequest};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::core_stage::{run_core_child, run_core_with_readers, CoreConfig, CoreInput, CoreItem, CoreMode, CoreResult};
use crate::email::{self, Delivery, EmailConfig, Mail};
use crate::html::{self, Templates};
use crate::limits::Limits;
use crate::metrics;
use crate::model::{JobError, Progress};
use crate::output::{place_atomic, write_atomic, Dirs};
use crate::pdf::{run_pdf_worker, PdfConfig, PdfJob};
use crate::pool::DocxBackend;
use crate::runner::{Handler, JobCtx};
use crate::seam::DocxJob;
use crate::store::JobStore;
use crate::titles::{title_page_strings, TitleDto};

pub const KIND_DOCX: &str = "docx";
pub const KIND_HTML: &str = "html";
const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const PDF_MIME: &str = "application/pdf";

#[derive(Clone, Debug)]
pub struct StageDeadlines {
    pub locate: Duration,
    pub provision: Duration,
    pub parse: Duration,
    pub assemble: Duration,
}

impl StageDeadlines {
    /// Generous: a request should end with a document, so a slow stage is waited for, not abandoned.
    pub fn production() -> StageDeadlines {
        StageDeadlines { locate: Duration::from_secs(300), provision: Duration::from_secs(900), parse: Duration::from_secs(300), assemble: Duration::from_secs(300) }
    }
}

#[derive(Clone, Debug)]
pub struct PipelineConfig {
    pub dirs: Dirs,
    /// `ASSET_CACHING_PERIOD`: a document younger than this is served again, not rebuilt.
    pub fresh_for: Duration,
    pub stages: StageDeadlines,
    /// The core's output structure for docx: `proposed_look` (what the template-free docx generator reads) or
    /// `as_built` (DOC's own HTML shapes, used to compare with the oracle). HTML jobs always use `as_built`.
    pub structure: String,
    /// Write the interleaver's parts next to the findings (`<key>.parts.json`); for comparison and debugging.
    pub dump_parts: bool,
    pub core: CoreConfig,
    pub templates: Templates,
    pub limits: Limits,
    pub email: Option<EmailConfig>,
    pub email_body_template: Option<PathBuf>,
    /// The PDF engine behind the Python seam; `None` = `POST /documents` with `generate_pdf` answers a typed 501.
    pub pdf: Option<PdfConfig>,
}

/// A validated request, as the job stores it: the client's JSON, plus what validation derived.
pub struct Prepared {
    pub key: String,
    pub raw: Value,
    pub valid: doc_request::request::ValidRequest,
}

/// Validate a client request and compute its cache key (the same function as DOC's, pinned by the golden
/// hashes in `tests/unit/test_document_generator.py`). The error is the list of violations for a 422.
pub fn prepare(raw: &Value) -> Result<Prepared, Vec<doc_request::request::Violation>> {
    let valid = doc_request::request::validate(raw)?;
    let key = doc_request::request::request_key(&valid);
    Ok(Prepared { key, raw: raw.clone(), valid })
}

pub struct DocPipeline<C: Catalog, A: Acquirer, S: JobStore> {
    pub app: Arc<App<C, A>>,
    pub cfg: PipelineConfig,
    pub docx: Arc<DocxBackend>,
    /// The Python seam service (container deployment): its configuration and a shared HTTP client.
    pub remote: Option<(crate::remote::RemoteConfig, reqwest::Client)>,
    pub store: Arc<S>,
}

fn resource_name(code: &str) -> String {
    RESOURCE_TYPE_CODES_AND_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| (*n).to_string()).unwrap_or_else(|| code.to_string())
}

fn is_text(path: &str) -> bool {
    let l = path.to_lowercase();
    l.ends_with(".md") || l.ends_with(".txt") || l.ends_with(".yaml") || l.ends_with(".yml")
}

/// DOC's `filter_usfm_files`: the first file's suffix decides how the book code is looked for: in the stem of a `.usfm` file, in the
/// whole path (which includes the repository directory's name) of a `.txt` file.
fn filter_usfm_files(files: &[String], book: &str) -> Vec<String> {
    let code = book.to_lowercase();
    let Some(first) = files.first() else { return Vec::new() };
    if first.ends_with(".usfm") {
        files.iter().filter(|f| doc_model::tree::path_stem(f).to_lowercase().contains(&code)).cloned().collect()
    } else if first.ends_with(".txt") {
        files.iter().filter(|f| f.to_lowercase().contains(&code)).cloned().collect()
    } else {
        Vec::new()
    }
}

/// What a USFM resource turned out to be on disk.
enum UsfmFound {
    /// No USFM file names the book: DOC builds an empty book and goes on.
    None,
    /// One file holds the book.
    Single(String),
    /// A Writer repository: chapter directories of chunk files; the core stitches them into one USFM text.
    Writer(Vec<(String, Vec<u8>)>),
}

/// DOC's `usfm_asset_file` over one repository: the `.usfm` files one level below its directory, else its `.txt` files, else the `.txt`
/// files two levels below it (`title.txt` excluded); the files of the book are those named for the book code; more than one file is a Writer
/// repository whose files (the chunk files and `front/title.txt`) are handed to the stitcher.
fn find_usfm(ws: &Workspace, reader: &dyn RepoReader, segment: &str, book: &str) -> Result<UsfmFound, JobError> {
    let found = booklist::find_usfm_files(ws, segment);
    let chosen = filter_usfm_files(&found, book);
    match chosen.len() {
        0 => Ok(UsfmFound::None),
        1 => Ok(UsfmFound::Single(chosen[0].clone())),
        _ => {
            let mut rels: Vec<String> = reader.files().into_iter().filter(|f| (f.ends_with(".usfm") || f.ends_with(".txt")) && f.matches('/').count() <= 1).collect();
            rels.sort();
            let mut files = Vec::with_capacity(rels.len());
            for rel in rels {
                match reader.read_text(&rel) {
                    Ok(Some(t)) => files.push((rel, t.into_bytes())),
                    Ok(None) => {}
                    Err(e) => return Err(JobError::permanent("usfm_unreadable", format!("{segment}/{rel}: {e}"))),
                }
            }
            Ok(UsfmFound::Writer(files))
        }
    }
}

struct Located {
    /// The requests that resolved to a repository, as `(index into the request list, repository)`, in request order.
    resolved: Vec<(usize, RepoRef)>,
    /// Requests DOC's lookup finds nothing for: they are left out of the document, each with a finding (DOC drops them silently).
    unresolved: Vec<usize>,
    unique: Vec<RepoRef>,
    findings: Vec<Value>,
}

/// What the core needs to know about the repository of one resolved resource request.
#[derive(Clone, Debug)]
pub struct RepoInfo {
    /// Index of the request in the validated request list.
    pub index: usize,
    pub url: String,
    pub segment: String,
    pub resource_type: String,
    pub lang_name: String,
    pub localized_lang_name: String,
    pub direction: String,
}

/// What the parse stage returns: the typed books, the findings it produced, and DOC's title page strings.
pub struct Parsed {
    pub books: Value,
    pub findings: Vec<Value>,
    pub title1: String,
    pub title2: String,
}

struct Provisioned {
    readers: HashMap<String, Arc<dyn RepoReader>>,
    roots: HashMap<String, PathBuf>,
    reused: u64,
}

/// Resource files to books: ULB through the USFM model, TN (and the other markdown resources) through
/// the readers. Pure CPU; runs in the core child (or on the blocking pool in inline mode).
/// `html_shapes`: chapters in DOC's own HTML shape (`div.chapter`, `span.verse`: what DOC's CSS styles) for the
/// HTML output; the docx path takes the model's semantic markup.
///
/// A book a repository has no file for is an empty book and a finding (DOC builds the document and goes on); only a request whose every
/// resource is an absent USFM book is an error, because there is nothing to build a document from.
pub fn parse(p: &Prepared, infos: &[RepoInfo], readers_by_segment: &HashMap<String, Arc<dyn RepoReader>>, generate_docx: bool, html_shapes: bool) -> Result<Parsed, JobError> {
    let mut usfm_books: Vec<Value> = Vec::new();
    let mut dtos: Vec<Dto> = Vec::new();
    let mut tree = Tree::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut findings: Vec<Value> = Vec::new();
    let mut usfm_names: Vec<(String, String, String)> = Vec::new();
    let mut empty_usfm = 0usize;
    let ws = Workspace::new(readers_by_segment.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    // A repository can serve several requests (the same language and type for two books): read the files of every book asked of it, once.
    let mut books_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for repo in infos {
        let req = &p.valid.resources[repo.index];
        if req.kind != doc_request::request::ResourceKind::Usfm {
            books_of.entry(repo.segment.as_str()).or_default().push(req.book.as_str());
        }
    }
    let check_usfm = std::env::var("DOC_CHECK_USFM").map(|v| v != "false" && v != "0").unwrap_or(true);
    for repo in infos {
        let req = &p.valid.resources[repo.index];
        let book = req.book.as_str();
        let Some(reader) = readers_by_segment.get(&repo.segment) else {
            return Err(JobError::transient("repo_unavailable", format!("{} was not provisioned", repo.url)));
        };
        let rt_name = resource_name(&req.resource_type);
        if req.kind == doc_request::request::ResourceKind::Usfm {
            let ref_ = RepoRef {
                url: repo.url.clone(),
                segment: repo.segment.clone(),
                resource_type: req.resource_type.clone(),
                components: Vec::new(),
                language: doc_lookup::catalog::Language { english_name: repo.lang_name.clone(), ietf_code: req.lang_code.clone(), national_name: repo.localized_lang_name.clone(), direction: repo.direction.clone() },
            };
            let source = match find_usfm(&ws, reader.as_ref(), &repo.segment, book)? {
                UsfmFound::None => {
                    findings.push(json!({"code": "usfm_not_found", "severity": "warning", "lang": req.lang_code, "book": book, "repo": repo.url, "message": format!("{} has no USFM file for {book}; the book is empty in the document", repo.url)}));
                    empty_usfm += 1;
                    String::new()
                }
                UsfmFound::Single(path) => match ws.read(&path) {
                    Ok(Some(s)) => s,
                    Ok(None) => return Err(JobError::transient("repo_unavailable", format!("{path} vanished from {}", repo.url))),
                    Err(e) => return Err(JobError::permanent("usfm_unreadable", format!("{path}: {e}"))),
                },
                UsfmFound::Writer(files) => doc_canon::stitch::stitch_writer_book(&files, book, ""),
            };
            let source = remove_xml_incompatible_chars(&source);
            // DOC repairs the USFM of resources it knows to be defective, chapter by chapter, before converting it (`CHECK_USFM` and
            // `CHECK_ALL_BOOKS_FOR_LANGUAGE`, both on in DOC's `.env`); chapter labels come out first unless the request asks for them.
            let source = if check_usfm && !source.is_empty() && doc_canon::usfm_fixes::needs_fixing(&req.lang_code, &req.resource_type, book, true) {
                let labelled = if p.valid.options.use_chapter_labels { source.clone() } else { doc_canon::usfm_fixes::remove_chapter_labels(&source) };
                let repaired = doc_canon::usfm_fixes::fix_book(&labelled);
                if repaired != source {
                    findings.push(json!({"code": "usfm_repaired", "severity": "info", "lang": req.lang_code, "book": book, "message": "this resource is known to have USFM defects; DOC's repair rules were applied before parsing"}));
                }
                repaired
            } else {
                source
            };
            let model = doc_canon::usfm::parse_book(&source);
            let opts = doc_canon::look::UlbOptions::default();
            let chapters: Vec<Value> = model
                .chapters
                .iter()
                .map(|c| {
                    let verses: Vec<Value> = c.verses.iter().map(|v| json!([v.label_doc, doc_canon::usfm::verse_html(&v.text_doc)])).collect();
                    let content = if html_shapes { doc_canon::usfm::chapter_html_plain(c.number, &c.verses, c.has_p) } else { doc_canon::look::ulb_chapter_html(c, &opts) };
                    json!([c.number, {"content": content, "verses": verses}])
                })
                .collect();
            // DOC's name for the book in the title: its own markers, the manifest, `front/title.txt`; else the English name of the book code
            let (localized, name_findings) = booklist::usfm_book_name(&ws, &ref_, &req.lang_code, book, &source);
            for f in name_findings {
                findings.push(serde_json::to_value(f).unwrap_or(Value::Null));
            }
            let national = if localized.is_empty() { req.book.english_name().to_string() } else { localized };
            usfm_names.push((req.lang_code.clone(), book.to_lowercase(), national.clone()));
            usfm_books.push(json!({
                "lang_code": req.lang_code, "lang_name": repo.lang_name, "book_code": book,
                "national_book_name": national, "resource_type_name": rt_name, "chapters": chapters
            }));
        } else {
            if seen.insert(repo.segment.clone()) {
                let files = reader.files();
                let mut texts: HashMap<String, Option<String>> = HashMap::new();
                let wanted_books = books_of.get(repo.segment.as_str()).map(Vec::as_slice).unwrap_or(&[]);
                for f in files.iter().filter(|f| is_text(f)) {
                    if wanted_books.iter().any(|b| wanted_text(f, b)) {
                        texts.insert(f.clone(), reader.read_text(f).ok().flatten());
                    }
                }
                tree.add_dir(&repo.segment, &TreeData { files, texts });
            }
            dtos.push(Dto {
                lang_code: req.lang_code.clone(),
                lang_name: repo.lang_name.clone(),
                resource_type: req.resource_type.clone(),
                resource_type_name: rt_name,
                book_code: book.to_string(),
                lang_direction: Default::default(),
                dir: repo.segment.clone(),
            });
        }
    }
    // Nothing to build a document from: every resource that resolved is a USFM book its repository does not have. Not transient.
    if dtos.is_empty() && !usfm_books.is_empty() && empty_usfm == usfm_books.len() {
        return Err(JobError::permanent("resource_unavailable", "none of the requested resources has content for the requested books".to_string()));
    }
    let reqs: Vec<Req> = p.valid.resources.iter().map(|r| Req { lang_code: r.lang_code.clone(), resource_type: r.resource_type.clone(), book_code: r.book.as_str().to_string() }).collect();
    let settings = ReaderSettings {
        layout_for_print: p.valid.options.layout_for_print,
        generate_docx,
        show_tn_book_intro: p.valid.options.show_tn_book_intro,
        compat: ReaderCompat::fixed(),
        xref_links: false,
    };
    let rb = readers::read_books(&dtos, &reqs, &tree, &settings).map_err(|e| JobError::permanent("resource_unreadable", format!("{e:?}")))?;
    let titles: Vec<TitleDto> = infos
        .iter()
        .map(|i| {
            let r = &p.valid.resources[i.index];
            TitleDto { lang_code: r.lang_code.clone(), lang_name: i.lang_name.clone(), localized_lang_name: i.localized_lang_name.clone(), resource_type_name: resource_name(&r.resource_type), book_code: r.book.as_str().to_string() }
        })
        .collect();
    let (title1, title2) = title_page_strings(&titles, &usfm_names);
    Ok(Parsed { books: json!({"tn": rb.tn, "tnc": rb.tnc, "tq": rb.tq, "tw": rb.tw, "bc": rb.bc, "usfm": usfm_books}), findings, title1, title2 })
}

pub fn assemble(p: &Prepared, books: Value, structure: &str, unresolved: &[Value], parse_findings: Vec<Value>) -> Result<(Value, Value), JobError> {
    let mut request = p.raw.clone();
    if let Some(o) = request.as_object_mut() {
        o.entry("tw_mode").or_insert(json!("link"));
    }
    let input = json!({"request": request, "books": books, "bug_compat": "fixed", "structure": structure, "unresolved": unresolved}).to_string();
    let out = doc_canon::run_json(&input);
    let v: Value = serde_json::from_str(&out).map_err(|e| JobError::permanent("core_output", e.to_string()))?;
    if v["ok"] != json!(true) {
        return Err(JobError::permanent("core_rejected", out.chars().take(400).collect::<String>()));
    }
    let mut findings = v["findings"].clone();
    if let Some(list) = findings.as_array_mut() {
        list.extend(parse_findings);
    }
    Ok((v["parts"].clone(), findings))
}

/// Which text files of a markdown resource repository the readers may need for `book`: the files at the top level,
/// the book's own directory (`jud`, or Bible Commentary's `66-jud`), the shared article directories of the
/// commentary and Translation Words repositories (`articles/`, `bible/`), and nothing else (a Translation Notes
/// repository has a directory per book, and reading all of them would read the whole Bible).
fn wanted_text(path: &str, book: &str) -> bool {
    let Some((first, _)) = path.split_once('/') else { return true };
    let first_l = first.to_ascii_lowercase();
    let book_l = book.to_ascii_lowercase();
    let numbered = first_l.split_once('-').is_some_and(|(n, rest)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && rest == book_l);
    first_l == book_l || numbered || first_l == "articles" || first_l == "bible"
}

impl<C: Catalog, A: Acquirer, S: JobStore> DocPipeline<C, A, S> {
    /// One stage: its own deadline, one structured log line (request id, task, attempt, stage, duration, result)
    /// and a duration observation for the metrics.
    async fn stage<T>(&self, ctx: &JobCtx, key: &str, name: &'static str, limit: Duration, fut: impl std::future::Future<Output = Result<T, JobError>>) -> Result<T, JobError> {
        let t = Instant::now();
        let r = match tokio::time::timeout(limit, fut).await {
            Ok(r) => r,
            Err(_) => Err(JobError::transient("stage_timeout", format!("stage '{name}' did not finish in {} s", limit.as_secs()))),
        };
        let ms = t.elapsed().as_millis() as u64;
        match &r {
            Ok(_) => info!(request_id = %ctx.request_id, task_id = %ctx.task_id, attempt = ctx.attempt, key, stage = name, ms, ok = true, "stage finished"),
            Err(e) => warn!(request_id = %ctx.request_id, task_id = %ctx.task_id, attempt = ctx.attempt, key, stage = name, ms, ok = false, error = %e, "stage failed"),
        }
        metrics::observe_stage(&*self.store, name, t.elapsed().as_secs_f64()).await;
        r
    }

    /// DOC's `resource_lookup_dto` for every request, over the primary catalog plus DOC's augmentation. A request the lookup finds nothing for
    /// is left out of the document with a finding, as DOC leaves it out (silently); a request that resolves to nothing at all still ends with a
    /// document.
    async fn locate(&self, p: &Prepared) -> Result<Located, JobError> {
        let deadline = Deadline::after(self.cfg.stages.locate);
        let rows = self.app.catalog_rows(deadline).await.map_err(|e| JobError::transient("catalog_unavailable", e.to_string()))?;
        let mut resolved: Vec<(usize, RepoRef)> = Vec::new();
        let mut unresolved: Vec<usize> = Vec::new();
        let mut unique: Vec<RepoRef> = Vec::new();
        let mut findings: Vec<Value> = Vec::new();
        for (i, r) in p.valid.resources.iter().enumerate() {
            match locator::resource_lookup(&r.lang_code, &r.resource_type, r.book.as_str(), &rows) {
                Some(found) => {
                    if !unique.iter().any(|u| u.url == found.url) {
                        unique.push(found.clone());
                    }
                    resolved.push((i, found));
                }
                None => {
                    unresolved.push(i);
                    findings.push(json!({"code": "resource_not_in_catalog", "severity": "warning", "lang": r.lang_code, "book": r.book.as_str(), "message": format!("{} has no repository in the catalog for {}; the document is built without it (DOC leaves it out too)", r.resource_type, r.lang_code)}));
                }
            }
        }
        Ok(Located { resolved, unresolved, unique, findings })
    }

    async fn provision(&self, located: &Located) -> Result<Provisioned, JobError> {
        let requests: Vec<RepoRequest> = located.unique.iter().map(|r| RepoRequest { url: r.url.clone(), segment: r.segment.clone() }).collect();
        let acq = self.app.acquirer().acquire(&requests, Deadline::after(self.cfg.stages.provision)).await;
        if let Some(f) = acq.failed.first() {
            return Err(JobError::transient("repo_unavailable", format!("{}: {:?} after {} attempt(s): {}", f.url, f.kind, f.attempts, f.message)));
        }
        let mut out = Provisioned { readers: HashMap::new(), roots: HashMap::new(), reused: 0 };
        for a in acq.acquired {
            if a.freshness == Freshness::Cached {
                out.reused += 1;
            }
            if let Some(r) = &a.root {
                out.roots.insert(a.segment.clone(), r.clone());
            }
            out.readers.insert(a.segment.clone(), a.reader.clone());
        }
        Ok(out)
    }

    async fn append_findings(&self, key: &str, extra: Vec<Value>) {
        if extra.is_empty() {
            return;
        }
        let path = self.cfg.dirs.findings_path(key);
        let mut all: Vec<Value> = tokio::fs::read(&path).await.ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        all.extend(extra);
        if let Ok(bytes) = serde_json::to_vec_pretty(&all) {
            let _ = write_atomic(&path, &bytes).await;
        }
    }

    /// The email step for a docx request: idempotent per task (see [`crate::email`]).
    async fn send_email(&self, ctx: &JobCtx, p: &Prepared, doc_path: &Path, mime: &'static str) -> Result<(), JobError> {
        let (Some(cfg), Some(to)) = (self.cfg.email.as_ref(), p.valid.email_address.as_deref()) else { return Ok(()) };
        if !cfg.enabled {
            metrics::incr(&*self.store, "doc_email_total", &[("outcome", "skipped")]).await;
            return Ok(());
        }
        ctx.progress(Progress::SendingEmail).await;
        let body = email::email_body(self.cfg.email_body_template.as_deref(), &p.key);
        let mail = Mail { to, message_id: format!("{}.{}", ctx.task_id, p.key), body, attachment: Some((doc_path, mime)) };
        let owner = format!("{}#a{}", ctx.task_id, ctx.attempt);
        let last = ctx.attempt >= ctx.max_attempts;
        let outcome = email::deliver(&*self.store, cfg, &ctx.task_id, &owner, &mail, last).await;
        let (label, finding): (&str, Option<(&str, String)>) = match &outcome {
            Delivery::Sent => ("sent", None),
            Delivery::AlreadySent => ("already_sent", None),
            Delivery::Skipped => ("skipped", None),
            Delivery::InDoubt => ("in_doubt", Some(("email_in_doubt", "an earlier attempt may have sent the email; it was not sent again".to_string()))),
            Delivery::Failed(m) => ("failed", Some(("email_not_sent", m.clone()))),
            Delivery::RetryLater(_) => ("retry", None),
        };
        if label != "retry" {
            metrics::incr(&*self.store, "doc_email_total", &[("outcome", label)]).await;
        }
        if let Some((code, message)) = finding {
            self.append_findings(&p.key, vec![json!({"code": code, "severity": "warning", "message": message})]).await;
        }
        match outcome {
            Delivery::RetryLater(m) => Err(JobError::transient("email_retry", m)),
            _ => Ok(()),
        }
    }

    async fn core(&self, ctx: &JobCtx, p: &Prepared, located: &Located, prov: &Provisioned, structure: &str, generate_docx: bool, scratch: &Path) -> Result<CoreResult, JobError> {
        let infos: Vec<RepoInfo> = located
            .resolved
            .iter()
            .map(|(i, r)| RepoInfo {
                index: *i,
                url: r.url.clone(),
                segment: r.segment.clone(),
                resource_type: r.resource_type.clone(),
                lang_name: r.language.english_name.clone(),
                localized_lang_name: r.language.national_name.clone(),
                direction: r.language.direction.clone(),
            })
            .collect();
        let unresolved: Vec<Value> = located
            .unresolved
            .iter()
            .map(|i| {
                let r = &p.valid.resources[*i];
                json!({"lang_code": r.lang_code, "resource_type": r.resource_type, "book_code": r.book.as_str()})
            })
            .collect();
        let all_on_disk = infos.iter().all(|i| prov.roots.contains_key(&i.segment));
        if self.cfg.core.mode == CoreMode::Child && all_on_disk {
            let items = infos
                .iter()
                .map(|i| CoreItem {
                    index: i.index,
                    url: i.url.clone(),
                    segment: i.segment.clone(),
                    resource_type: i.resource_type.clone(),
                    lang_name: i.lang_name.clone(),
                    localized_lang_name: i.localized_lang_name.clone(),
                    direction: i.direction.clone(),
                    dir: prov.roots.get(&i.segment).cloned().unwrap_or_default(),
                })
                .collect();
            let input = CoreInput { request: p.raw.clone(), items, unresolved, structure: structure.to_string(), generate_docx, html_shapes: !generate_docx };
            return run_core_child(&self.cfg.core, &input, scratch, Some(ctx)).await;
        }
        let (request, readers, structure) = (p.raw.clone(), prov.readers.clone(), structure.to_string());
        tokio::task::spawn_blocking(move || run_core_with_readers(&request, &infos, &unresolved, &readers, &structure, generate_docx, !generate_docx))
            .await
            .map_err(|e| JobError::transient("internal_panic", e.to_string()))?
    }
}

impl<C: Catalog, A: Acquirer, S: JobStore> DocPipeline<C, A, S> {
    /// Locate, provision, run the core, make the document (docx or HTML) and place it with its findings. Returns the
    /// findings this step adds to the core's (a finding that belongs to the HTML wrapper, for instance).
    async fn build_main(&self, ctx: &JobCtx, p: &Prepared, kind: &str, doc_path: &Path, scratch: &Path, started: Instant) -> Result<(), JobError> {
        let key = p.key.clone();
        let is_docx = kind == KIND_DOCX;
        let scratch_path: PathBuf = scratch.to_path_buf();
        ctx.progress(Progress::LocatingAssets).await;
        let located = self.stage(ctx, &key, "locate", self.cfg.stages.locate, self.locate(p)).await?;

        ctx.progress(Progress::ProvisioningAssetFiles).await;
        let prov = self.stage(ctx, &key, "provision", self.cfg.stages.provision, self.provision(&located)).await?;
        if prov.reused > 0 {
            let _ = self.store.metric_add(&[(metrics::counter("doc_cache_hits_total", &[("kind", "repository")]), prov.reused)]).await;
        }

        // The core (parse and assemble) is one unit that can run in a killable child; its two progress strings
        // are published around it so the legacy status order is kept.
        ctx.progress(Progress::ParsingAssetFiles).await;
        let structure = if is_docx { self.cfg.structure.clone() } else { "as_built".to_string() };
        let core_limit = self.cfg.stages.parse + self.cfg.stages.assemble;
        let core_out = self.stage(ctx, &key, "core", core_limit, self.core(ctx, p, &located, &prov, &structure, is_docx, &scratch_path)).await?;
        ctx.progress(Progress::AssemblingContent).await;
        let CoreResult { parts, mut findings, title1, title2 } = core_out;
        if let Some(list) = findings.as_array_mut() {
            list.extend(located.findings.iter().cloned());
        }
        let findings_bytes = serde_json::to_vec_pretty(&findings).map_err(|e| JobError::permanent("findings_encode", e.to_string()))?;
        let mut extra: Vec<Value> = Vec::new();

        if is_docx {
            ctx.progress(Progress::ConvertingToDocx).await;
            let first = &p.valid.resources[0];
            let out_scratch: PathBuf = scratch_path.join("out.docx");
            let job = DocxJob {
                version: crate::seam::DOCX_JOB_VERSION,
                job_id: ctx.task_id.clone(),
                request_id: ctx.request_id.clone(),
                request_key: key.clone(),
                output_path: out_scratch.clone(),
                book_name: first.book.english_name().to_string(),
                title1: title1.clone(),
                title2: title2.clone(),
                layout_for_print: p.valid.options.layout_for_print,
                parts,
            };
            let report = self.stage(ctx, &key, "docx", Duration::from_secs(3600), self.docx.run(&job, &scratch_path.join("docx-job.json"), Some(&ctx))).await?;
            if ctx.is_cancelled() {
                return Err(JobError::transient("docx_worker_cancelled", "cancelled before placement"));
            }
            if self.cfg.dump_parts {
                let bytes = serde_json::to_vec_pretty(&job.parts).map_err(|e| JobError::permanent("parts_encode", e.to_string()))?;
                write_atomic(&self.cfg.dirs.output.join(format!("{key}.parts.json")), &bytes).await?;
            }
            // The findings first, then the document: when the document exists, its findings do.
            write_atomic(&self.cfg.dirs.findings_path(&key), &findings_bytes).await?;
            place_atomic(&out_scratch, doc_path).await?;
            info!(request_id = %ctx.request_id, key = %key, task_id = %ctx.task_id, total_ms = started.elapsed().as_millis() as u64, docx_wall_ms = report.wall.as_millis() as u64, worker_pid = report.pid, "document placed");
            if !report.result.warnings.is_empty() {
                warn!(request_id = %ctx.request_id, key = %key, warnings = report.result.warnings.len(), "docx worker warnings");
            }
        } else {
            let timestring = chrono_like_now();
            if self.cfg.dump_parts {
                let bytes = serde_json::to_vec_pretty(&parts).map_err(|e| JobError::permanent("parts_encode", e.to_string()))?;
                write_atomic(&self.cfg.dirs.output.join(format!("{key}.parts.json")), &bytes).await?;
            }
            let mut content = html::compose_document(&parts);
            if p.valid.resources.iter().any(|r| r.kind == doc_request::request::ResourceKind::Usfm) {
                let (c, replaced) = html::check_content_for_issues(content);
                content = c;
                if replaced {
                    extra.push(json!({"code": "html_no_verses", "severity": "warning", "message": "no verses were found in the scripture text; DOC's notice replaces the content"}));
                }
            }
            let header = self.cfg.templates.header(p.valid.layout.code(), false, &html::escape_html(&title1), &html::escape_html(&title2), "", &timestring)?;
            let doc = html::enclose(&content, &header);
            let out_scratch = scratch_path.join("out.html");
            tokio::fs::write(&out_scratch, doc.as_bytes()).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", out_scratch.display())))?;
            write_atomic(&self.cfg.dirs.findings_path(&key), &findings_bytes).await?;
            place_atomic(&out_scratch, doc_path).await?;
            info!(request_id = %ctx.request_id, key = %key, task_id = %ctx.task_id, total_ms = started.elapsed().as_millis() as u64, bytes = doc.len(), "document placed");
        }
        self.append_findings(&key, extra).await;
        Ok(())
    }

    /// HTML to PDF through the Python seam (WeasyPrint or Prince); the PDF is placed under the request key.
    async fn convert_pdf(&self, ctx: &JobCtx, key: &str, html_path: &Path, scratch: &Path, use_prince: bool) -> Result<PathBuf, JobError> {
        let Some(cfg) = self.cfg.pdf.as_ref() else { return Err(JobError::permanent("pdf_not_configured", "no PDF engine is configured")) };
        ctx.progress(Progress::ConvertingToPdf).await;
        let out_scratch = scratch.join("out.pdf");
        let engine = if cfg.engine == "remote" { (if use_prince { "prince" } else { "weasyprint" }).to_string() } else { cfg.engine.clone() };
        let job = PdfJob { version: 1, job_id: ctx.task_id.clone(), request_id: ctx.request_id.clone(), input_html: html_path.to_path_buf(), output_path: out_scratch.clone(), engine };
        let report = if cfg.engine == "remote" {
            let Some(remote) = self.remote.as_ref() else { return Err(JobError::permanent("pdf_not_configured", "DOC_PDF_ENGINE=remote needs DOC_PYWORKER_URL")) };
            let (r, used, fell_back) = self.stage(ctx, key, "pdf", Duration::from_secs(3600), crate::remote::run_pdf_remote(&remote.1, &remote.0, &job, &scratch.join("pdf-job.json"), Some(ctx))).await?;
            if fell_back {
                let _ = self.store.metric_add(&[(metrics::counter("doc_pdf_engine_fallbacks_total", &[("engine", used.as_str())]), 1)]).await;
                self.append_findings(key, vec![json!({"code": "pdf_engine_fallback", "severity": "info", "message": format!("the PDF was made with {used} because the requested engine could not be used for this document")})]).await;
            }
            r
        } else {
            self.stage(ctx, key, "pdf", Duration::from_secs(3600), run_pdf_worker(cfg, &job, &scratch.join("pdf-job.json"), Some(ctx))).await?
        };
        let dest = self.cfg.dirs.pdf_path(key);
        place_atomic(&out_scratch, &dest).await?;
        info!(request_id = %ctx.request_id, key, bytes = report.bytes, pdf_wall_ms = report.wall.as_millis() as u64, "pdf placed");
        Ok(dest)
    }
}

impl<C: Catalog, A: Acquirer, S: JobStore> Handler for DocPipeline<C, A, S> {
    async fn run(&self, ctx: JobCtx) -> Result<String, JobError> {
        let started = Instant::now();
        let kind = ctx.kind.clone();
        ctx.progress(Progress::ReceivingRequest).await;
        let raw: Value = serde_json::from_str(&ctx.payload).map_err(|e| JobError::permanent("invalid_request", e.to_string()))?;
        let p = prepare(&raw).map_err(|v| JobError::permanent("invalid_request", v.iter().map(|x| x.message.clone()).collect::<Vec<_>>().join("; ")))?;
        let key = p.key.clone();
        let doc_path = self.cfg.dirs.path_for(&kind, &key);
        let is_docx = kind == KIND_DOCX;
        let want_pdf = !is_docx && p.valid.generate_pdf && self.cfg.pdf.is_some();
        let main_fresh = self.cfg.dirs.fresh(&kind, &key, self.cfg.fresh_for).await;
        let pdf_fresh = !want_pdf || self.cfg.dirs.fresh("pdf", &key, self.cfg.fresh_for).await;
        if main_fresh && pdf_fresh {
            info!(request_id = %ctx.request_id, task_id = %ctx.task_id, key = %key, kind = %kind, "cache hit: a fresh document exists");
            metrics::incr(&*self.store, "doc_cache_hits_total", &[("kind", "document")]).await;
            let attach = if is_docx { Some((doc_path.clone(), DOCX_MIME)) } else if want_pdf { Some((self.cfg.dirs.pdf_path(&key), PDF_MIME)) } else { None };
            if let Some((path, mime)) = attach {
                self.send_email(&ctx, &p, &path, mime).await?;
            }
            return Ok(key);
        }
        if let Some(free) = self.cfg.dirs.free_bytes() {
            if free < self.cfg.limits.min_free_bytes {
                return Err(JobError::transient("disk_space_low", format!("{} MB free on the output filesystem, {} MB required", free / 1_048_576, self.cfg.limits.min_free_bytes / 1_048_576)));
            }
        }
        tokio::fs::create_dir_all(&self.cfg.dirs.output).await.map_err(|e| JobError::transient("output_io", e.to_string()))?;
        let removed = self.cfg.dirs.remove_stale_attempts(&ctx.task_id, ctx.attempt);
        if removed > 0 {
            info!(request_id = %ctx.request_id, task_id = %ctx.task_id, removed, "removed scratch directories of earlier attempts");
        }
        let scratch = self.cfg.dirs.new_scratch(&ctx.task_id, ctx.attempt)?;
        if !main_fresh {
            self.build_main(&ctx, &p, &kind, &doc_path, &scratch.path, started).await?;
        }
        let mut attach: Option<(PathBuf, &'static str)> = if is_docx { Some((doc_path.clone(), DOCX_MIME)) } else { None };
        if want_pdf {
            let pdf = self.convert_pdf(&ctx, &key, &doc_path, &scratch.path, p.valid.options.use_prince).await?;
            attach = Some((pdf, PDF_MIME));
        }
        if let Some((path, mime)) = attach {
            self.send_email(&ctx, &p, &path, mime).await?;
        }
        Ok(key)
    }
}

/// `datetime.now().ctime()`: "Wed Nov 25 05:12:03 2026".
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m - 1) as usize];
    format!("{wd} {mon} {d:2} {:02}:{:02}:{:02} {y}", rem / 3600, rem % 3600 / 60, rem % 60)
}
