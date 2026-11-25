use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use doc_flight::SingleFlight;
use doc_ports::{Acquired, Acquirer, Acquisition, Deadline, FailKind, FailedRepo, Freshness, RepoLock, RepoRequest};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use crate::disk::DiskRepo;

#[derive(Clone, Debug)]
pub struct GitConfig {
    pub git_bin: PathBuf,
    pub cache_dir: PathBuf,
    pub user_agent: String,
    pub x_requested_with: String,
    /// DOC's `ASSET_CACHING_PERIOD` (30 minutes).
    pub stale_after: Duration,
    /// Time for one clone attempt; the subprocess (and its process group) is killed after it.
    pub clone_timeout: Duration,
    /// git's own stall detection: abort below `low_speed_limit` bytes/s for `low_speed_time`.
    pub low_speed_limit: u32,
    pub low_speed_time: Duration,
    pub max_attempts: u32,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// Total time one refresh of one repository may take, attempts and waits included.
    pub fetch_ceiling: Duration,
    /// Clones running at once across all requests.
    pub max_parallel_clones: usize,
    /// Old versions are removed this long after a swap (readers may still hold them).
    pub gc_grace: Duration,
    /// Lease of the cross-process clone lock; renewed while a clone runs.
    pub lock_lease: Duration,
    /// How long a worker waits for another worker's clone of the same repository before cloning itself.
    pub lock_wait: Duration,
}

impl GitConfig {
    pub fn new(cache_dir: PathBuf) -> GitConfig {
        GitConfig {
            git_bin: PathBuf::from("git"),
            cache_dir,
            user_agent: "wa-doc".into(),
            x_requested_with: "WA-Tool-Doc".into(),
            stale_after: Duration::from_secs(30 * 60),
            clone_timeout: Duration::from_secs(180),
            low_speed_limit: 1000,
            low_speed_time: Duration::from_secs(45),
            max_attempts: 4,
            backoff_base: Duration::from_millis(500),
            backoff_cap: Duration::from_secs(15),
            fetch_ceiling: Duration::from_secs(600),
            max_parallel_clones: 8,
            gc_grace: Duration::from_secs(600),
            lock_lease: Duration::from_secs(30),
            lock_wait: Duration::from_secs(660),
        }
    }
}

#[derive(Clone, Debug)]
struct Live {
    dir: PathBuf,
    age: Duration,
}

#[derive(Clone, Debug)]
struct Refreshed {
    live: Live,
    freshness: Freshness,
}

#[derive(Clone, Debug)]
struct RefreshFailed {
    kind: FailKind,
    attempts: u32,
    message: String,
}

type FlightValue = Result<Refreshed, RefreshFailed>;

struct Inner {
    cfg: GitConfig,
    clones: Semaphore,
    counter: AtomicU64,
    /// Shared by every process that uses the same cache directory (None: one process only).
    lock: Option<Arc<dyn RepoLock>>,
    /// Clones this process actually ran (a counter for the metrics and for tests).
    clones_run: AtomicU64,
}

pub struct GitAcquirer {
    inner: Arc<Inner>,
    /// repository directory -> (live version directory, its reader): one `DiskRepo` per version, so
    /// its directory listing is computed once, not per request.
    readers: Mutex<HashMap<PathBuf, (PathBuf, Arc<DiskRepo>)>>,
    flight: SingleFlight<String, FlightValue>,
}

fn key_of(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// What a failed clone tells us: a missing repository will not appear within one request.
fn looks_permanent(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("repository not found") || s.contains("does not exist") || s.contains("not found: did you run git update-server-info")
}

impl GitAcquirer {
    pub fn new(cfg: GitConfig) -> GitAcquirer {
        let parallel = cfg.max_parallel_clones.max(1);
        GitAcquirer {
            inner: Arc::new(Inner { cfg, clones: Semaphore::new(parallel), counter: AtomicU64::new(0), lock: None, clones_run: AtomicU64::new(0) }),
            flight: SingleFlight::new(),
            readers: Mutex::new(HashMap::new()),
        }
    }

    /// Share clones with other processes through `lock`: only the holder clones a repository; the others
    /// wait, then find the fresh copy on disk.
    pub fn with_lock(cfg: GitConfig, lock: Arc<dyn RepoLock>) -> GitAcquirer {
        let parallel = cfg.max_parallel_clones.max(1);
        GitAcquirer {
            inner: Arc::new(Inner { cfg, clones: Semaphore::new(parallel), counter: AtomicU64::new(0), lock: Some(lock), clones_run: AtomicU64::new(0) }),
            flight: SingleFlight::new(),
            readers: Mutex::new(HashMap::new()),
        }
    }

    /// Clones this process ran since it started (cache hits and clones waited for are not counted).
    pub fn clones_run(&self) -> Arc<dyn Fn() -> u64 + Send + Sync> {
        let inner = self.inner.clone();
        Arc::new(move || inner.clones_run.load(Ordering::SeqCst))
    }

    fn reader_for(&self, repo_dir: &Path, live_dir: &Path) -> Arc<DiskRepo> {
        let mut map = match self.readers.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some((dir, reader)) = map.get(repo_dir) {
            if dir == live_dir {
                return reader.clone();
            }
        }
        let reader = Arc::new(DiskRepo::new(live_dir.to_path_buf()));
        map.insert(repo_dir.to_path_buf(), (live_dir.to_path_buf(), reader.clone()));
        reader
    }

    pub fn config(&self) -> &GitConfig {
        &self.inner.cfg
    }

    /// The directory a URL's versions live in.
    pub fn repo_dir(&self, url: &str) -> PathBuf {
        self.inner.cfg.cache_dir.join(key_of(url))
    }
}

impl Inner {
    /// The live version of a repository, if a valid copy exists (DOC's check: `.git/config`,
    /// `.git/HEAD`, `.git/objects` present and the tree not empty).
    fn inspect(&self, repo_dir: &Path) -> Option<Live> {
        let target = std::fs::read_link(repo_dir.join("current")).ok()?;
        let dir = repo_dir.join(&target);
        let git = dir.join(".git");
        let valid = ["config", "HEAD", "objects"].iter().all(|f| git.join(f).exists())
            && std::fs::read_dir(&dir).map(|mut d| d.any(|e| e.is_ok_and(|e| e.file_name() != ".git"))).unwrap_or(false);
        if !valid {
            return None;
        }
        let marker = repo_dir.join(format!("{}.fetched", target.to_string_lossy()));
        let age = std::fs::metadata(&marker)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .unwrap_or(Duration::MAX);
        Some(Live { dir, age })
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let exp = self.cfg.backoff_base.saturating_mul(1u32 << attempt.saturating_sub(1).min(16));
        exp.min(self.cfg.backoff_cap).mul_f64(rand::random::<f64>())
    }

    async fn clone_once(&self, url: &str, dest: &Path) -> Result<(), (FailKind, String, bool)> {
        let c = &self.cfg;
        let mut cmd = Command::new(&c.git_bin);
        cmd.arg("-c").arg(format!("http.userAgent={}", c.user_agent))
            .arg("-c").arg(format!("http.extraHeader=X-Requested-With:{}", c.x_requested_with))
            .arg("-c").arg(format!("http.lowSpeedLimit={}", c.low_speed_limit))
            .arg("-c").arg(format!("http.lowSpeedTime={}", c.low_speed_time.as_secs()))
            .args(["clone", "--depth=1", "--single-branch", "--quiet", "--", url])
            .arg(dest)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0);
        let child = cmd.spawn().map_err(|e| (FailKind::Spawn, format!("cannot start {}: {e}", c.git_bin.display()), false))?;
        let pid = child.id();
        match tokio::time::timeout(c.clone_timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let tail: String = stderr.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
                    Err((FailKind::CloneFailed, format!("git exited with {}: {}", output.status, tail.trim()), looks_permanent(&stderr)))
                }
            }
            Ok(Err(e)) => Err((FailKind::CloneFailed, format!("waiting for git: {e}"), false)),
            Err(_) => {
                if let Some(pid) = pid {
                    // kill the whole group so git-remote-https goes too; no unsafe, no extra crate
                    let _ = Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).output().await;
                }
                Err((FailKind::Timeout, format!("clone exceeded {} s and was killed", c.clone_timeout.as_secs()), false))
            }
        }
    }

    /// Clone with retries into a temp dir and swap it in. Runs inside the single-flight.
    async fn refresh(self: Arc<Self>, url: String, repo_dir: PathBuf) -> FlightValue {
        let Some(lock) = self.lock.clone() else {
            return self.refresh_locked(url, repo_dir, None).await;
        };
        // Cross-process single flight: one worker clones, the others wait and then find the fresh copy.
        let key = format!("clone:{}", key_of(&url));
        let guard = match lock.acquire(&key, self.cfg.lock_wait).await {
            Ok(Some(g)) => Some(g),
            Ok(None) => {
                warn!(repo = %url, wait_s = self.cfg.lock_wait.as_secs(), "another worker held the clone lock for the whole wait; cloning anyway");
                None
            }
            Err(e) => {
                warn!(repo = %url, error = %e, "clone lock unavailable; cloning without it");
                None
            }
        };
        // keep the lease alive while the clone runs
        let renewer = guard.clone().map(|g| {
            let lock = lock.clone();
            let every = (self.cfg.lock_lease / 3).max(Duration::from_millis(100));
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(every).await;
                    if !lock.renew(&g).await {
                        warn!(key = %g.key, "clone lock lease lost while cloning");
                        return;
                    }
                }
            })
        });
        let out = self.refresh_locked(url, repo_dir, guard.clone().map(|g| (lock.clone(), g))).await;
        if let Some(r) = renewer {
            r.abort();
        }
        if let Some(g) = guard {
            lock.release(&g).await;
        }
        out
    }

    async fn refresh_locked(self: Arc<Self>, url: String, repo_dir: PathBuf, held: Option<(Arc<dyn RepoLock>, doc_ports::LockGuard)>) -> FlightValue {
        // the request that led here may have been beaten by another one that has just finished
        if let Some(live) = self.inspect(&repo_dir) {
            if live.age <= self.cfg.stale_after {
                return Ok(Refreshed { live, freshness: Freshness::Cached });
            }
        }
        let started = Instant::now();
        let _permit = self.clones.acquire().await.map_err(|e| RefreshFailed { kind: FailKind::Spawn, attempts: 0, message: e.to_string() })?;
        let _ = tokio::fs::create_dir_all(&repo_dir).await;
        let mut attempts = 0u32;
        let mut last = (FailKind::CloneFailed, String::from("no attempt made"));
        while attempts < self.cfg.max_attempts && started.elapsed() < self.cfg.fetch_ceiling {
            attempts += 1;
            let id = format!("{}-{}-{}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0), std::process::id(), self.counter.fetch_add(1, Ordering::SeqCst));
            let tmp = repo_dir.join(format!("tmp-{id}"));
            let t0 = Instant::now();
            self.clones_run.fetch_add(1, Ordering::SeqCst);
            let result = self.clone_once(&url, &tmp).await;
            let result: Result<(), (FailKind, String, bool)> = match result {
                Ok(()) => match self.fenced_swap(&repo_dir, &tmp, &id, held.as_ref()).await {
                    Ok(live) => {
                        info!(repo = %url, attempts, elapsed_ms = t0.elapsed().as_millis() as u64, "repository fetched");
                        self.spawn_gc(repo_dir.clone(), id);
                        return Ok(Refreshed { live, freshness: Freshness::Fetched });
                    }
                    Err(msg) => Err((FailKind::InvalidClone, msg, false)),
                },
                Err(e) => Err(e),
            };
            let _ = tokio::fs::remove_dir_all(&tmp).await; // a partial clone never stays behind
            if let Err((kind, msg, permanent)) = result {
                last = (kind, msg.clone());
                let exhausted = permanent || attempts >= self.cfg.max_attempts || started.elapsed() >= self.cfg.fetch_ceiling;
                if exhausted {
                    warn!(repo = %url, attempt = attempts, kind = ?kind, error = %msg, permanent, "clone failed, giving up");
                    break;
                }
                let delay = self.backoff(attempts);
                warn!(repo = %url, attempt = attempts, max = self.cfg.max_attempts, kind = ?kind, error = %msg, attempt_ms = t0.elapsed().as_millis() as u64, next_in_ms = delay.as_millis() as u64, "clone attempt failed, will retry");
                tokio::time::sleep(delay).await;
            }
        }
        Err(RefreshFailed { kind: last.0, attempts, message: last.1 })
    }

    /// The fencing check before publishing: a holder that lost its lock (lease expired while it was stuck)
    /// does not swap; another process is cloning or has cloned, and ours would only race it.
    async fn fenced_swap(&self, repo_dir: &Path, tmp: &Path, id: &str, held: Option<&(Arc<dyn RepoLock>, doc_ports::LockGuard)>) -> Result<Live, String> {
        if let Some((lock, guard)) = held {
            if !lock.holds(guard).await {
                if let Some(live) = self.inspect(repo_dir) {
                    if live.age <= self.cfg.stale_after {
                        info!(repo = %repo_dir.display(), fence = guard.fence, "clone lock lost, and a fresh copy exists: discarding this clone");
                        let _ = tokio::fs::remove_dir_all(tmp).await;
                        return Ok(live);
                    }
                }
                warn!(repo = %repo_dir.display(), fence = guard.fence, "clone lock lost but no fresh copy exists: publishing this clone anyway (the swap is atomic)");
            }
        }
        self.validate_and_swap(repo_dir, tmp, id).await
    }

    async fn validate_and_swap(&self, repo_dir: &Path, tmp: &Path, id: &str) -> Result<Live, String> {
        let git = tmp.join(".git");
        let populated = ["config", "HEAD", "objects"].iter().all(|f| git.join(f).exists());
        let has_files = std::fs::read_dir(tmp).map(|mut d| d.any(|e| e.is_ok_and(|e| e.file_name() != ".git"))).unwrap_or(false);
        if !populated || !has_files {
            return Err("git reported success but the clone is empty or incomplete".into());
        }
        let version = format!("v-{id}");
        let final_dir = repo_dir.join(&version);
        tokio::fs::rename(tmp, &final_dir).await.map_err(|e| format!("rename into place: {e}"))?;
        tokio::fs::write(repo_dir.join(format!("{version}.fetched")), b"").await.map_err(|e| format!("marker: {e}"))?;
        let link_tmp = repo_dir.join(format!("current.new-{id}"));
        let _ = tokio::fs::remove_file(&link_tmp).await;
        tokio::fs::symlink(&version, &link_tmp).await.map_err(|e| format!("symlink: {e}"))?;
        tokio::fs::rename(&link_tmp, repo_dir.join("current")).await.map_err(|e| format!("atomic swap: {e}"))?;
        Ok(Live { dir: final_dir, age: Duration::ZERO })
    }

    /// Remove superseded versions, but only after the grace period (a reader may be in one).
    fn spawn_gc(self: &Arc<Self>, repo_dir: PathBuf, keep_id: String) {
        let grace = self.cfg.gc_grace;
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let keep = format!("v-{keep_id}");
            let Ok(mut rd) = tokio::fs::read_dir(&repo_dir).await else { return };
            while let Ok(Some(entry)) = rd.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                let old_version = name.starts_with("v-") && name != keep && name != format!("{keep}.fetched");
                let stale_tmp = name.starts_with("tmp-") || name.starts_with("current.new-");
                if old_version || stale_tmp {
                    let path = entry.path();
                    if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                        let _ = tokio::fs::remove_dir_all(path).await;
                    } else {
                        let _ = tokio::fs::remove_file(path).await;
                    }
                }
            }
        });
    }
}

impl Acquirer for GitAcquirer {
    async fn acquire(&self, repos: &[RepoRequest], deadline: Deadline) -> Acquisition {
        let futures = repos.iter().map(|req| self.one(req.clone(), deadline));
        let results = futures::future::join_all(futures).await;
        let mut out = Acquisition::default();
        for r in results {
            match r {
                Ok(a) => out.acquired.push(a),
                Err(f) => out.failed.push(f),
            }
        }
        out
    }
}

impl GitAcquirer {
    async fn one(&self, req: RepoRequest, deadline: Deadline) -> Result<Acquired, FailedRepo> {
        let inner = self.inner.clone();
        let repo_dir = self.repo_dir(&req.url);
        let repo_dir_for_reader = repo_dir.clone();
        let live = inner.inspect(&repo_dir);
        let acquired = |live: Live, freshness: Freshness, refresh_error: Option<String>| Acquired {
            url: req.url.clone(),
            segment: req.segment.clone(),
            reader: self.reader_for(&repo_dir_for_reader, &live.dir),
            freshness,
            age_secs: live.age.as_secs(),
            refresh_error,
            root: Some(live.dir.clone()),
        };
        if let Some(l) = &live {
            if l.age <= inner.cfg.stale_after {
                debug!(repo = %req.url, age_s = l.age.as_secs(), "repository cache hit");
                return Ok(acquired(l.clone(), Freshness::Cached, None));
            }
        }
        let failure = |kind: FailKind, attempts: u32, message: String| FailedRepo { url: req.url.clone(), segment: req.segment.clone(), kind, attempts, message };
        let url = req.url.clone();
        let flight = self.flight.run(req.url.clone(), inner.clone().refresh(url, repo_dir));
        let outcome = tokio::time::timeout(deadline.remaining(), flight).await;
        let stale_or = |reason: String, f: FailedRepo, live: Option<Live>| match live {
            Some(l) => {
                warn!(repo = %req.url, age_s = l.age.as_secs(), reason = %reason, "serving the stale repository copy");
                Ok(acquired(l, Freshness::Stale, Some(reason)))
            }
            None => Err(f),
        };
        match outcome {
            Ok((Ok(Ok(refreshed)), role)) => {
                debug!(repo = %req.url, ?role, freshness = ?refreshed.freshness, "repository ready");
                Ok(acquired(refreshed.live, refreshed.freshness, None))
            }
            Ok((Ok(Err(e)), _)) => {
                let msg = e.message.clone();
                stale_or(msg.clone(), failure(e.kind, e.attempts, msg), live)
            }
            Ok((Err(e), _)) => {
                let msg = format!("{e:?}");
                stale_or(msg.clone(), failure(FailKind::Spawn, 0, msg), live)
            }
            Err(_) => {
                let msg = "request deadline passed while the fetch was still running (it continues in the background)".to_string();
                stale_or(msg.clone(), failure(FailKind::DeadlineExceeded, 0, msg), live)
            }
        }
    }
}
