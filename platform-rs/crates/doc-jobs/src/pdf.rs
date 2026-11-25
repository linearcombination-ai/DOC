//! PDF output through the Python seam (stretch goal of P6): `python/pdf_worker.py` takes the finished HTML file and
//! writes a PDF with WeasyPrint (default) or Prince, selected by `DOC_PDF_ENGINE`; the Rust side supervises it exactly
//! like the docx worker (own process group, deadline, cancel, reap, output validated before placement).
//! Prince is a licensed engine whose terms are the captain's call; this path calls the `prince` command if it is
//! installed and is **not exercised** on this host (no Prince binary).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;
use tracing::{info, warn};

use crate::model::JobError;
use crate::runner::JobCtx;
use crate::seam::{read_capped, relay_worker_log, tail, GroupGuard};

#[derive(Clone, Debug)]
pub struct PdfConfig {
    /// The interpreter that can `import weasyprint` (DOC's venv); may differ from the docx interpreter.
    pub python: PathBuf,
    pub script: PathBuf,
    /// `weasyprint` or `prince`.
    pub engine: String,
    pub deadline: Duration,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PdfJob {
    pub version: u32,
    pub job_id: String,
    pub request_id: String,
    pub input_html: PathBuf,
    pub output_path: PathBuf,
    pub engine: String,
}

/// At start: can the engine be used at all? An error is a configuration problem, not a runtime surprise.
pub fn probe(cfg: &PdfConfig) -> Result<(), String> {
    let out = match cfg.engine.as_str() {
        "weasyprint" => std::process::Command::new(&cfg.python).args(["-I", "-c", "import weasyprint"]).output(),
        "prince" => std::process::Command::new("prince").arg("--version").output(),
        other => return Err(format!("unknown PDF engine {other:?}")),
    };
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("the {} engine is not usable: {}", cfg.engine, String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or(""))),
        Err(e) => Err(format!("cannot run the {} engine check: {e}", cfg.engine)),
    }
}

pub async fn validate_pdf_file(path: &Path) -> Result<(), JobError> {
    let data = tokio::fs::read(path).await.map_err(|e| JobError::transient("pdf_output_missing", format!("{}: {e}", path.display())))?;
    let end = &data[data.len().saturating_sub(1024)..];
    if data.len() < 200 || !data.starts_with(b"%PDF-") || !end.windows(5).any(|w| w == b"%%EOF") {
        return Err(JobError::transient("pdf_output_invalid", format!("{} is not a complete PDF ({} bytes)", path.display(), data.len())));
    }
    Ok(())
}

pub struct PdfReport {
    pub pid: u32,
    pub wall: Duration,
    pub bytes: u64,
}

pub async fn run_pdf_worker(cfg: &PdfConfig, job: &PdfJob, job_file: &Path, ctx: Option<&JobCtx>) -> Result<PdfReport, JobError> {
    let bytes = serde_json::to_vec(job).map_err(|e| JobError::permanent("pdf_job_encode", e.to_string()))?;
    tokio::fs::write(job_file, &bytes).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", job_file.display())))?;
    let mut cmd = Command::new(&cfg.python);
    cmd.arg("-I").arg(&cfg.script).arg("--job").arg(job_file);
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).process_group(0);
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| JobError::transient("pdf_worker_spawn", format!("{}: {e}", cfg.python.display())))?;
    let pid = child.id().unwrap_or(0);
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
            Ok(Err(e)) => Err(JobError::transient("pdf_worker_wait", e.to_string())),
            Err(_) => Err(JobError::transient("pdf_worker_timeout", format!("the PDF worker (pid {pid}) did not finish in {} s and was killed", cfg.deadline.as_secs()))),
        },
        _ = cancelled => Err(JobError::transient("pdf_worker_cancelled", format!("the job was cancelled; PDF worker (pid {pid}) killed"))),
    };
    let status = match status {
        Ok(s) => {
            guard.disarm();
            s
        }
        Err(e) => {
            guard.kill();
            let _ = child.wait().await;
            warn!(pid, error = %e, "PDF worker killed (process group)");
            return Err(e);
        }
    };
    let _ = killpg(Pid::from_raw(i32::try_from(pid).unwrap_or(0)), Signal::SIGKILL);
    let stdout = match out {
        Some(h) => h.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let stderr = match err {
        Some(h) => h.await.unwrap_or_default(),
        None => Vec::new(),
    };
    relay_worker_log(&stderr, &job.request_id);
    match status.code() {
        Some(0) => {}
        Some(2) => return Err(JobError::permanent("pdf_input_rejected", format!("the PDF worker rejected the job: {}", tail(&stderr)))),
        Some(c) => return Err(JobError::transient("pdf_worker_failed", format!("the PDF worker exited {c}: {}", tail(&stderr)))),
        None => return Err(JobError::transient("pdf_worker_crashed", format!("the PDF worker (pid {pid}) died from a signal: {}", tail(&stderr)))),
    }
    let line = String::from_utf8_lossy(&stdout).lines().rev().find(|l| l.trim_start().starts_with('{')).map(str::to_string).unwrap_or_default();
    let v: Value = serde_json::from_str(&line).map_err(|e| JobError::transient("pdf_worker_protocol", format!("no JSON answer ({e}): {}", tail(&stdout))))?;
    validate_pdf_file(&job.output_path).await?;
    let wall = started.elapsed();
    info!(pid, wall_ms = wall.as_millis() as u64, engine = %job.engine, worker_ms = v.get("worker_ms").and_then(serde_json::Value::as_f64).unwrap_or(0.0), "pdf worker finished");
    Ok(PdfReport { pid, wall, bytes: v.get("bytes").and_then(serde_json::Value::as_u64).unwrap_or(0) })
}
