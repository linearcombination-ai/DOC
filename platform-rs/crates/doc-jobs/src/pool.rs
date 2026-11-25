//! The persistent docx worker pool, beside the per-job process mode of [`crate::seam`].
//!
//! A worker is `python3 -I docx_worker.py --serve`: one `DocxJob` JSON line in on stdin, one answer line out on
//! stdout, JSON log lines on stderr (relayed through `tracing` with the request id). The pool keeps up to
//! `size` of them; a worker is replaced when it crashes, when a job passes its deadline (the whole process
//! group is killed), when it has done `recycle_after` jobs, or when its resident size passes `rss_ceiling_kb`.
//! A job never waits for a replacement to be *ready*: replacement is lazy (the next job spawns one).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, Semaphore};
use tracing::{info, warn};

use crate::model::JobError;
use crate::runner::JobCtx;
use crate::seam::{relay_line, run_docx_worker, tail, validate_docx_file, DocxJob, DocxResult, GroupGuard, SeamConfig, SeamReport};

#[derive(Clone, Debug)]
pub struct PoolConfig {
    pub python: PathBuf,
    pub script: PathBuf,
    /// Workers at most; also the number of jobs that can use the docx stage at once.
    pub size: usize,
    /// One job's ceiling; past it the worker is killed (with its group) and replaced.
    pub deadline: Duration,
    /// Retire a worker after this many jobs (a bound on slow leaks in the generator).
    pub recycle_after: u32,
    /// Retire a worker whose resident size exceeds this (KiB) after a job.
    pub rss_ceiling_kb: u64,
    pub env: Vec<(String, String)>,
}

#[derive(Default)]
pub struct PoolStats {
    pub spawned: AtomicU64,
    pub jobs: AtomicU64,
    pub restart_crash: AtomicU64,
    pub restart_deadline: AtomicU64,
    pub restart_recycle: AtomicU64,
    pub restart_memory: AtomicU64,
    pub live: AtomicU64,
    pub busy: AtomicU64,
    pub last_rss_kb: AtomicU64,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
    guard: GroupGuard,
    jobs_done: u32,
    spawned_at: Instant,
}

impl Worker {
    async fn kill(mut self) {
        self.guard.kill();
        let _ = self.child.wait().await; // reaped: the pid is really gone when this returns
    }
}

pub struct DocxPool {
    cfg: PoolConfig,
    idle: Mutex<Vec<Worker>>,
    slots: Semaphore,
    pub stats: Arc<PoolStats>,
}

#[derive(Clone, Copy)]
enum Why {
    Crash,
    Deadline,
    Recycle,
    Memory,
}

impl DocxPool {
    pub fn new(cfg: PoolConfig) -> Arc<DocxPool> {
        let n = cfg.size.max(1);
        Arc::new(DocxPool { cfg, idle: Mutex::new(Vec::new()), slots: Semaphore::new(n), stats: Arc::new(PoolStats::default()) })
    }

    /// Start one worker ahead of the first job, so the first job does not pay the interpreter start.
    pub async fn warm(&self) {
        if let Ok(w) = self.spawn().await {
            self.idle.lock().await.push(w);
        }
    }

    async fn spawn(&self) -> Result<Worker, JobError> {
        let mut cmd = Command::new(&self.cfg.python);
        cmd.arg("-I").arg(&self.cfg.script).arg("--serve");
        for (k, v) in &self.cfg.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).process_group(0);
        let mut child = cmd.spawn().map_err(|e| JobError::transient("docx_worker_spawn", format!("{}: {e}", self.cfg.python.display())))?;
        let pid = child.id().unwrap_or(0);
        let guard = GroupGuard { pgid: i32::try_from(pid).ok().filter(|p| *p > 0) };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(JobError::transient("docx_worker_spawn", "no pipes on the worker process"));
        };
        if let Some(err) = child.stderr.take() {
            tokio::spawn(relay_stderr(err));
        }
        self.stats.spawned.fetch_add(1, Ordering::SeqCst);
        self.stats.live.fetch_add(1, Ordering::SeqCst);
        info!(pid, "persistent docx worker started");
        Ok(Worker { child, stdin, stdout: BufReader::new(stdout), pid, guard, jobs_done: 0, spawned_at: Instant::now() })
    }

    fn restarted(&self, why: Why, pid: u32, detail: &str) {
        let c = match why {
            Why::Crash => &self.stats.restart_crash,
            Why::Deadline => &self.stats.restart_deadline,
            Why::Recycle => &self.stats.restart_recycle,
            Why::Memory => &self.stats.restart_memory,
        };
        c.fetch_add(1, Ordering::SeqCst);
        self.stats.live.fetch_sub(1, Ordering::SeqCst);
        warn!(pid, reason = match why { Why::Crash => "crash", Why::Deadline => "deadline", Why::Recycle => "recycle", Why::Memory => "memory" }, detail, "persistent docx worker replaced");
    }

    /// Do one job on a pooled worker.
    pub async fn run(&self, job: &DocxJob, ctx: Option<&JobCtx>) -> Result<SeamReport, JobError> {
        let _slot = self.slots.acquire().await.map_err(|e| JobError::transient("docx_pool_closed", e.to_string()))?;
        let worker = match self.idle.lock().await.pop() {
            Some(w) => w,
            None => self.spawn().await?,
        };
        self.stats.busy.fetch_add(1, Ordering::SeqCst);
        let out = self.run_on(worker, job, ctx).await;
        self.stats.busy.fetch_sub(1, Ordering::SeqCst);
        out
    }

    async fn run_on(&self, mut w: Worker, job: &DocxJob, ctx: Option<&JobCtx>) -> Result<SeamReport, JobError> {
        let started = Instant::now();
        let pid = w.pid;
        let mut line = serde_json::to_vec(job).map_err(|e| JobError::permanent("docx_job_encode", e.to_string()))?;
        line.push(b'\n');
        let cancelled = async {
            match ctx {
                Some(c) => c.cancelled().await,
                None => std::future::pending::<()>().await,
            }
        };
        let exchange = async {
            w.stdin.write_all(&line).await?;
            w.stdin.flush().await?;
            let mut answer = String::new();
            let n = (&mut w.stdout).take(4 * 1024 * 1024).read_line(&mut answer).await?;
            Ok::<(usize, String), std::io::Error>((n, answer))
        };
        let outcome = tokio::select! {
            r = tokio::time::timeout(self.cfg.deadline, exchange) => match r {
                Ok(Ok((0, _))) => Err((Why::Crash, JobError::transient("docx_worker_crashed", format!("the persistent docx worker (pid {pid}) closed its output: it died")))),
                Ok(Ok((_, a))) => Ok(a),
                Ok(Err(e)) => Err((Why::Crash, JobError::transient("docx_worker_crashed", format!("the persistent docx worker (pid {pid}) failed: {e}")))),
                Err(_) => Err((Why::Deadline, JobError::transient("docx_worker_timeout", format!("the docx worker (pid {pid}) did not finish in {} s and was killed", self.cfg.deadline.as_secs())))),
            },
            _ = cancelled => Err((Why::Deadline, JobError::transient("docx_worker_cancelled", format!("the job was cancelled; docx worker (pid {pid}) killed")))),
        };
        let answer = match outcome {
            Ok(a) => a,
            Err((why, err)) => {
                w.kill().await;
                self.restarted(why, pid, &err.to_string());
                return Err(err);
            }
        };
        let wall = started.elapsed();
        let result: DocxResult = match serde_json::from_str(answer.trim()) {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("no JSON answer ({e}): {}", tail(answer.as_bytes()));
                w.kill().await;
                self.restarted(Why::Crash, pid, &msg);
                return Err(JobError::transient("docx_worker_protocol", msg));
            }
        };
        w.jobs_done += 1;
        self.stats.jobs.fetch_add(1, Ordering::SeqCst);
        self.stats.last_rss_kb.store(result.rss_kb, Ordering::SeqCst);
        let retire = if w.jobs_done >= self.cfg.recycle_after {
            Some(Why::Recycle)
        } else if self.cfg.rss_ceiling_kb > 0 && result.rss_kb > self.cfg.rss_ceiling_kb {
            Some(Why::Memory)
        } else {
            None
        };
        let verdict: Result<SeamReport, JobError> = if !result.ok {
            let m = format!("the docx worker answered exit {}: {}", result.exit, result.message);
            Err(if result.exit == 2 { JobError::permanent("docx_input_rejected", m) } else { JobError::transient("docx_worker_failed", m) })
        } else {
            match validate_docx_file(&job.output_path).await {
                Ok(()) => Ok(SeamReport { pid, wall, result: result.clone() }),
                Err(e) => Err(e),
            }
        };
        match retire {
            Some(why) => {
                let detail = format!("after {} job(s), {} KiB resident, up {} s", w.jobs_done, result.rss_kb, w.spawned_at.elapsed().as_secs());
                w.kill().await;
                self.restarted(why, pid, &detail);
            }
            None => self.idle.lock().await.push(w),
        }
        if let Ok(r) = &verdict {
            info!(pid, wall_ms = wall.as_millis() as u64, worker_ms = r.result.worker_ms, rss_kb = r.result.rss_kb, jobs_done = r.result.jobs_done, "docx worker finished (persistent)");
        }
        verdict
    }

    pub fn info(&self) -> Value {
        let s = &self.stats;
        json!({
            "mode": "persistent",
            "size": self.cfg.size, "live": s.live.load(Ordering::SeqCst), "busy": s.busy.load(Ordering::SeqCst),
            "spawned": s.spawned.load(Ordering::SeqCst), "jobs": s.jobs.load(Ordering::SeqCst),
            "restarts": {"crash": s.restart_crash.load(Ordering::SeqCst), "deadline": s.restart_deadline.load(Ordering::SeqCst), "recycle": s.restart_recycle.load(Ordering::SeqCst), "memory": s.restart_memory.load(Ordering::SeqCst)},
            "last_rss_kb": s.last_rss_kb.load(Ordering::SeqCst), "recycle_after": self.cfg.recycle_after, "rss_ceiling_kb": self.cfg.rss_ceiling_kb,
        })
    }

    /// Kill every idle worker (shutdown).
    pub async fn shutdown(&self) {
        let workers: Vec<Worker> = std::mem::take(&mut *self.idle.lock().await);
        for w in workers {
            w.kill().await;
            self.stats.live.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

async fn relay_stderr(err: ChildStderr) {
    let mut lines = BufReader::new(err).lines();
    while let Ok(Some(l)) = lines.next_line().await {
        relay_line(&l, "");
    }
}

/// Which way the docx stage runs.
pub enum DocxBackend {
    PerJob(SeamConfig),
    Pool(Arc<DocxPool>),
    /// The Python service in another container (P7).
    Remote(crate::remote::RemoteConfig, reqwest::Client),
}

impl DocxBackend {
    pub async fn run(&self, job: &DocxJob, job_file: &Path, ctx: Option<&JobCtx>) -> Result<SeamReport, JobError> {
        match self {
            DocxBackend::PerJob(cfg) => run_docx_worker(cfg, job, job_file, ctx).await,
            DocxBackend::Pool(p) => p.run(job, ctx).await,
            DocxBackend::Remote(cfg, client) => crate::remote::run_docx_remote(client, cfg, job, job_file, ctx).await,
        }
    }
    pub fn info(&self) -> Value {
        match self {
            DocxBackend::PerJob(cfg) => json!({"mode": "per_job", "deadline_s": cfg.deadline.as_secs()}),
            DocxBackend::Pool(p) => p.info(),
            DocxBackend::Remote(cfg, _) => json!({"mode": "remote", "url": cfg.base_url, "deadline_s": cfg.docx_deadline.as_secs()}),
        }
    }
    pub fn pool(&self) -> Option<&Arc<DocxPool>> {
        match self {
            DocxBackend::Pool(p) => Some(p),
            DocxBackend::PerJob(_) | DocxBackend::Remote(..) => None,
        }
    }
}
