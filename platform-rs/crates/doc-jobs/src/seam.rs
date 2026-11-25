//! The docx seam: a versioned `DocxJob` JSON file in, a docx file out, a Python process in between.
//!
//! Rust starts one process per job in its own process group, feeds it by file (argv carries one path),
//! reads a one-line JSON answer from stdout, and supervises it with a deadline and a cancel signal.
//! Whatever ends the wait (deadline, cancel, the future being dropped) kills the whole group, so a hung
//! interpreter or a grandchild it started cannot outlive the job; the child is always reaped.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{info, warn};

use crate::model::JobError;
use crate::runner::JobCtx;

pub const DOCX_JOB_VERSION: u32 = 2;

/// The message to the Python worker. Versioned: a worker that does not know the version exits 2.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocxJob {
    pub version: u32,
    pub job_id: String,
    /// The HTTP request id (`X-Request-Id`): the Python process logs it on its lines.
    #[serde(default)]
    pub request_id: String,
    pub request_key: String,
    /// Where the worker writes the docx (a scratch path; Rust moves it into place).
    pub output_path: PathBuf,
    pub book_name: String,
    pub title1: String,
    pub title2: String,
    pub layout_for_print: bool,
    /// The interleaver's parts, exactly as the core returned them.
    pub parts: Value,
}

/// The worker's one-line answer on stdout.
#[derive(Clone, Debug, Deserialize)]
pub struct DocxResult {
    pub ok: bool,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub worker_ms: f64,
    /// `ru_maxrss` of the worker process in KiB.
    #[serde(default)]
    pub maxrss_kb: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Current resident size of the worker in KiB (`--serve` answers and per-job answers).
    #[serde(default)]
    pub rss_kb: u64,
    /// Jobs this worker process has done (persistent mode).
    #[serde(default)]
    pub jobs_done: u64,
    /// `ok == false` in persistent mode: 2 not acceptable (permanent), 3 internal (transient).
    #[serde(default)]
    pub exit: u32,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct SeamConfig {
    pub python: PathBuf,
    pub script: PathBuf,
    pub deadline: Duration,
    /// Extra environment for the child (fault injection in tests).
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct SeamReport {
    pub pid: u32,
    pub wall: Duration,
    pub result: DocxResult,
}

/// Kills the process group when dropped unless disarmed.
pub(crate) struct GroupGuard {
    pub(crate) pgid: Option<i32>,
}

impl GroupGuard {
    pub(crate) fn kill(&mut self) {
        if let Some(pgid) = self.pgid.take() {
            // ESRCH (already gone) is the normal case after a clean exit.
            let _ = killpg(Pid::from_raw(pgid), Signal::SIGKILL);
        }
    }
    pub(crate) fn disarm(&mut self) {
        self.pgid = None;
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

pub(crate) async fn read_capped<R: tokio::io::AsyncRead + Unpin>(mut r: R, cap: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => return out,
            Ok(n) => {
                if out.len() < cap {
                    let take = n.min(cap - out.len());
                    out.extend_from_slice(&buf[..take]);
                }
            }
        }
    }
}

pub(crate) fn tail(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let t = s.trim();
    let start = t.len().saturating_sub(600);
    let mut i = start;
    while !t.is_char_boundary(i) {
        i += 1;
    }
    t[i..].to_string()
}

/// Run the worker for `job`. The job file is written next to the output (the caller's scratch dir).
pub async fn run_docx_worker(cfg: &SeamConfig, job: &DocxJob, job_file: &Path, ctx: Option<&JobCtx>) -> Result<SeamReport, JobError> {
    let bytes = serde_json::to_vec(job).map_err(|e| JobError::permanent("docx_job_encode", e.to_string()))?;
    tokio::fs::write(job_file, &bytes).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", job_file.display())))?;
    let mut cmd = Command::new(&cfg.python);
    cmd.arg("-I").arg(&cfg.script).arg("--job").arg(job_file);
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).process_group(0);
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| JobError::transient("docx_worker_spawn", format!("{}: {e}", cfg.python.display())))?;
    let pid = child.id().unwrap_or(0);
    // The child leads its own group (process_group(0)), so pgid == pid.
    let mut guard = GroupGuard { pgid: i32::try_from(pid).ok().filter(|p| *p > 0) };
    let out = child.stdout.take().map(|s| tokio::spawn(read_capped(s, 64 * 1024)));
    let err = child.stderr.take().map(|s| tokio::spawn(read_capped(s, 64 * 1024)));
    let cancelled = async {
        match ctx {
            Some(c) => c.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };
    let status = tokio::select! {
        r = tokio::time::timeout(cfg.deadline, child.wait()) => match r {
            Ok(Ok(s)) => Ok(s),
            Ok(Err(e)) => Err(JobError::transient("docx_worker_wait", e.to_string())),
            Err(_) => Err(JobError::transient("docx_worker_timeout", format!("the docx worker (pid {pid}) did not finish in {} s and was killed", cfg.deadline.as_secs()))),
        },
        _ = cancelled => Err(JobError::transient("docx_worker_cancelled", format!("the job was cancelled; docx worker (pid {pid}) killed"))),
    };
    let status = match status {
        Ok(s) => {
            guard.disarm();
            s
        }
        Err(e) => {
            guard.kill();
            let _ = child.wait().await; // reap: no zombie, the pid is really gone when this returns
            warn!(pid, code = ?e, "docx worker killed (process group)");
            return Err(e);
        }
    };
    // A grandchild may still hold the pipes; the group is killed so the readers end.
    let _ = killpg(Pid::from_raw(i32::try_from(pid).unwrap_or(0)), Signal::SIGKILL);
    let stdout = match out {
        Some(h) => h.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let stderr = match err {
        Some(h) => h.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let wall = started.elapsed();
    relay_worker_log(&stderr, &job.request_id);
    match status.code() {
        Some(0) => {}
        Some(2) => return Err(JobError::permanent("docx_input_rejected", format!("the docx worker rejected the job: {}", tail(&stderr)))),
        Some(c) => return Err(JobError::transient("docx_worker_failed", format!("the docx worker exited {c}: {}", tail(&stderr)))),
        None => return Err(JobError::transient("docx_worker_crashed", format!("the docx worker (pid {pid}) died from a signal: {}", tail(&stderr)))),
    }
    let line = String::from_utf8_lossy(&stdout).lines().rev().find(|l| l.trim_start().starts_with('{')).map(str::to_string).unwrap_or_default();
    let result: DocxResult = serde_json::from_str(&line).map_err(|e| JobError::transient("docx_worker_protocol", format!("no JSON answer on stdout ({e}): {}", tail(&stdout))))?;
    if !result.ok {
        return Err(JobError::transient("docx_worker_failed", "the docx worker reported ok=false".to_string()));
    }
    validate_docx_file(&job.output_path).await?;
    info!(pid, wall_ms = wall.as_millis() as u64, worker_ms = result.worker_ms, maxrss_kb = result.maxrss_kb, bytes = result.bytes, "docx worker finished");
    Ok(SeamReport { pid, wall, result })
}

/// A docx is a zip: local header magic at the start and an end-of-central-directory record at the end.
/// This catches a truncated or empty file; deeper checks (XML well-formedness) are the tests' job.
pub async fn validate_docx_file(path: &Path) -> Result<(), JobError> {
    let data = tokio::fs::read(path).await.map_err(|e| JobError::transient("docx_output_missing", format!("{}: {e}", path.display())))?;
    let eocd = data.len() >= 22 && data[data.len() - 22..data.len() - 18] == [0x50, 0x4b, 0x05, 0x06];
    if data.len() < 100 || data[..4] != [0x50, 0x4b, 0x03, 0x04] || !eocd {
        return Err(JobError::transient("docx_output_invalid", format!("{} is not a complete zip ({} bytes)", path.display(), data.len())));
    }
    Ok(())
}

/// Re-emit the Python worker's JSON log lines through `tracing`, so one log stream carries the request id from
/// the HTTP request to the Python process. A line that is not JSON is kept as a message.
pub(crate) fn relay_worker_log(stderr: &[u8], request_id: &str) {
    for line in String::from_utf8_lossy(stderr).lines() {
        relay_line(line, request_id);
    }
}

pub(crate) fn relay_line(line: &str, request_id: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(v) if v.is_object() => {
            let event = v.get("event").and_then(Value::as_str).unwrap_or("log");
            let rid = v.get("request_id").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(request_id);
            let level = v.get("level").and_then(Value::as_str).unwrap_or("info");
            let pid = v.get("pid").and_then(Value::as_u64).unwrap_or(0);
            let msg = v.get("message").and_then(Value::as_str).unwrap_or("");
            let worker_ms = v.get("worker_ms").and_then(Value::as_f64).unwrap_or(0.0);
            if level == "warn" || level == "error" {
                warn!(target: "docx_worker", request_id = %rid, event, pid, worker_ms, detail = msg, "python docx worker");
            } else {
                info!(target: "docx_worker", request_id = %rid, event, pid, worker_ms, detail = msg, "python docx worker");
            }
        }
        _ => warn!(target: "docx_worker", request_id = %request_id, detail = line, "python docx worker (unstructured line)"),
    }
}
