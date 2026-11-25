//! The core's CPU stages (parse the resource files, assemble the parts) as a unit that can run in a child
//! process: `doc-jobd core-stage IN.json OUT.json`. A hung or runaway core call is then killable (the whole
//! process group, on a deadline or a cancel), where a thread on the blocking pool can only be abandoned.
//! The child reads the repositories from the directories the acquirer put on disk; only the request, the
//! directory list and the result cross the process boundary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use doc_acquire::DiskRepo;
use doc_lookup::reader::RepoReader;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::warn;

use crate::model::JobError;
use crate::pipeline::{assemble, parse, prepare, RepoInfo};
use crate::runner::JobCtx;
use crate::seam::{read_capped, tail, GroupGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreMode {
    /// A child process of this binary (killable). The default.
    Child,
    /// On the blocking pool of this process (not killable; for tests and the Redis-less mode).
    Inline,
}

#[derive(Clone, Debug)]
pub struct CoreConfig {
    pub mode: CoreMode,
    /// The binary to run with `core-stage`; `None` = this executable.
    pub exe: Option<PathBuf>,
    /// Ceiling of one core stage in the child.
    pub deadline: Duration,
    /// Extra environment (fault injection in tests: `DOC_CORE_FAULT`).
    pub env: Vec<(String, String)>,
}

impl CoreConfig {
    pub fn production() -> CoreConfig {
        CoreConfig { mode: CoreMode::Child, exe: None, deadline: Duration::from_secs(600), env: Vec::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreItem {
    pub url: String,
    pub segment: String,
    pub lang_name: String,
    /// The version directory of the repository on disk.
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreInput {
    pub request: Value,
    /// One per resource request, in order.
    pub items: Vec<CoreItem>,
    pub structure: String,
    pub generate_docx: bool,
    #[serde(default)]
    pub html_shapes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreOutput {
    pub ok: bool,
    #[serde(default)]
    pub parts: Value,
    #[serde(default)]
    pub findings: Value,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub permanent: bool,
}

impl CoreOutput {
    fn failure(e: &JobError) -> CoreOutput {
        let f = e.failure();
        CoreOutput { ok: false, parts: Value::Null, findings: Value::Null, code: f.code, message: f.message, permanent: !f.retryable }
    }
}

/// Parse and assemble from readers (the same code in the child and inline).
pub fn run_core_with_readers(request: &Value, infos: &[RepoInfo], readers: &HashMap<String, Arc<dyn RepoReader>>, structure: &str, generate_docx: bool, html_shapes: bool) -> Result<(Value, Value), JobError> {
    let p = prepare(request).map_err(|v| JobError::permanent("invalid_request", v.iter().map(|x| x.message.clone()).collect::<Vec<_>>().join("; ")))?;
    let books = parse(&p, infos, readers, generate_docx, html_shapes)?;
    assemble(&p, books, structure)
}

/// Test hook, never set in production: make the core hang or crash for the first N runs.
fn fault_due() -> Option<String> {
    let fault = std::env::var("DOC_CORE_FAULT").ok().filter(|f| !f.is_empty())?;
    let times: u32 = std::env::var("DOC_CORE_FAULT_TIMES").ok().and_then(|t| t.parse().ok()).unwrap_or(u32::MAX);
    let seen: u32 = match std::env::var("DOC_CORE_FAULT_STATE") {
        Ok(path) => {
            let n = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let _ = std::fs::write(&path, (n + 1).to_string());
            n
        }
        Err(_) => 0,
    };
    (seen < times).then_some(fault)
}

/// The child's entry point: `core-stage IN OUT`. Exit 0 with an `ok` answer or a typed failure in OUT; a
/// crash or a kill leaves no OUT and the parent classifies it.
pub fn child_main(input: &Path, output: &Path) -> i32 {
    if let Some(f) = fault_due() {
        match f.as_str() {
            "hang" => {
                if let Ok(path) = std::env::var("DOC_CORE_PIDFILE") {
                    let _ = std::fs::write(path, std::process::id().to_string());
                }
                loop {
                    std::thread::sleep(Duration::from_secs(3600));
                }
            }
            "crash" => std::process::abort(),
            _ => {}
        }
    }
    let out = match std::fs::read(input).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<CoreInput>(&b).map_err(|e| e.to_string())) {
        Ok(inp) => {
            let infos: Vec<RepoInfo> = inp.items.iter().map(|i| RepoInfo { url: i.url.clone(), segment: i.segment.clone(), lang_name: i.lang_name.clone() }).collect();
            let mut readers: HashMap<String, Arc<dyn RepoReader>> = HashMap::new();
            for i in &inp.items {
                readers.entry(i.segment.clone()).or_insert_with(|| Arc::new(DiskRepo::new(i.dir.clone())));
            }
            match run_core_with_readers(&inp.request, &infos, &readers, &inp.structure, inp.generate_docx, inp.html_shapes) {
                Ok((parts, findings)) => CoreOutput { ok: true, parts, findings, code: String::new(), message: String::new(), permanent: false },
                Err(e) => CoreOutput::failure(&e),
            }
        }
        Err(e) => CoreOutput::failure(&JobError::permanent("core_input", e)),
    };
    match serde_json::to_vec(&out).map_err(|e| e.to_string()).and_then(|b| std::fs::write(output, b).map_err(|e| e.to_string())) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cannot write the core output: {e}");
            3
        }
    }
}

/// Run the core stage in a child process under a deadline and the job's cancel signal. The whole process group
/// is killed and reaped on every exit path.
pub async fn run_core_child(cfg: &CoreConfig, input: &CoreInput, dir: &Path, ctx: Option<&JobCtx>) -> Result<(Value, Value), JobError> {
    let exe = match &cfg.exe {
        Some(p) => p.clone(),
        None => std::env::current_exe().map_err(|e| JobError::transient("core_spawn", format!("cannot find this executable: {e}")))?,
    };
    let (inp, outp) = (dir.join("core-in.json"), dir.join("core-out.json"));
    let bytes = serde_json::to_vec(input).map_err(|e| JobError::permanent("core_encode", e.to_string()))?;
    tokio::fs::write(&inp, &bytes).await.map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", inp.display())))?;
    let _ = tokio::fs::remove_file(&outp).await;
    let mut cmd = Command::new(&exe);
    cmd.arg("core-stage").arg(&inp).arg(&outp);
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true).process_group(0);
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| JobError::transient("core_spawn", format!("{}: {e}", exe.display())))?;
    let pid = child.id().unwrap_or(0);
    let mut guard = GroupGuard { pgid: i32::try_from(pid).ok().filter(|p| *p > 0) };
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
            Ok(Err(e)) => Err(JobError::transient("core_wait", e.to_string())),
            Err(_) => Err(JobError::transient("core_timeout", format!("the core (pid {pid}) did not finish in {} s and was killed", cfg.deadline.as_secs()))),
        },
        _ = cancelled => Err(JobError::transient("core_cancelled", format!("the job was cancelled; core (pid {pid}) killed"))),
    };
    let status = match status {
        Ok(s) => {
            guard.disarm();
            s
        }
        Err(e) => {
            guard.kill();
            let _ = child.wait().await;
            warn!(pid, error = %e, "core stage killed (process group)");
            return Err(e);
        }
    };
    let _ = killpg(Pid::from_raw(i32::try_from(pid).unwrap_or(0)), Signal::SIGKILL);
    let stderr = match err {
        Some(h) => h.await.unwrap_or_default(),
        None => Vec::new(),
    };
    if status.code().is_none() {
        return Err(JobError::transient("core_crashed", format!("the core (pid {pid}) died from a signal: {}", tail(&stderr))));
    }
    let data = tokio::fs::read(&outp).await.map_err(|e| JobError::transient("core_failed", format!("the core (pid {pid}) exited {:?} without an answer ({e}): {}", status.code(), tail(&stderr))))?;
    let out: CoreOutput = serde_json::from_slice(&data).map_err(|e| JobError::transient("core_protocol", format!("unreadable core answer: {e}")))?;
    tracing::debug!(pid, ms = started.elapsed().as_millis() as u64, "core stage finished");
    if out.ok {
        Ok((out.parts, out.findings))
    } else if out.permanent {
        Err(JobError::permanent(leak_code(&out.code), out.message))
    } else {
        Err(JobError::transient(leak_code(&out.code), out.message))
    }
}

/// Error codes are `&'static str`; the child's code is mapped back to the known ones (anything else is `core_failed`).
fn leak_code(code: &str) -> &'static str {
    const KNOWN: [&str; 12] = ["invalid_request", "book_not_found", "usfm_unreadable", "resource_unreadable", "core_output", "core_rejected", "repo_unavailable", "core_input", "internal_panic", "resource_not_found", "core_failed", "unsupported_request"];
    KNOWN.iter().find(|k| **k == code).copied().unwrap_or("core_failed")
}

pub fn infos_json(infos: &[RepoInfo]) -> Value {
    json!(infos.iter().map(|i| json!({"url": i.url, "segment": i.segment})).collect::<Vec<_>>())
}
