//! The use cases. Generic over the ports, so the fault-injection tests substitute fakes or the real
//! adapters pointed at mock servers, and the shell wires the production adapters.
//!
//! `AsBuilt` additionally emulates two behaviours of DOC's process-wide state that are defects
//! worth reproducing exactly until the content team rules:
//! * a data-API failure is turned into an empty catalog and remembered for the cache TTL (180 s),
//!   so a transient failure is served as `[]` for three minutes;
//! * every book-list request appends DOC's six augmentation entries to the *cached* catalog list,
//!   so the language list changes after the first book-list request within a TTL window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use doc_contract::wire::{BookEntry, ChaptersInBooks, LanguageEntry, ResourceTypeEntry};
use doc_contract::{Finding, Mode};
use doc_lookup::booklist;
use doc_lookup::catalog::{AsBuilt, Query, RepoEntry};
use doc_lookup::locator::{self, RepoRef};
use doc_lookup::reader::Workspace;
use doc_lookup::restypes;
use doc_lookup::tables_gen::{BOOK_NAMES, GATEWAY_LANGUAGES};
use doc_lookup::{Abort, Outcome};
use doc_model::names::NameProvider;
use doc_ports::{Acquirer, Catalog, CatalogError, CatalogSnapshot, Deadline, RepoRequest};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Clone, Debug)]
pub struct AppConfig {
    /// The default mode of a request that does not choose one.
    pub default_mode: Mode,
    /// A request's whole budget (data API, clones, parsing). DOC's gunicorn timeout is 120 s; the
    /// work continues in the background past a request's deadline, so a generous value costs little.
    pub request_deadline: Duration,
    /// How long as_built remembers a data-API failure (DOC: the 180 s TTL cache).
    pub as_built_failure_memory: Duration,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig { default_mode: Mode::AsBuilt, request_deadline: Duration::from_secs(300), as_built_failure_memory: Duration::from_secs(180) }
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    /// The data API could not give an answer within the budget (fixed mode only; as_built answers `[]`).
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// DOC's own code would raise an unhandled exception (a 500).
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Default)]
struct AsBuiltState {
    /// Query -> until when a failure is remembered.
    failed_until: HashMap<Query, Instant>,
    /// The augmentation DOC appended to the cached primary list: (snapshot id, extras).
    polluted: Option<(u64, Vec<RepoEntry>)>,
}

pub struct App<C: Catalog, A: Acquirer> {
    catalog: C,
    acquirer: A,
    names: Arc<dyn NameProvider + Send + Sync>,
    cfg: AppConfig,
    state: Mutex<AsBuiltState>,
}

/// A catalog as a use case sees it.
struct Repos {
    repos: Vec<RepoEntry>,
    snapshot_id: u64,
    findings: Vec<Finding>,
}

impl<C: Catalog, A: Acquirer> App<C, A> {
    pub fn new(catalog: C, acquirer: A, names: Arc<dyn NameProvider + Send + Sync>, cfg: AppConfig) -> App<C, A> {
        App { catalog, acquirer, names, cfg, state: Mutex::new(AsBuiltState::default()) }
    }

    pub fn config(&self) -> &AppConfig {
        &self.cfg
    }

    fn deadline(&self) -> Deadline {
        Deadline::after(self.cfg.request_deadline)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AsBuiltState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// The catalog rows for a query, or `None` where DOC would have answered with an empty list.
    async fn repos(&self, query: Query, mode: Mode, deadline: Deadline) -> Result<Option<Repos>, AppError> {
        if mode == Mode::AsBuilt {
            if let Some(until) = self.lock().failed_until.get(&query) {
                if Instant::now() < *until {
                    debug!(?query, "as_built: serving the remembered data-API failure as an empty catalog");
                    return Ok(None);
                }
            }
        }
        let snapshot: Arc<CatalogSnapshot> = match self.catalog.snapshot(query, deadline).await {
            Ok(s) => s,
            Err(e) => {
                return match mode {
                    Mode::Fixed => Err(AppError::Catalog(e)),
                    Mode::AsBuilt => {
                        warn!(?query, error = %e, "as_built: the data API failed; answering with an empty catalog and remembering it, as DOC does");
                        self.lock().failed_until.insert(query, Instant::now() + self.cfg.as_built_failure_memory);
                        Ok(None)
                    }
                }
            }
        };
        // Normalising a 5 MB catalog (parse, index, clone 20k rows) is CPU work of tens of milliseconds: it
        // runs on the blocking pool so the runtime threads that answer `/health/status` stay responsive.
        let work = move || -> Result<Option<Repos>, AppError> {
            match mode {
                Mode::AsBuilt => match snapshot.as_built() {
                    AsBuilt::Repos(r) => Ok(Some(Repos { repos: r.clone(), snapshot_id: snapshot.id, findings: Vec::new() })),
                    AsBuilt::EmptyCached(reason) => {
                        warn!(?query, reason = %reason, "as_built: the catalog answer is discarded, as DOC does");
                        Ok(None)
                    }
                    AsBuilt::Crash(reason) => Err(AppError::Internal(reason.clone())),
                },
                Mode::Fixed => match snapshot.fixed() {
                    Ok((r, findings)) => {
                        let mut findings = findings.clone();
                        findings.extend(snapshot.stale_finding());
                        Ok(Some(Repos { repos: r.clone(), snapshot_id: snapshot.id, findings }))
                    }
                    Err(m) => Err(AppError::Catalog(CatalogError::Malformed { attempts: 1, detail: m.to_string() })),
                },
            }
        };
        match tokio::task::spawn_blocking(work).await {
            Ok(r) => r,
            Err(e) => Err(AppError::Internal(format!("catalog normalisation task failed: {e}"))),
        }
    }

    pub fn chapters_in_books(&self) -> ChaptersInBooks {
        ChaptersInBooks(
            doc_model::tables::BOOKS.iter().map(|(code, _, n)| ((*code).to_string(), (1..=*n).collect())).collect(),
        )
    }

    pub async fn languages(&self, mode: Mode) -> Result<Outcome<Vec<LanguageEntry>>, AppError> {
        let deadline = self.deadline();
        let (primary, non_primary) = tokio::join!(self.repos(Query::Primary, mode, deadline), self.repos(Query::NonPrimary, mode, deadline));
        let (primary, non_primary) = (primary?, non_primary?);
        let mut findings = Vec::new();
        let mut p: Vec<RepoEntry> = Vec::new();
        let mut np: Vec<RepoEntry> = Vec::new();
        if let Some(r) = primary {
            findings.extend(r.findings.clone());
            p = r.repos;
            if mode == Mode::AsBuilt {
                // DOC's cached list may carry the augmentation of an earlier book-list request
                if let Some((id, extras)) = &self.lock().polluted {
                    if *id == r.snapshot_id {
                        p.extend(extras.iter().cloned());
                    }
                }
            }
        }
        if let Some(r) = non_primary {
            findings.extend(r.findings);
            np = r.repos;
        }
        let value = doc_lookup::languages::lang_codes_and_names(&p, &np, GATEWAY_LANGUAGES, mode);
        Ok(Outcome { value, findings })
    }

    /// The repositories of `lang` and the primary catalog's augmentation, as DOC's list path sees them.
    async fn locate(&self, lang: &str, mode: Mode, deadline: Deadline, mutate_cache: bool, filter_excluded: bool) -> Result<Option<(Vec<RepoRef>, Vec<Finding>)>, AppError> {
        let Some(r) = self.repos(Query::Primary, mode, deadline).await? else {
            return Ok(None);
        };
        let mut findings = r.findings;
        let mut repos = r.repos;
        if filter_excluded {
            repos = locator::filter_excluded(repos);
        }
        match mode {
            Mode::AsBuilt if mutate_cache => {
                // `add_data_not_supplied_by_data_api(data.git_repo)` appends to the cached list
                let mut st = self.lock();
                let extras = match &st.polluted {
                    Some((id, extras)) if *id == r.snapshot_id => extras.clone(),
                    _ => {
                        let extras = locator::augmentation(&repos);
                        st.polluted = Some((r.snapshot_id, extras.clone()));
                        extras
                    }
                };
                repos.extend(extras);
            }
            Mode::AsBuilt => {
                // resource_types works on a filtered copy: the cached list is not touched, but a
                // pollution an earlier book-list request left is visible
                let st = self.lock();
                if let Some((id, extras)) = &st.polluted {
                    if *id == r.snapshot_id {
                        repos.extend(extras.iter().cloned());
                    }
                }
                drop(st);
                let more = locator::augmentation(&repos);
                repos.extend(more);
            }
            Mode::Fixed => repos = locator::augmented(&repos),
        }
        let (refs, f) = locator::repos_to_clone(lang, &repos, mode);
        findings.extend(f);
        Ok(Some((refs, findings)))
    }

    /// P5: the repositories of `lang` as the catalog lists them (fixed mode), not yet acquired.
    /// `Ok(None)` is not reachable in fixed mode; kept for the shared `locate` signature.
    pub async fn locate_for_job(&self, lang: &str, deadline: Deadline) -> Result<Option<(Vec<RepoRef>, Vec<Finding>)>, AppError> {
        self.locate(lang, Mode::Fixed, deadline, false, false).await
    }

    /// P5: the acquirer, so a job can fetch exactly the repositories it needs and see each failure.
    pub fn acquirer(&self) -> &A {
        &self.acquirer
    }

    async fn workspace(&self, refs: &[RepoRef], deadline: Deadline, findings: &mut Vec<Finding>) -> Workspace {
        let requests: Vec<RepoRequest> = refs
            .iter()
            .filter(|r| r.resource_type != "rg")
            .map(|r| RepoRequest { url: r.url.clone(), segment: r.segment.clone() })
            .collect();
        let acquisition = self.acquirer.acquire(&requests, deadline).await;
        let report = acquisition.findings();
        for f in &report {
            warn!(repo = ?f.repo, code = ?f.code, detail = %f.detail, "acquisition finding");
        }
        findings.extend(report);
        // keep the catalog order, as DOC iterates `repo_clone_list`
        let mut entries = Vec::with_capacity(acquisition.acquired.len());
        for r in refs {
            if let Some(a) = acquisition.acquired.iter().find(|a| a.url == r.url) {
                entries.push((r.segment.clone(), a.reader.clone()));
            }
        }
        Workspace::new(entries)
    }

    pub async fn book_codes_for_lang(&self, lang: &str, usfm_only: bool, mode: Mode) -> Result<Outcome<Vec<BookEntry>>, AppError> {
        let deadline = self.deadline();
        let started = Instant::now();
        let Some((refs, mut findings)) = self.locate(lang, mode, deadline, true, false).await? else {
            return Ok(Outcome::default());
        };
        let ws = self.workspace(&refs, deadline, &mut findings).await;
        let (lang_s, names) = (lang.to_string(), self.names.clone());
        let joined = tokio::task::spawn_blocking(move || booklist::book_codes_for_lang(&lang_s, &refs, &ws, usfm_only, mode, names.as_ref())).await;
        let result = joined.map_err(|e| AppError::Internal(format!("book list task: {e}")))?;
        let outcome = match result {
            Ok(mut o) => {
                findings.append(&mut o.findings);
                Outcome { value: o.value, findings }
            }
            Err(Abort(reason)) => {
                warn!(lang, reason = %reason, "as_built: DOC swallows this into an empty list");
                Outcome { value: Vec::new(), findings }
            }
        };
        info!(lang, usfm_only, ?mode, books = outcome.value.len(), findings = outcome.findings.len(), elapsed_ms = started.elapsed().as_millis() as u64, "book list");
        Ok(outcome)
    }

    pub async fn shared_book_codes(&self, lang0: &str, lang1: &str, mode: Mode) -> Result<Outcome<Vec<BookEntry>>, AppError> {
        let (a, b) = tokio::join!(self.book_codes_for_lang(lang0, false, mode), self.book_codes_for_lang(lang1, false, mode));
        let (a, b) = (a?, b?);
        let value = a.value.into_iter().filter(|BookEntry(code, _)| b.value.iter().any(|BookEntry(c, _)| c == code)).collect();
        let mut findings = a.findings;
        findings.extend(b.findings);
        Ok(Outcome { value, findings })
    }

    pub async fn resource_types(&self, lang: &str, book_codes: &str, mode: Mode) -> Result<Outcome<Vec<ResourceTypeEntry>>, AppError> {
        let deadline = self.deadline();
        let codes = restypes::split_book_codes(book_codes);
        let _ = BOOK_NAMES;
        let Some((refs, mut findings)) = self.locate(lang, mode, deadline, false, true).await? else {
            return Ok(Outcome::default());
        };
        let ws = self.workspace(&refs, deadline, &mut findings).await;
        let (refs_c, mode_c) = (refs.clone(), mode);
        let joined = tokio::task::spawn_blocking(move || restypes::get_resource_types(&refs_c, &codes, &ws, mode_c)).await;
        let result = joined.map_err(|e| AppError::Internal(format!("resource types task: {e}")))?;
        match result {
            Ok(mut o) => {
                findings.append(&mut o.findings);
                Ok(Outcome { value: o.value, findings })
            }
            Err(Abort(reason)) => {
                warn!(lang, reason = %reason, "as_built: DOC swallows this into an empty list");
                Ok(Outcome { value: Vec::new(), findings })
            }
        }
    }
}
