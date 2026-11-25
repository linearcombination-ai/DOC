//! The docx and PDF seam across a container boundary (P7).
//!
//! The local seam starts a Python process per job; here the Python side is its own container (`python/seam_server.py`)
//! that runs the very same scripts. The worker writes the job file on the shared volume exactly as before, POSTs the
//! path, and waits for the answer under the job's deadline and cancel signal. The service enforces the deadline by killing the
//! subprocess group; the client enforces it again with a margin, so a stuck service is also an ordinary transient failure.
//! Every failure maps onto the codes the local seam uses, so retry and status behaviour are unchanged: an unreachable or
//! restarting service is transient (the job is retried with backoff and ends with a document when the service is back), a
//! rejected job is permanent.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::model::JobError;
use crate::pdf::{validate_pdf_file, PdfJob, PdfReport};
use crate::runner::JobCtx;
use crate::seam::{tail, validate_docx_file, DocxJob, DocxResult, SeamReport};

/// Extra time over the job's own deadline before the client gives up on the HTTP call (the service kills at the deadline).
const CLIENT_MARGIN: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct RemoteConfig {
    /// `http://pyworker:9100`, no trailing slash.
    pub base_url: String,
    pub docx_deadline: Duration,
    pub pdf_deadline: Duration,
}

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    exit: Option<i32>,
    #[serde(default)]
    signal: Option<i32>,
    #[serde(default)]
    timed_out: bool,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    wall_ms: f64,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    fallback: bool,
}

/// One shared client: connection reuse to the service, no proxy, no redirects.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).pool_idle_timeout(Duration::from_secs(30)).connect_timeout(Duration::from_secs(5)).build().unwrap_or_default()
}

async fn post(client: &reqwest::Client, cfg: &RemoteConfig, route: &str, body: serde_json::Value, deadline: Duration, ctx: Option<&JobCtx>, label: &'static str) -> Result<Answer, JobError> {
    let url = format!("{}{}", cfg.base_url, route);
    let send = client.post(&url).timeout(deadline + CLIENT_MARGIN).json(&body).send();
    let cancelled = async {
        match ctx {
            Some(c) => c.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };
    let resp = tokio::select! {
        r = send => r.map_err(|e| JobError::transient(if e.is_timeout() { "pyworker_timeout" } else { "pyworker_unreachable" }, format!("{label} service {url}: {e}")))?,
        _ = cancelled => return Err(JobError::transient("pyworker_cancelled", format!("the job was cancelled while waiting for the {label} service"))),
    };
    let status = resp.status();
    let text = resp.text().await.map_err(|e| JobError::transient("pyworker_unreachable", format!("{label} service {url}: reading the answer: {e}")))?;
    match status.as_u16() {
        200 => serde_json::from_str::<Answer>(&text).map_err(|e| JobError::transient("pyworker_protocol", format!("{label} service answered 200 with a body that is not an answer ({e}): {}", tail(text.as_bytes())))),
        400 => Err(JobError::permanent("pyworker_bad_request", format!("the {label} service rejected the request (is the job volume shared at the same path?): {}", tail(text.as_bytes())))),
        s => Err(JobError::transient("pyworker_unavailable", format!("the {label} service answered {s}: {}", tail(text.as_bytes())))),
    }
}

/// A docx through the service. The job file is written here (the caller's scratch directory, on the shared volume).
pub async fn run_docx_remote(client: &reqwest::Client, cfg: &RemoteConfig, job: &DocxJob, job_file: &Path, ctx: Option<&JobCtx>) -> Result<SeamReport, JobError> {
    let bytes = serde_json::to_vec(job).map_err(|e| JobError::permanent("docx_job_encode", e.to_string()))?;
    tokio::fs::write(job_file, &bytes).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", job_file.display())))?;
    let started = Instant::now();
    let body = json!({"job_file": job_file, "deadline_ms": cfg.docx_deadline.as_millis() as u64, "request_id": job.request_id});
    let a = post(client, cfg, "/v1/docx", body, cfg.docx_deadline, ctx, "docx").await?;
    let wall = started.elapsed();
    if a.timed_out {
        warn!(pid = a.pid, "remote docx worker killed at its deadline");
        return Err(JobError::transient("docx_worker_timeout", format!("the docx worker did not finish in {} s and was killed by the service", cfg.docx_deadline.as_secs())));
    }
    match (a.exit, a.signal) {
        (Some(0), _) => {}
        (Some(2), _) => return Err(JobError::permanent("docx_input_rejected", format!("the docx worker rejected the job: {}", tail(a.stderr.as_bytes())))),
        (Some(c), _) => return Err(JobError::transient("docx_worker_failed", format!("the docx worker exited {c}: {}", tail(a.stderr.as_bytes())))),
        (None, s) => return Err(JobError::transient("docx_worker_crashed", format!("the docx worker died from signal {}: {}", s.unwrap_or(0), tail(a.stderr.as_bytes())))),
    }
    let line = a.stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).map(str::to_string).unwrap_or_default();
    let result: DocxResult = serde_json::from_str(&line).map_err(|e| JobError::transient("docx_worker_protocol", format!("no JSON answer on stdout ({e}): {}", tail(a.stdout.as_bytes()))))?;
    if !result.ok {
        return Err(JobError::transient("docx_worker_failed", "the docx worker reported ok=false".to_string()));
    }
    validate_docx_file(&job.output_path).await?;
    info!(pid = a.pid, wall_ms = wall.as_millis() as u64, worker_ms = result.worker_ms, service_ms = a.wall_ms, bytes = result.bytes, "docx worker finished (remote)");
    Ok(SeamReport { pid: a.pid, wall, result })
}

/// A PDF through the service. `engine` is the engine asked for; the service answers with the engine used (it falls back
/// from Prince to WeasyPrint when Prince is missing or fails, because a request must end with a PDF).
pub async fn run_pdf_remote(client: &reqwest::Client, cfg: &RemoteConfig, job: &PdfJob, job_file: &Path, ctx: Option<&JobCtx>) -> Result<(PdfReport, String, bool), JobError> {
    let bytes = serde_json::to_vec(job).map_err(|e| JobError::permanent("pdf_job_encode", e.to_string()))?;
    tokio::fs::write(job_file, &bytes).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", job_file.display())))?;
    let started = Instant::now();
    let body = json!({"job_file": job_file, "deadline_ms": cfg.pdf_deadline.as_millis() as u64, "request_id": job.request_id, "engine": job.engine});
    let a = post(client, cfg, "/v1/pdf", body, cfg.pdf_deadline * 2, ctx, "pdf").await?;
    let wall = started.elapsed();
    if a.timed_out {
        return Err(JobError::transient("pdf_worker_timeout", format!("the PDF worker did not finish in {} s and was killed by the service", cfg.pdf_deadline.as_secs())));
    }
    match (a.exit, a.signal) {
        (Some(0), _) => {}
        (Some(2), _) => return Err(JobError::permanent("pdf_input_rejected", format!("the PDF worker rejected the job: {}", tail(a.stderr.as_bytes())))),
        (Some(c), _) => return Err(JobError::transient("pdf_worker_failed", format!("the PDF worker exited {c}: {}", tail(a.stderr.as_bytes())))),
        (None, s) => return Err(JobError::transient("pdf_worker_crashed", format!("the PDF worker died from signal {}: {}", s.unwrap_or(0), tail(a.stderr.as_bytes())))),
    }
    validate_pdf_file(&job.output_path).await?;
    let engine = a.engine.clone().unwrap_or_else(|| job.engine.clone());
    let v: serde_json::Value = a.stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).and_then(|l| serde_json::from_str(l).ok()).unwrap_or(serde_json::Value::Null);
    let size = v.get("bytes").and_then(serde_json::Value::as_u64).unwrap_or(0);
    info!(pid = a.pid, wall_ms = wall.as_millis() as u64, engine = %engine, fallback = a.fallback, bytes = size, "pdf worker finished (remote)");
    Ok((PdfReport { pid: a.pid, wall, bytes: size }, engine, a.fallback))
}
