//! The data-API client behind the `Catalog` port.
//!
//! Behaviour (each line is a requirement of the P4 brief and has a fault-injection test):
//! * every attempt has a connect and a total timeout; the whole refresh has a ceiling;
//! * a failed attempt (transport error, timeout, non-200, a body that is not a usable GraphQL
//!   answer) is retried with exponential backoff and full jitter, honouring `Retry-After`, up to
//!   `max_attempts` or the ceiling; the failure is returned as a typed [`CatalogError`];
//! * a failure is never cached; a success is cached for `ttl` (DOC: 180 s);
//! * concurrent requests for the same query share one refresh (single-flight); the refresh runs
//!   as its own task, so a caller that gives up (its deadline) leaves the refresh running and the
//!   next request is served from the cache it fills;
//! * when a refresh fails and an older success exists (at most `max_stale` old) it is served,
//!   marked `stale` (self-heal without hiding the condition);
//! * logs: one `info` line per refresh outcome, `warn` per failed attempt with the delay to the
//!   next, `debug` for cache hits and joins (with ages and roles).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use doc_flight::SingleFlight;
use doc_lookup::catalog::Query;
use doc_ports::{Catalog, CatalogError, CatalogSnapshot, Deadline};
use tracing::{debug, info, warn};

/// DOC's two GraphQL queries, verbatim (`resource_lookup.py:68-83`, `:147-165`).
const PRIMARY_QUERY: &str = "\nquery MyQuery {\n  git_repo(\n    where: {content: {wa_content_metadata: {status: {_eq: \"Primary\"}}}}\n  ) {\n    repo_url\n    content {\n      resource_type\n      language {\n        english_name\n        ietf_code\n        national_name\n        direction\n      }\n    }\n  }\n}\n    ";
const NON_PRIMARY_QUERY: &str = "\nquery MyQuery {\n  git_repo(\n    where: {content: {language: {ietf_code: {_eq: \"bg\"}}}}\n  ) {\n    repo_url\n    content {\n      resource_type\n      language {\n        english_name\n        ietf_code\n        national_name\n        direction\n      }\n    }\n  }\n}\n    ";

#[derive(Clone, Debug)]
pub struct CatalogConfig {
    pub url: String,
    pub user_agent: String,
    pub x_requested_with: String,
    pub connect_timeout: Duration,
    /// Total time for one attempt.
    pub attempt_timeout: Duration,
    pub max_attempts: u32,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// Total time one refresh (all attempts and waits) may take.
    pub refresh_ceiling: Duration,
    /// How long a success is served without asking again (DOC: 180 s).
    pub ttl: Duration,
    /// The oldest success that may be served when a refresh fails.
    pub max_stale: Duration,
    /// A primary catalog with fewer rows than this is treated as a failed answer (an empty list
    /// from the data API is far more likely an upstream hiccup than a fact).
    pub min_primary_rows: usize,
}

impl CatalogConfig {
    pub fn new(url: &str) -> CatalogConfig {
        CatalogConfig {
            url: url.to_string(),
            user_agent: "wa-doc".into(),
            x_requested_with: "WA-Tool-Doc".into(),
            connect_timeout: Duration::from_secs(5),
            attempt_timeout: Duration::from_secs(45),
            max_attempts: 6,
            backoff_base: Duration::from_millis(250),
            backoff_cap: Duration::from_secs(10),
            refresh_ceiling: Duration::from_secs(120),
            ttl: Duration::from_secs(180),
            max_stale: Duration::from_secs(24 * 3600),
            min_primary_rows: 1,
        }
    }
}

struct Entry {
    snapshot: Arc<CatalogSnapshot>,
    fetched: Instant,
}

struct Inner {
    cfg: CatalogConfig,
    client: reqwest::Client,
    cache: Mutex<HashMap<Query, Entry>>,
    next_id: AtomicU64,
    stats: Arc<CatalogStats>,
}

pub struct HttpCatalog {
    inner: Arc<Inner>,
    flight: SingleFlight<Query, Result<Arc<CatalogSnapshot>, CatalogError>>,
}

fn query_text(q: Query) -> &'static str {
    match q {
        Query::Primary => PRIMARY_QUERY,
        Query::NonPrimary => NON_PRIMARY_QUERY,
    }
}

fn query_name(q: Query) -> &'static str {
    match q {
        Query::Primary => "primary",
        Query::NonPrimary => "non_primary",
    }
}

/// Counters the metrics and `/health/status` read: refreshes that answered, attempts that failed, and when
/// the data API last answered (Unix seconds, 0 = never).
#[derive(Debug, Default)]
pub struct CatalogStats {
    pub refreshes: AtomicU64,
    pub failures: AtomicU64,
    pub last_ok_unix_s: AtomicU64,
}

/// Why one attempt failed, and whether the server asked for a pause.
struct AttemptFailure {
    message: String,
    malformed: bool,
    retry_after: Option<Duration>,
}

impl HttpCatalog {
    pub fn new(cfg: CatalogConfig) -> Result<HttpCatalog, reqwest::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(cfg.connect_timeout)
            .timeout(cfg.attempt_timeout)
            .user_agent(cfg.user_agent.clone())
            .build()?;
        Ok(HttpCatalog {
            inner: Arc::new(Inner { cfg, client, cache: Mutex::new(HashMap::new()), next_id: AtomicU64::new(1), stats: Arc::new(CatalogStats::default()) }),
            flight: SingleFlight::new(),
        })
    }

    pub fn stats(&self) -> Arc<CatalogStats> {
        self.inner.stats.clone()
    }

    fn cached(&self, q: Query) -> Option<(Arc<CatalogSnapshot>, Duration)> {
        let cache = self.inner.cache.lock().ok()?;
        cache.get(&q).map(|e| (e.snapshot.clone(), e.fetched.elapsed()))
    }

    /// Forget a cached entry (tests, and an operator hook).
    pub fn invalidate(&self) {
        if let Ok(mut c) = self.inner.cache.lock() {
            c.clear();
        }
    }
}

impl Inner {
    async fn attempt(&self, q: Query) -> Result<Vec<u8>, AttemptFailure> {
        let body = serde_json::json!({ "query": query_text(q) });
        let response = self
            .client
            .post(&self.cfg.url)
            .header("X-Requested-With", &self.cfg.x_requested_with)
            .json(&body)
            .send()
            .await
            .map_err(|e| AttemptFailure { message: describe(&e), malformed: false, retry_after: None })?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|s| Duration::from_secs(s.min(30)));
        if !status.is_success() {
            return Err(AttemptFailure { message: format!("HTTP {status}"), malformed: false, retry_after });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure { message: format!("reading the body: {}", describe(&e)), malformed: false, retry_after: None })?;
        // the same validation the fixed parse applies to structure; rows are validated per mode later
        // (on the blocking pool: parsing a 5 MB catalog must not hold a runtime thread)
        let owned = bytes.to_vec();
        let parsed = tokio::task::spawn_blocking(move || {
            let r = doc_lookup::catalog::parse_fixed(&owned, q);
            (owned, r.map(|(repos, _)| repos.len()))
        })
        .await
        .map_err(|e| AttemptFailure { message: format!("catalog validation task failed: {e}"), malformed: false, retry_after: None })?;
        let (bytes, parsed) = parsed;
        match parsed {
            Ok(rows) => {
                if q == Query::Primary && rows < self.cfg.min_primary_rows {
                    return Err(AttemptFailure {
                        message: format!("primary catalog has {rows} rows (minimum {})", self.cfg.min_primary_rows),
                        malformed: true,
                        retry_after: None,
                    });
                }
                Ok(bytes)
            }
            Err(m) => Err(AttemptFailure { message: m.to_string(), malformed: true, retry_after: None }),
        }
    }

    fn backoff(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        let exp = self.cfg.backoff_base.saturating_mul(1u32 << attempt.saturating_sub(1).min(16));
        let capped = exp.min(self.cfg.backoff_cap);
        let jittered = capped.mul_f64(rand::random::<f64>());
        match retry_after {
            Some(ra) => ra.max(jittered),
            None => jittered,
        }
    }

    async fn refresh(self: Arc<Self>, q: Query) -> Result<Arc<CatalogSnapshot>, CatalogError> {
        let started = Instant::now();
        let mut attempts = 0u32;
        let (last, malformed_seen) = loop {
            attempts += 1;
            let t0 = Instant::now();
            match self.attempt(q).await {
                Ok(body) => {
                    let id = self.next_id.fetch_add(1, Ordering::SeqCst);
                    let size = body.len();
                    let snapshot = Arc::new(CatalogSnapshot::new(id, q, Arc::new(body), false, 0));
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.insert(q, Entry { snapshot: snapshot.clone(), fetched: Instant::now() });
                    }
                    self.stats.refreshes.fetch_add(1, Ordering::SeqCst);
                    self.stats.last_ok_unix_s.store(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), Ordering::SeqCst);
                    info!(query = query_name(q), attempts, bytes = size, elapsed_ms = started.elapsed().as_millis() as u64, snapshot = id, "catalog refreshed");
                    return Ok(snapshot);
                }
                Err(f) => {
                    self.stats.failures.fetch_add(1, Ordering::SeqCst);
                    let elapsed = started.elapsed();
                    let exhausted = attempts >= self.cfg.max_attempts || elapsed >= self.cfg.refresh_ceiling;
                    if exhausted {
                        warn!(query = query_name(q), attempt = attempts, max = self.cfg.max_attempts, error = %f.message, malformed = f.malformed, "catalog refresh failed, giving up");
                        break (f.message, f.malformed);
                    }
                    let delay = self.backoff(attempts, f.retry_after).min(self.cfg.refresh_ceiling.saturating_sub(elapsed));
                    warn!(
                        query = query_name(q), attempt = attempts, max = self.cfg.max_attempts, error = %f.message,
                        attempt_ms = t0.elapsed().as_millis() as u64, next_in_ms = delay.as_millis() as u64, "catalog attempt failed, will retry"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        };
        Err(if malformed_seen {
            CatalogError::Malformed { attempts, detail: last }
        } else {
            CatalogError::Unavailable { attempts, last }
        })
    }
}

impl Catalog for HttpCatalog {
    async fn snapshot(&self, q: Query, deadline: Deadline) -> Result<Arc<CatalogSnapshot>, CatalogError> {
        let ttl = self.inner.cfg.ttl;
        let max_stale = self.inner.cfg.max_stale;
        let cached = self.cached(q);
        if let Some((snap, age)) = &cached {
            if *age < ttl {
                debug!(query = query_name(q), age_ms = age.as_millis() as u64, snapshot = snap.id, "catalog cache hit");
                return Ok(Arc::new(snap.relabelled(false, age.as_secs())));
            }
        }
        let inner = self.inner.clone();
        let flight = self.flight.run(q, async move { inner.refresh(q).await });
        let outcome = tokio::time::timeout(deadline.remaining(), flight).await;
        let stale = |reason: &str| -> Option<Arc<CatalogSnapshot>> {
            let (snap, age) = cached.as_ref()?;
            if *age > max_stale {
                return None;
            }
            warn!(query = query_name(q), age_s = age.as_secs(), reason, snapshot = snap.id, "serving the stale catalog");
            Some(Arc::new(snap.relabelled(true, age.as_secs())))
        };
        match outcome {
            Ok((Ok(Ok(snap)), role)) => {
                debug!(query = query_name(q), ?role, snapshot = snap.id, "catalog answered");
                Ok(snap)
            }
            Ok((Ok(Err(e)), role)) => {
                debug!(query = query_name(q), ?role, error = %e, "catalog refresh failed");
                stale(&e.to_string()).ok_or(e)
            }
            Ok((Err(flight_error), _)) => {
                let e = CatalogError::Unavailable { attempts: 0, last: format!("{flight_error:?}") };
                stale(&e.to_string()).ok_or(e)
            }
            Err(_elapsed) => {
                let e = CatalogError::DeadlineExceeded { last: "refresh still running".into() };
                stale(&e.to_string()).ok_or(e)
            }
        }
    }
}

fn describe(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_decode() {
        "decode"
    } else if e.is_request() {
        "request"
    } else {
        "error"
    };
    // `Display` of a reqwest error includes the URL, which carries no secrets here (a public endpoint)
    format!("{kind}: {e}")
}
