//! doc-ports: the seams between the application layer and the outside world.
//!
//! * [`Catalog`]: the BIEL data API. A call returns a [`CatalogSnapshot`] (the validated response,
//!   parsed lazily per mode) or a typed [`CatalogError`]; a failure is never a value.
//! * [`Acquirer`]: repository acquisition. A call returns an [`Acquisition`]: the repositories that
//!   are ready (with a reader) and the ones that are not, each with a reason. Nothing is dropped
//!   silently.
//! * [`Deadline`]: the per-request budget every call receives.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use doc_contract::{Finding, FindingCode, Severity};
use doc_lookup::catalog::{self, AsBuilt, Malformed, Query, RepoEntry};
use doc_lookup::reader::RepoReader;
use thiserror::Error;

/// An absolute point in time a request must finish by.
#[derive(Clone, Copy, Debug)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(d: Duration) -> Deadline {
        Deadline(Instant::now() + d)
    }
    pub fn at(i: Instant) -> Deadline {
        Deadline(i)
    }
    pub fn instant(self) -> Instant {
        self.0
    }
    pub fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }
    pub fn expired(self) -> bool {
        Instant::now() >= self.0
    }
}

// ---------------------------------------------------------------- catalog

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    /// Transport failure, timeout or a non-200 status, after all attempts.
    #[error("data API unavailable after {attempts} attempt(s): {last}")]
    Unavailable { attempts: u32, last: String },
    /// A 200 whose body is not a usable GraphQL answer, after all attempts.
    #[error("data API returned a malformed answer after {attempts} attempt(s): {detail}")]
    Malformed { attempts: u32, detail: String },
    /// The caller's deadline passed while a refresh was still running (it continues in the
    /// background, so the next request is served from the cache it fills).
    #[error("deadline exceeded waiting for the data API (last error: {last})")]
    DeadlineExceeded { last: String },
}

impl CatalogError {
    /// A caller may retry: every catalog failure is transient by construction (permanent shape
    /// errors are retried within the budget and reported as `Malformed`, which can also heal).
    pub fn retryable(&self) -> bool {
        true
    }
}

/// A validated catalog response. `body` is the raw JSON; the as-built and fixed parses are
/// computed on first use and kept (the body is ~5 MB, a parse about 50 ms).
#[derive(Debug)]
pub struct CatalogSnapshot {
    /// Increases with every refresh; the as-built mode keys its emulated cache pollution on it.
    pub id: u64,
    pub query: Query,
    pub body: Arc<Vec<u8>>,
    /// Served after a failed refresh (stale-if-error).
    pub stale: bool,
    /// Seconds since the data API answered.
    pub age_secs: u64,
    parses: Arc<Parses>,
}

#[derive(Debug, Default)]
struct Parses {
    as_built: OnceLock<AsBuilt>,
    fixed: OnceLock<FixedParse>,
}

/// The fixed parse: rows and findings, or why the structure was refused.
pub type FixedParse = Result<(Vec<RepoEntry>, Vec<Finding>), Malformed>;

impl CatalogSnapshot {
    pub fn new(id: u64, query: Query, body: Arc<Vec<u8>>, stale: bool, age_secs: u64) -> CatalogSnapshot {
        CatalogSnapshot { id, query, body, stale, age_secs, parses: Arc::new(Parses::default()) }
    }

    /// The same catalog with another age/stale label; the parses are shared, not recomputed.
    pub fn relabelled(&self, stale: bool, age_secs: u64) -> CatalogSnapshot {
        CatalogSnapshot { id: self.id, query: self.query, body: self.body.clone(), stale, age_secs, parses: self.parses.clone() }
    }

    pub fn as_built(&self) -> &AsBuilt {
        self.parses.as_built.get_or_init(|| catalog::parse_as_built(&self.body, self.query))
    }

    pub fn fixed(&self) -> &FixedParse {
        self.parses.fixed.get_or_init(|| catalog::parse_fixed(&self.body, self.query))
    }

    /// The stale marker as a finding.
    pub fn stale_finding(&self) -> Option<Finding> {
        self.stale.then(|| {
            Finding::new(
                FindingCode::CatalogServedStale,
                Severity::Warning,
                format!("the data API could not be reached; the catalog from {} s ago was served", self.age_secs),
            )
        })
    }
}

pub trait Catalog: Send + Sync + 'static {
    fn snapshot(&self, query: Query, deadline: Deadline) -> impl Future<Output = Result<Arc<CatalogSnapshot>, CatalogError>> + Send;
}

// ---------------------------------------------------------------- acquirer

/// What to acquire: the repository URL and DOC's directory name for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRequest {
    pub url: String,
    pub segment: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Cloned (or refreshed) for this request.
    Fetched,
    /// A copy within the staleness window was reused.
    Cached,
    /// A refresh failed; the older copy is served.
    Stale,
}

#[derive(Clone)]
pub struct Acquired {
    pub url: String,
    pub segment: String,
    pub reader: Arc<dyn RepoReader>,
    pub freshness: Freshness,
    /// Seconds since the copy was fetched.
    pub age_secs: u64,
    /// Present when `freshness` is `Stale`: why the refresh failed.
    pub refresh_error: Option<String>,
    /// The directory of this version on disk, when the repository lives on disk (a child process
    /// that must not share memory with this one reads it from here).
    pub root: Option<PathBuf>,
}

impl std::fmt::Debug for Acquired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Acquired").field("url", &self.url).field("freshness", &self.freshness).field("age_secs", &self.age_secs).finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailKind {
    CloneFailed,
    Timeout,
    Spawn,
    InvalidClone,
    DeadlineExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedRepo {
    pub url: String,
    pub segment: String,
    pub kind: FailKind,
    pub attempts: u32,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct Acquisition {
    pub acquired: Vec<Acquired>,
    pub failed: Vec<FailedRepo>,
}

impl Acquisition {
    /// The failures and stale services as findings (fixed mode returns them; as-built logs them).
    pub fn findings(&self) -> Vec<Finding> {
        let mut out = Vec::new();
        for f in &self.failed {
            out.push(
                Finding::new(
                    FindingCode::RepoUnavailable,
                    Severity::Error,
                    format!("{:?} after {} attempt(s): {}", f.kind, f.attempts, f.message),
                )
                .repo(&f.url),
            );
        }
        for a in &self.acquired {
            if a.freshness == Freshness::Stale {
                out.push(
                    Finding::new(
                        FindingCode::RepoServedStale,
                        Severity::Warning,
                        format!("refresh failed ({}); the copy from {} s ago was served", a.refresh_error.as_deref().unwrap_or("unknown"), a.age_secs),
                    )
                    .repo(&a.url),
                );
            }
        }
        out
    }
}

pub trait Acquirer: Send + Sync + 'static {
    fn acquire(&self, repos: &[RepoRequest], deadline: Deadline) -> impl Future<Output = Acquisition> + Send;
}


// ---------------------------------------------------------------- cross-process lock

/// A held lock. `fence` grows with every grant of the same key: a holder whose fence is older than the
/// current one has lost the lock and must not publish what it made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockGuard {
    pub key: String,
    /// Identifies this grant (owner and fence); only the holder can renew or release it.
    pub token: String,
    pub fence: u64,
}

pub type LockFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A lock shared by every process that clones into the same cache: one clone per repository, not one per
/// worker. A lease bounds a dead holder; correctness never depends on it (the swap is atomic and a second
/// clone is only wasted work), so a backend that is down is not an error for the caller (`Err` = proceed
/// without the lock).
pub trait RepoLock: Send + Sync + 'static {
    /// Wait up to `wait` for the lock. `Ok(None)`: someone else still holds it after `wait`.
    fn acquire<'a>(&'a self, key: &'a str, wait: Duration) -> LockFuture<'a, Result<Option<LockGuard>, String>>;
    /// Extend the lease. `false`: the lock is no longer this guard's.
    fn renew<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool>;
    /// Remove the lock only if this guard still holds it (compare and delete).
    fn release<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, ()>;
    /// Whether this guard still holds the lock (the fencing check before publishing).
    fn holds<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool>;
}
