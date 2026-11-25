//! `doc-jobd`: the Rust shell with the job runner. One binary, role from `DOC_JOBD_ROLE`: `all` (API and
//! worker, default), `api` (HTTP only), `worker` (runner only). The API enqueues, the worker runs; they meet only
//! in the store, so either can be restarted, scaled or killed without the other noticing.
//!
//! Subcommands: `core-stage IN OUT` (the killable core, started by the worker itself) and `check-config`
//! (validate the environment, print a summary, exit 0 or 2).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use doc_acquire::GitAcquirer;
use doc_app::App;
use doc_catalog::{CatalogStats, HttpCatalog};
use doc_jobs::api::{ops_routes, request_id_mw, routes, JobsApi};
use doc_jobs::config::{DocxMode, JobdConfig, Role, StoreKind};
use doc_jobs::lock::{MemLock, RedisLock};
use doc_jobs::mem_store::MemStore;
use doc_jobs::metrics;
use doc_jobs::pipeline::DocPipeline;
use doc_jobs::pool::{DocxBackend, DocxPool};
use doc_jobs::redis_store::{redact, RedisStore};
use doc_jobs::runner::Runner;
use doc_jobs::store::JobStore;
use doc_kitchen::KitchenCanon;
use doc_ports::RepoLock;
use doc_shell::config::Config;
use doc_shell::{build_with, Shared};
use serde_json::json;
use tokio::sync::watch;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;


async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut term = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "cannot install the SIGTERM handler");
            let _ = ctrl_c.await;
            return;
        }
    };
    tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    info!("shutdown requested");
}

/// Everything a running process reports about itself, and the deltas of its local counters.
struct Beat {
    id: String,
    role: Role,
    started: Instant,
    catalog: Arc<CatalogStats>,
    docx: Option<Arc<DocxBackend>>,
    clones: Arc<dyn Fn() -> u64 + Send + Sync>,
    running: Arc<dyn Fn() -> usize + Send + Sync>,
}

#[derive(Default)]
struct Last {
    refreshes: u64,
    failures: u64,
    clones: u64,
    restarts: [u64; 4],
}

impl Beat {
    fn info(&self) -> String {
        let c = &self.catalog;
        json!({
            "role": self.role.as_str(), "id": self.id, "pid": std::process::id(), "uptime_s": self.started.elapsed().as_secs(), "version": env!("CARGO_PKG_VERSION"),
            "catalog": {"last_ok_unix_s": c.last_ok_unix_s.load(Ordering::SeqCst), "refreshes": c.refreshes.load(Ordering::SeqCst), "failures": c.failures.load(Ordering::SeqCst)},
            "docx": self.docx.as_ref().map(|d| d.info()),
            "pool": self.docx.as_ref().and_then(|d| d.pool()).map(|p| p.info()),
            "clones": (self.clones)(), "running_jobs": (self.running)(),
        })
        .to_string()
    }

    async fn run<S: JobStore>(self, store: Arc<S>, every: Duration, mut sd: watch::Receiver<bool>) {
        let mut last = Last::default();
        loop {
            if let Err(e) = store.worker_beat(&self.id, &self.info(), every * 3).await {
                warn!(error = %e, "cannot report liveness to the store");
            }
            // flush the deltas of the process-local counters into the cluster-wide metrics
            let mut items: Vec<(String, u64)> = Vec::new();
            let (r, f, cl) = (self.catalog.refreshes.load(Ordering::SeqCst), self.catalog.failures.load(Ordering::SeqCst), (self.clones)());
            items.push((metrics::counter("doc_catalog_refreshes_total", &[]), r - last.refreshes));
            items.push((metrics::counter("doc_catalog_refresh_failures_total", &[]), f - last.failures));
            items.push((metrics::counter("doc_repo_clones_total", &[]), cl - last.clones));
            (last.refreshes, last.failures, last.clones) = (r, f, cl);
            if let Some(p) = self.docx.as_ref().and_then(|d| d.pool()) {
                let s = &p.stats;
                let now = [s.restart_crash.load(Ordering::SeqCst), s.restart_deadline.load(Ordering::SeqCst), s.restart_recycle.load(Ordering::SeqCst), s.restart_memory.load(Ordering::SeqCst)];
                for (i, reason) in ["crash", "deadline", "recycle", "memory"].iter().enumerate() {
                    items.push((metrics::counter("doc_docx_pool_restarts_total", &[("reason", reason)]), now[i] - last.restarts[i]));
                }
                last.restarts = now;
            }
            items.retain(|(_, n)| *n > 0);
            let _ = store.metric_add(&items).await;
            tokio::select! { _ = tokio::time::sleep(every) => {}, _ = sd.changed() => {} }
            if *sd.borrow() {
                return;
            }
        }
    }
}

async fn serve(router: axum::Router, addr: &str, mut sd: watch::Receiver<bool>, what: &'static str) -> Result<tokio::task::JoinHandle<()>, std::process::ExitCode> {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            error!(addr, error = %err, what, "cannot bind");
            return Err(std::process::ExitCode::from(2));
        }
    };
    info!(addr, what, "listening");
    Ok(tokio::spawn(async move {
        let shutdown = async move {
            while !*sd.borrow() {
                if sd.changed().await.is_err() {
                    break;
                }
            }
        };
        if let Err(err) = axum::serve(listener, router).with_graceful_shutdown(shutdown).await {
            error!(error = %err, what, "server error");
        }
    }))
}

async fn run<S: JobStore>(store: Arc<S>, store_kind: &'static str, cfg: JobdConfig, config: Config, lock: Option<Arc<dyn RepoLock>>) -> std::process::ExitCode {
    let catalog = match HttpCatalog::new(config.catalog.clone()) {
        Ok(c) => c,
        Err(err) => {
            error!(error = %err, "cannot build the HTTP client");
            return std::process::ExitCode::from(2);
        }
    };
    let catalog_stats = catalog.stats();
    let acquirer = match lock {
        Some(l) => GitAcquirer::with_lock(config.git.clone(), l),
        None => GitAcquirer::new(config.git.clone()),
    };
    let clones = acquirer.clones_run();
    let app = Arc::new(App::new(catalog, acquirer, Arc::new(KitchenCanon), config.app.clone()));
    let (sd_tx, sd_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = sd_tx.send(true);
    });
    let mut tasks = tokio::task::JoinSet::new();
    let worker_id = cfg.runner.worker_id.clone();
    let mut docx: Option<Arc<DocxBackend>> = None;
    let mut running: Arc<dyn Fn() -> usize + Send + Sync> = Arc::new(|| 0);
    let mut pool_for_shutdown: Option<Arc<DocxPool>> = None;
    if cfg.role.runs_worker() {
        let pc = cfg.pipeline.clone();
        // Scratch of dead attempts: older than two job ceilings cannot belong to a live attempt.
        let swept = pc.dirs.sweep_scratch(cfg.runner.job_ceiling * 2);
        info!(swept, output = %pc.dirs.output.display(), "worker starting; scratch of dead attempts removed");
        let backend = match cfg.docx_mode {
            DocxMode::PerJob => Arc::new(DocxBackend::PerJob(cfg.seam.clone())),
            DocxMode::Remote => match cfg.remote.clone() {
                Some(r) => Arc::new(DocxBackend::Remote(r, doc_jobs::remote::client())),
                None => {
                    error!("DOC_PYWORKER_URL is required for DOC_DOCX_MODE=remote");
                    return std::process::ExitCode::from(2);
                }
            },
            DocxMode::Persistent => {
                let pool = DocxPool::new(cfg.pool.clone());
                pool.warm().await;
                pool_for_shutdown = Some(pool.clone());
                Arc::new(DocxBackend::Pool(pool))
            }
        };
        docx = Some(backend.clone());
        let remote = cfg.remote.clone().map(|r| (r, doc_jobs::remote::client()));
        let handler = Arc::new(DocPipeline { app: app.clone(), cfg: pc.clone(), docx: backend, remote, store: store.clone() });
        let runner = Runner::new(store.clone(), handler, cfg.runner.clone());
        let r2 = runner.clone();
        running = Arc::new(move || r2.running_jobs());
        let sd = sd_rx.clone();
        tasks.spawn(async move { runner.run(sd).await });
        // the janitor: DOC's cache expiry (a document older than the freshness window plus a grace is deleted)
        let (dirs, expire_after, scratch_age, every) = (pc.dirs.clone(), pc.fresh_for + cfg.output_grace, cfg.runner.job_ceiling * 2, cfg.janitor_every);
        let mut sd = sd_rx.clone();
        tasks.spawn(async move {
            loop {
                tokio::select! { _ = tokio::time::sleep(every) => {}, _ = sd.changed() => {} }
                if *sd.borrow() {
                    return;
                }
                let (d, s) = (dirs.expire_outputs(expire_after), dirs.sweep_scratch(scratch_age));
                if d + s > 0 {
                    info!(expired_outputs = d, dead_scratch = s, "janitor removed files");
                }
            }
        });
    }
    let beat = Beat { id: worker_id.clone(), role: cfg.role, started: Instant::now(), catalog: catalog_stats, docx, clones, running };
    {
        let (st, sd, every) = (store.clone(), sd_rx.clone(), cfg.beat_every);
        tasks.spawn(async move { beat.run(st, every, sd).await });
    }
    let api = Arc::new(JobsApi {
        store: store.clone(),
        in_flight_ttl: cfg.runner.in_flight_ttl,
        store_kind,
        limits: cfg.pipeline.limits.clone(),
        output_dir: cfg.pipeline.dirs.output.clone(),
        pdf_enabled: cfg.pipeline.pdf.is_some(),
        alive_within: cfg.beat_every * 3,
        started: Instant::now(),
    });
    if cfg.role.runs_api() {
        let shared = Arc::new(Shared { app: app.clone(), default_mode: config.app.default_mode });
        let (router, _openapi) = build_with(shared, &config.cors_origins, Some(routes(api.clone())), Some(<doc_contract::jobs::JobsApiDoc as utoipa::OpenApi>::openapi()));
        let router = router.layer(axum::middleware::from_fn(request_id_mw));
        match serve(router, &config.addr, sd_rx.clone(), "api").await {
            Ok(h) => {
                tasks.spawn(async move {
                    let _ = h.await;
                });
            }
            Err(code) => return code,
        }
        info!(addr = %config.addr, role = cfg.role.as_str(), store = store_kind, worker = %worker_id, "doc-jobd listening");
    } else if let Some(addr) = &cfg.ops_addr {
        match serve(ops_routes(api.clone()), addr, sd_rx.clone(), "ops").await {
            Ok(h) => {
                tasks.spawn(async move {
                    let _ = h.await;
                });
            }
            Err(code) => return code,
        }
    }
    while tasks.join_next().await.is_some() {}
    if let Some(p) = pool_for_shutdown {
        p.shutdown().await;
    }
    info!("doc-jobd stopped");
    std::process::ExitCode::SUCCESS
}

fn summary(cfg: &JobdConfig, config: &Config) -> String {
    format!(
        "role={} store={:?} redis={} namespace={} addr={} output={} docx_mode={:?} core_mode={:?} concurrency={} lease_ms={} max_attempts={} drain_ms={} fresh_for_s={} max_body={} min_free_mb={} email_enabled={}",
        cfg.role.as_str(),
        cfg.store,
        redact(&cfg.redis.url),
        cfg.namespace,
        config.addr,
        cfg.pipeline.dirs.output.display(),
        cfg.docx_mode,
        cfg.pipeline.core.mode,
        cfg.runner.concurrency,
        cfg.runner.lease_ttl.as_millis(),
        cfg.runner.max_attempts,
        cfg.runner.drain_deadline.as_millis(),
        cfg.pipeline.fresh_for.as_secs(),
        cfg.pipeline.limits.max_body_bytes,
        cfg.pipeline.limits.min_free_bytes / 1_048_576,
        cfg.pipeline.email.as_ref().is_some_and(|e| e.enabled),
    )
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("core-stage") {
        let (Some(i), Some(o)) = (args.get(2), args.get(3)) else {
            eprintln!("usage: doc-jobd core-stage IN.json OUT.json");
            return std::process::ExitCode::from(2);
        };
        return std::process::ExitCode::from(u8::try_from(doc_jobs::core_stage::child_main(std::path::Path::new(i), std::path::Path::new(o))).unwrap_or(3));
    }
    let vars: HashMap<String, String> = std::env::vars().collect();
    // Both layers are validated and every problem is reported together, before anything is started.
    let shell = Config::from_vars(&vars);
    let jobd = JobdConfig::from_vars(&vars);
    let (config, cfg) = match (shell, jobd) {
        (Ok(a), Ok(b)) => (a, b),
        (a, b) => {
            let mut problems: Vec<String> = Vec::new();
            if let Err(e) = a {
                problems.extend(e.0);
            }
            if let Err(e) = b {
                problems.extend(e.0);
            }
            eprintln!("invalid configuration:\n  {}", problems.join("\n  "));
            return std::process::ExitCode::from(2);
        }
    };
    if args.get(1).map(String::as_str) == Some("print-openapi") {
        // the whole contract (the read endpoints, the findings routes, the job endpoints), from the real router
        let Ok(catalog) = HttpCatalog::new(config.catalog.clone()) else {
            eprintln!("cannot build the HTTP client");
            return std::process::ExitCode::from(1);
        };
        let app = Arc::new(App::new(catalog, GitAcquirer::new(config.git.clone()), Arc::new(KitchenCanon), config.app.clone()));
        let shared = Arc::new(Shared { app, default_mode: config.app.default_mode });
        let api = Arc::new(JobsApi { store: Arc::new(MemStore::new()), in_flight_ttl: cfg.runner.in_flight_ttl, store_kind: "memory", limits: cfg.pipeline.limits.clone(), output_dir: cfg.pipeline.dirs.output.clone(), pdf_enabled: cfg.pipeline.pdf.is_some(), alive_within: Duration::from_secs(15), started: Instant::now() });
        let (_router, doc) = build_with(shared, &[], Some(routes(api)), Some(<doc_contract::jobs::JobsApiDoc as utoipa::OpenApi>::openapi()));
        return match doc.to_pretty_json() {
            Ok(j) => {
                println!("{j}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("cannot serialise the OpenAPI document: {e}");
                std::process::ExitCode::from(1)
            }
        };
    }
    if args.get(1).map(String::as_str) == Some("check-config") {
        println!("configuration ok: {}", summary(&cfg, &config));
        return std::process::ExitCode::SUCCESS;
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let b = tracing_subscriber::fmt().with_env_filter(filter).with_target(true);
    if config.log_json {
        b.json().flatten_event(true).init();
    } else {
        b.init();
    }
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot start the async runtime: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    rt.block_on(async move {
        info!(config = %summary(&cfg, &config), "starting");
        match cfg.store {
            StoreKind::Memory => run(Arc::new(MemStore::new()), "memory", cfg.clone(), config, Some(MemLock::new(Duration::from_secs(30)) as Arc<dyn RepoLock>)).await,
            StoreKind::Redis => {
                // Redis may be down when the process starts (a deploy order, a restart): wait, do not die.
                let mut n = 0u32;
                let store = loop {
                    match RedisStore::connect_with(&cfg.redis, &cfg.namespace).await {
                        Ok(s) => break s,
                        Err(err) => {
                            n += 1;
                            error!(error = %err, attempt = n, redis = %redact(&cfg.redis.url), "cannot reach Redis; retrying");
                            tokio::time::sleep(Duration::from_secs(u64::from(n.min(10)))).await;
                        }
                    }
                };
                let lock: Option<Arc<dyn RepoLock>> = cfg.clone_lock.then(|| Arc::new(RedisLock::new(store.connection(), &cfg.namespace, &format!("{}.{}", cfg.runner.worker_id, std::process::id()), config.git.lock_lease)) as Arc<dyn RepoLock>);
                run(Arc::new(store), "redis", cfg.clone(), config, lock).await
            }
        }
    })
}
