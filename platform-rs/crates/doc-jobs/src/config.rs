//! Configuration of the job service, read from the environment once, at start. Every problem is collected and
//! reported together (a name, the bad value, what was expected), and the process exits 2 before it binds a
//! port or touches Redis: a typo does not become a runtime surprise at the first request.
//!
//! DOC's own variable names are kept where DOC has them (`CELERY_BROKER_URL` as the Redis URL fallback,
//! `ASSET_CACHING_PERIOD` in minutes, `SEND_EMAIL`, `SMTP_*`, `FROM_EMAIL_ADDRESS`, `EMAIL_SEND_SUBJECT`);
//! the new ones are `DOC_JOBD_*` and `DOC_REDIS_*`. `docs/RUNBOOK.md` lists them all with their defaults.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::core_stage::{CoreConfig, CoreMode};
use crate::email::{EmailConfig, SmtpTls};
use crate::html::Templates;
use crate::limits::Limits;
use crate::output::Dirs;
use crate::pdf::PdfConfig;
use crate::pipeline::{PipelineConfig, StageDeadlines};
use crate::pool::PoolConfig;
use crate::redis_store::RedisOptions;
use crate::retry::Backoff;
use crate::runner::RunnerConfig;
use crate::seam::SeamConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    All,
    Api,
    Worker,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::All => "all",
            Role::Api => "api",
            Role::Worker => "worker",
        }
    }
    pub fn runs_worker(self) -> bool {
        matches!(self, Role::All | Role::Worker)
    }
    pub fn runs_api(self) -> bool {
        matches!(self, Role::All | Role::Api)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    Redis,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocxMode {
    PerJob,
    Persistent,
    /// A Python service in another container (`DOC_PYWORKER_URL`): the container deployment.
    Remote,
}

#[derive(Clone, Debug)]
pub struct JobdConfig {
    pub role: Role,
    pub store: StoreKind,
    pub redis: RedisOptions,
    pub namespace: String,
    pub runner: RunnerConfig,
    pub pipeline: PipelineConfig,
    pub seam: SeamConfig,
    pub docx_mode: DocxMode,
    pub pool: PoolConfig,
    /// The Python seam service, when `DOC_DOCX_MODE=remote` or `DOC_PDF_ENGINE=remote`.
    pub remote: Option<crate::remote::RemoteConfig>,
    /// How often a process reports itself to the store (workers, API nodes); `3 x` this is the liveness window.
    pub beat_every: Duration,
    /// Output files older than `fresh_for + output_grace` are deleted by the janitor.
    pub output_grace: Duration,
    pub janitor_every: Duration,
    /// Where a worker-only process serves `/metrics` and `/health/status` (the API role serves them itself).
    pub ops_addr: Option<String>,
    /// Share clones between worker processes through the store's lock (default on; off is the test control).
    pub clone_lock: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid configuration:\n  {}", .0.join("\n  "))]
pub struct JobdConfigError(pub Vec<String>);

struct Env<'a> {
    vars: &'a HashMap<String, String>,
    problems: Vec<String>,
}

impl Env<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
    }
    fn bad(&mut self, name: &str, v: &str, expected: &str) {
        self.problems.push(format!("{name}={v:?}: {expected}"));
    }
    /// A number in `lo..=hi`.
    fn num<T: FromStr + PartialOrd + std::fmt::Display + Copy>(&mut self, name: &str, default: T, lo: T, hi: T) -> T {
        match self.get(name).map(str::to_string) {
            None => default,
            Some(v) => match v.parse::<T>() {
                Ok(x) if x >= lo && x <= hi => x,
                Ok(_) => {
                    self.bad(name, &v, &format!("must be between {lo} and {hi}"));
                    default
                }
                Err(_) => {
                    self.bad(name, &v, "is not a number");
                    default
                }
            },
        }
    }
    fn ms(&mut self, name: &str, default: Duration, lo_ms: u64, hi_ms: u64) -> Duration {
        let d = u64::try_from(default.as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(self.num(name, d, lo_ms, hi_ms))
    }
    fn flag(&mut self, name: &str, default: bool) -> bool {
        match self.get(name).map(str::to_string) {
            None => default,
            Some(v) => match v.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                _ => {
                    self.bad(name, &v, "must be true or false");
                    default
                }
            },
        }
    }
    fn choice<'b>(&mut self, name: &str, default: &'b str, allowed: &[&'b str]) -> &'b str {
        match self.get(name).map(str::to_string) {
            None => default,
            Some(v) => match allowed.iter().find(|a| a.eq_ignore_ascii_case(&v)) {
                Some(a) => a,
                None => {
                    self.bad(name, &v, &format!("must be one of {}", allowed.join(", ")));
                    default
                }
            },
        }
    }
    fn existing_file(&mut self, name: &str) -> Option<PathBuf> {
        let v = self.get(name)?.to_string();
        let p = PathBuf::from(&v);
        if p.is_file() {
            Some(p)
        } else {
            self.bad(name, &v, "no such file");
            None
        }
    }
    /// A password given directly or through a file (the file keeps it out of the process list and the logs).
    fn secret(&mut self, name: &str) -> Option<String> {
        if let Some(path) = self.get(&format!("{name}_FILE")).map(str::to_string) {
            return match std::fs::read_to_string(&path) {
                Ok(s) => Some(s.trim_end_matches(['\n', '\r']).to_string()),
                Err(e) => {
                    self.bad(&format!("{name}_FILE"), &path, &format!("cannot be read: {e}"));
                    None
                }
            };
        }
        self.get(name).map(str::to_string)
    }
}

impl JobdConfig {
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<JobdConfig, JobdConfigError> {
        let mut e = Env { vars, problems: Vec::new() };
        let role = match e.choice("DOC_JOBD_ROLE", "all", &["all", "api", "worker"]) {
            "api" => Role::Api,
            "worker" => Role::Worker,
            _ => Role::All,
        };
        let store = if e.choice("DOC_JOBD_STORE", "redis", &["redis", "memory"]) == "memory" { StoreKind::Memory } else { StoreKind::Redis };
        if store == StoreKind::Memory && role != Role::All {
            e.problems.push("DOC_JOBD_STORE=memory needs DOC_JOBD_ROLE=all: the API and the worker must share one process".into());
        }

        // Redis: URL, credentials, TLS
        let url = e.get("DOC_JOBD_REDIS_URL").or_else(|| e.get("CELERY_BROKER_URL")).unwrap_or("redis://127.0.0.1:6379/0").to_string();
        if !(url.starts_with("redis://") || url.starts_with("rediss://") || url.starts_with("redis+unix://") || url.starts_with("unix://")) {
            e.bad("DOC_JOBD_REDIS_URL", &crate::redis_store::redact(&url), "must start with redis:// or rediss://");
        }
        let ca_file = e.existing_file("DOC_REDIS_CA_FILE");
        if ca_file.is_some() && !url.starts_with("rediss://") {
            e.problems.push("DOC_REDIS_CA_FILE is set but the Redis URL is not rediss:// (no TLS would be used)".into());
        }
        let redis = RedisOptions { url, username: e.get("DOC_REDIS_USERNAME").map(str::to_string), password: e.secret("DOC_REDIS_PASSWORD"), ca_file };
        let namespace = e.get("DOC_JOBD_NAMESPACE").unwrap_or("docjobs").to_string();
        if !namespace.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) {
            e.bad("DOC_JOBD_NAMESPACE", &namespace, "letters, digits, '-' and '_' only");
        }

        // runner
        let worker_id = e.get("DOC_JOBD_WORKER_ID").map(str::to_string).unwrap_or_else(|| format!("w{}", std::process::id()));
        let mut r = RunnerConfig::production(&worker_id);
        r.concurrency = e.num("DOC_JOBD_CONCURRENCY", r.concurrency, 1, 256);
        r.lease_ttl = e.ms("DOC_JOBD_LEASE_TTL_MS", r.lease_ttl, 300, 3_600_000);
        r.claim_block = e.ms("DOC_JOBD_CLAIM_BLOCK_MS", r.claim_block, 50, 60_000);
        r.promote_every = e.ms("DOC_JOBD_PROMOTE_EVERY_MS", r.promote_every, 50, 60_000);
        r.reap_every = e.ms("DOC_JOBD_REAP_EVERY_MS", r.reap_every, 50, 600_000);
        r.max_attempts = e.num("DOC_JOBD_MAX_ATTEMPTS", r.max_attempts, 1, 100);
        r.backoff = Backoff {
            base: e.ms("DOC_JOBD_BACKOFF_BASE_MS", r.backoff.base, 1, 3_600_000),
            cap: e.ms("DOC_JOBD_BACKOFF_CAP_MS", r.backoff.cap, 1, 86_400_000),
            floor: e.ms("DOC_JOBD_BACKOFF_FLOOR_MS", r.backoff.floor, 0, 600_000),
        };
        if r.backoff.cap < r.backoff.base {
            e.problems.push("DOC_JOBD_BACKOFF_CAP_MS must not be smaller than DOC_JOBD_BACKOFF_BASE_MS".into());
        }
        r.job_ceiling = e.ms("DOC_JOBD_JOB_CEILING_MS", r.job_ceiling, 1_000, 86_400_000);
        r.drain_deadline = e.ms("DOC_JOBD_DRAIN_DEADLINE_MS", r.drain_deadline, 0, 3_600_000);
        if r.reap_every >= r.lease_ttl {
            e.problems.push("DOC_JOBD_REAP_EVERY_MS must be shorter than DOC_JOBD_LEASE_TTL_MS (or a dead worker's job waits two lease periods)".into());
        }

        // pipeline
        let output = PathBuf::from(e.get("DOC_JOBD_OUTPUT_DIR").unwrap_or("document_output_rs"));
        if output.exists() && !output.is_dir() {
            e.bad("DOC_JOBD_OUTPUT_DIR", &output.display().to_string(), "exists and is not a directory");
        } else if let Err(err) = std::fs::create_dir_all(&output) {
            e.bad("DOC_JOBD_OUTPUT_DIR", &output.display().to_string(), &format!("cannot be created: {err}"));
        }
        // ASSET_CACHING_PERIOD is in minutes in DOC's .env
        let period_min: u64 = e.num("ASSET_CACHING_PERIOD", 30, 0, 100_000);
        let mut stages = StageDeadlines::production();
        stages.locate = e.ms("DOC_JOBD_STAGE_LOCATE_MS", stages.locate, 100, 86_400_000);
        stages.provision = e.ms("DOC_JOBD_STAGE_PROVISION_MS", stages.provision, 100, 86_400_000);
        stages.parse = e.ms("DOC_JOBD_STAGE_PARSE_MS", stages.parse, 100, 86_400_000);
        stages.assemble = e.ms("DOC_JOBD_STAGE_ASSEMBLE_MS", stages.assemble, 100, 86_400_000);
        let here = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
        let docx_mode = match e.choice("DOC_DOCX_MODE", "per_job", &["per_job", "persistent", "remote"]) {
            "persistent" => DocxMode::Persistent,
            "remote" => DocxMode::Remote,
            _ => DocxMode::PerJob,
        };
        let script = e.get("DOC_JOBD_DOCX_WORKER").map(PathBuf::from).unwrap_or_else(|| here.join("../../python/docx_worker.py"));
        let python = PathBuf::from(e.get("DOC_JOBD_PYTHON").unwrap_or("python3"));
        if role.runs_worker() && docx_mode != DocxMode::Remote && !script.is_file() {
            e.bad("DOC_JOBD_DOCX_WORKER", &script.display().to_string(), "the docx worker script does not exist");
        }
        let docx_deadline = e.ms("DOC_JOBD_DOCX_DEADLINE_MS", Duration::from_secs(300), 100, 86_400_000);
        let fault_env: Vec<(String, String)> = vars.iter().filter(|(k, _)| k.starts_with("DOCX_WORKER_FAULT") || k == &"DOCX_WORKER_PIDFILE").map(|(k, v)| (k.clone(), v.clone())).collect();
        let pool = PoolConfig {
            python: python.clone(),
            script: script.clone(),
            size: e.num("DOC_DOCX_POOL_SIZE", r.concurrency.max(1), 1, 64),
            deadline: docx_deadline,
            recycle_after: e.num("DOC_DOCX_RECYCLE_AFTER", 200, 1, 1_000_000),
            rss_ceiling_kb: e.num::<u64>("DOC_DOCX_RSS_CEILING_MB", 512, 0, 65_536) * 1024,
            env: fault_env.clone(),
        };
        let core_mode = if e.choice("DOC_CORE_MODE", "child", &["child", "inline"]) == "inline" { CoreMode::Inline } else { CoreMode::Child };
        let mut core = CoreConfig::production();
        core.mode = core_mode;
        core.deadline = e.ms("DOC_JOBD_CORE_DEADLINE_MS", core.deadline, 100, 86_400_000);
        core.env = vars.iter().filter(|(k, _)| k.starts_with("DOC_CORE_FAULT")).map(|(k, v)| (k.clone(), v.clone())).collect();
        let templates = Templates::new(PathBuf::from(e.get("DOC_TEMPLATES_DIR").map(str::to_string).unwrap_or_else(|| here.join("../../../backend/templates/html").display().to_string())));
        if role.runs_worker() {
            if let Err(m) = templates.check() {
                e.problems.push(format!("DOC_TEMPLATES_DIR: {m}"));
            }
        }
        let mut limits = Limits::production();
        limits.max_body_bytes = e.num("DOC_JOBD_MAX_BODY_BYTES", limits.max_body_bytes, 1024, 64 * 1024 * 1024);
        limits.max_resources = e.num("DOC_JOBD_MAX_RESOURCES", limits.max_resources, 1, 10_000);
        limits.min_free_bytes = e.num::<u64>("DOC_JOBD_MIN_FREE_MB", 512, 0, 1_000_000_000) * 1024 * 1024;

        // email: DOC's own variable names
        let smtp_tls = match e.choice("SMTP_TLS", "starttls", &["none", "starttls", "tls"]) {
            "none" => SmtpTls::None,
            "tls" => SmtpTls::Implicit,
            _ => SmtpTls::StartTls,
        };
        let enabled = e.flag("SEND_EMAIL", false);
        let email = EmailConfig {
            enabled,
            host: e.get("SMTP_HOST").unwrap_or("localhost").to_string(),
            port: e.num("SMTP_PORT", 587u16, 1, 65_535),
            tls: smtp_tls,
            username: e.get("SMTP_USER").map(str::to_string),
            password: e.secret("SMTP_PASSWORD"),
            from: e.get("FROM_EMAIL_ADDRESS").unwrap_or("foo@example.com").to_string(),
            subject: e.get("EMAIL_SEND_SUBJECT").unwrap_or("Your document").to_string(),
            ca_file: e.existing_file("SMTP_CA_FILE"),
            timeout: e.ms("DOC_JOBD_SMTP_TIMEOUT_MS", Duration::from_secs(30), 100, 600_000),
            body_template: None,
            record_ttl: Duration::from_secs(7 * 24 * 3600),
        };
        if enabled && email.username.is_some() != email.password.is_some() {
            e.problems.push("SMTP_USER and SMTP_PASSWORD must be set together".into());
        }
        if enabled && email.username.is_some() && smtp_tls == SmtpTls::None {
            e.problems.push("SMTP credentials are set but SMTP_TLS=none: they would be sent in clear".into());
        }
        let email_body_template = templates_text_email(&templates);
        // PDF: off unless an engine is named; an engine that cannot run is a start-up error, not a surprise at the first request
        let engine = e.choice("DOC_PDF_ENGINE", "none", &["none", "weasyprint", "prince", "remote"]).to_string();
        let engine_is_remote = engine == "remote";
        let pdf_python = PathBuf::from(e.get("DOC_PDF_PYTHON").unwrap_or_else(|| python.to_str().unwrap_or("python3")));
        let pdf_script = e.get("DOC_PDF_WORKER").map(PathBuf::from).unwrap_or_else(|| here.join("../../python/pdf_worker.py"));
        let pdf_deadline = e.ms("DOC_PDF_DEADLINE_MS", Duration::from_secs(600), 1000, 86_400_000);
        let pdf = if engine == "none" {
            None
        } else if engine == "remote" {
            // The container deployment: the engine (Prince, with WeasyPrint as the fallback) lives in the Python service.
            Some(PdfConfig { python: pdf_python, script: pdf_script, engine, deadline: pdf_deadline, env: Vec::new() })
        } else {
            let c = PdfConfig { python: pdf_python, script: pdf_script, engine, deadline: pdf_deadline, env: vars.iter().filter(|(k, _)| k.starts_with("PDF_WORKER_FAULT")).map(|(k, v)| (k.clone(), v.clone())).collect() };
            if role.runs_worker() {
                if !c.script.is_file() {
                    e.problems.push(format!("DOC_PDF_WORKER={}: the PDF worker script does not exist", c.script.display()));
                } else if let Err(m) = crate::pdf::probe(&c) {
                    e.problems.push(format!("DOC_PDF_ENGINE={}: {m} (set DOC_PDF_PYTHON to an interpreter that can import weasyprint)", c.engine));
                }
            }
            Some(c)
        };

        let remote = match e.get("DOC_PYWORKER_URL").map(|u| u.trim_end_matches('/').to_string()) {
            Some(u) if u.starts_with("http://") || u.starts_with("https://") => Some(crate::remote::RemoteConfig { base_url: u, docx_deadline, pdf_deadline }),
            Some(u) => {
                e.bad("DOC_PYWORKER_URL", &u, "must start with http:// or https://");
                None
            }
            None => None,
        };
        if role.runs_worker() && remote.is_none() && (docx_mode == DocxMode::Remote || engine_is_remote) {
            e.problems.push("DOC_PYWORKER_URL must be set when DOC_DOCX_MODE=remote or DOC_PDF_ENGINE=remote (for example http://pyworker:9100)".into());
        }
        let pipeline = PipelineConfig {
            dirs: Dirs::new(output),
            fresh_for: Duration::from_secs(period_min * 60),
            stages,
            structure: e.choice("DOC_JOBD_STRUCTURE", "proposed_look", &["proposed_look", "as_built"]).to_string(),
            dump_parts: e.get("DOC_JOBD_DUMP_PARTS").is_some(),
            core,
            templates,
            limits,
            email: Some(email),
            email_body_template,
            pdf,
        };
        let seam = SeamConfig { python, script, deadline: docx_deadline, env: fault_env };
        let beat_every = e.ms("DOC_JOBD_BEAT_EVERY_MS", Duration::from_secs(5), 100, 600_000);
        let output_grace = e.ms("DOC_JOBD_OUTPUT_GRACE_MS", Duration::from_secs(30 * 60), 0, 30 * 86_400_000);
        let janitor_every = e.ms("DOC_JOBD_JANITOR_EVERY_MS", Duration::from_secs(300), 100, 86_400_000);
        let ops_addr = e.get("DOC_JOBD_OPS_ADDR").map(str::to_string);
        let clone_lock = e.flag("DOC_JOBD_CLONE_LOCK", true);
        if e.problems.is_empty() {
            Ok(JobdConfig { role, store, redis, namespace, runner: r, pipeline, seam, docx_mode, pool, remote, beat_every, output_grace, janitor_every, ops_addr, clone_lock })
        } else {
            Err(JobdConfigError(e.problems))
        }
    }
}

fn templates_text_email(t: &Templates) -> Option<PathBuf> {
    t.dir().parent().map(|p| p.join("text").join("email.txt")).filter(|p| p.is_file())
}
