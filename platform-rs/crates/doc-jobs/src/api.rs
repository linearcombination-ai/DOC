//! The HTTP edge of the job path: `POST /documents_docx`, `POST /documents`, `GET /task_status/{id}`,
//! `GET /health/status` (extended), `GET /metrics`. Bodies follow the legacy shapes; additions are fields the
//! old clients ignore (`result` on FAILURE, extra keys on health) or headers (`X-Request-Id`, `X-Doc-Coalesced`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use rand::RngExt;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::limits::{check_request, Limits};
use crate::metrics::{self, Snapshot, WorkerView};
use crate::model::JobView;
use crate::pipeline::{prepare, KIND_DOCX, KIND_HTML};
use crate::store::{JobStore, NewJob, QueueHealth};

pub struct JobsApi<S: JobStore> {
    pub store: Arc<S>,
    pub in_flight_ttl: Duration,
    pub store_kind: &'static str,
    pub limits: Limits,
    pub output_dir: PathBuf,
    /// Whether `POST /documents` with `generate_pdf` can be served (it is not, until the PDF seam exists).
    pub pdf_enabled: bool,
    /// A process whose last report is older than this is down.
    pub alive_within: Duration,
    pub started: Instant,
}

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

pub fn new_task_id() -> String {
    let mut b: [u8; 16] = rand::rng().random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn clean_request_id(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))).then(|| v.to_string())
}

/// Middleware: take `X-Request-Id` from the caller (or make one), carry it to the handlers and the job, echo it
/// in the response, and write one log line per request.
pub async fn request_id_mw(mut req: Request, next: Next) -> Response {
    let id = req.headers().get("x-request-id").and_then(|v| v.to_str().ok()).and_then(clean_request_id).unwrap_or_else(|| new_task_id());
    req.extensions_mut().insert(RequestId(id.clone()));
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let t = Instant::now();
    let mut resp = next.run(req).await;
    if let Ok(v) = HeaderValue::from_str(&id) {
        resp.headers_mut().insert("x-request-id", v);
    }
    if path != "/metrics" && path != "/health/status" {
        info!(request_id = %id, method = %method, path = %path, status = resp.status().as_u16(), ms = t.elapsed().as_millis() as u64, "request");
    }
    resp
}

fn detail(status: StatusCode, msg: &str, code: &str, retryable: bool) -> Response {
    let mut r = (status, Json(json!({"detail": msg, "code": code, "retryable": retryable}))).into_response();
    if retryable {
        r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    }
    r
}

async fn submit<S: JobStore>(api: Arc<JobsApi<S>>, rid: String, body: Bytes, kind: &'static str) -> Response {
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"detail": [{"loc": ["body"], "msg": format!("JSON decode error: {e}"), "type": "json_invalid"}]}))).into_response(),
    };
    let mut problems: Vec<Value> = check_request(&raw, &api.limits);
    let prepared = match prepare(&raw) {
        Ok(p) => Some(p),
        Err(violations) => {
            problems.extend(violations.iter().map(|v| json!({"loc": ["body"], "msg": v.message, "type": format!("{:?}", v.code)})));
            None
        }
    };
    let Some(prepared) = prepared.filter(|_| problems.is_empty()) else {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"detail": problems}))).into_response();
    };
    if kind == KIND_HTML && prepared.valid.generate_pdf && !api.pdf_enabled {
        return detail(StatusCode::NOT_IMPLEMENTED, "PDF output is not implemented in this service yet; send generate_pdf=false for the HTML document.", "pdf_not_implemented", false);
    }
    // Disk guard: refuse new work, with a retryable answer, while the output filesystem is nearly full.
    let free = crate::output::Dirs::new(api.output_dir.clone()).free_bytes();
    if free.is_some_and(|f| f < api.limits.min_free_bytes) {
        warn!(request_id = %rid, free_mb = free.unwrap_or(0) / 1_048_576, "refusing a request: the output filesystem is nearly full");
        return detail(StatusCode::SERVICE_UNAVAILABLE, "The server is short of disk space for new documents; please try again shortly.", "disk_space_low", true);
    }
    let job = NewJob { task_id: new_task_id(), dedup_key: format!("{kind}:{}", prepared.key), kind: kind.to_string(), payload: raw.to_string(), request_id: rid.clone() };
    match api.store.enqueue(job, api.in_flight_ttl).await {
        Ok(e) => {
            info!(request_id = %rid, task_id = %e.task_id, key = %prepared.key, kind, coalesced = e.coalesced, "document request accepted");
            let mut r = Json(json!({"task_id": e.task_id})).into_response();
            if e.coalesced {
                r.headers_mut().insert("x-doc-coalesced", HeaderValue::from_static("true"));
            }
            r
        }
        Err(e) => {
            warn!(request_id = %rid, error = %e, "queue unavailable at enqueue");
            detail(StatusCode::SERVICE_UNAVAILABLE, "The job queue is unavailable; please try again in a moment.", "queue_unavailable", true)
        }
    }
}

async fn post_documents_docx<S: JobStore>(State(api): State<Arc<JobsApi<S>>>, Extension(rid): Extension<RequestId>, body: Bytes) -> Response {
    submit(api, rid.0, body, KIND_DOCX).await
}

async fn post_documents<S: JobStore>(State(api): State<Arc<JobsApi<S>>>, Extension(rid): Extension<RequestId>, body: Bytes) -> Response {
    submit(api, rid.0, body, KIND_HTML).await
}

async fn get_task_status<S: JobStore>(State(api): State<Arc<JobsApi<S>>>, Path(id): Path<String>) -> Response {
    match api.store.status(&id).await {
        // Celery reports PENDING for an id it has never heard of; so does this.
        Ok(None) => Json(json!({"state": crate::model::PENDING})).into_response(),
        Ok(Some(view)) => Json(JobView::body(&view)).into_response(),
        Err(e) => {
            warn!(error = %e, "queue unavailable at status");
            detail(StatusCode::SERVICE_UNAVAILABLE, "The job queue is unavailable; please keep polling.", "queue_unavailable", true)
        }
    }
}

fn now_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn workers_view<S: JobStore>(api: &JobsApi<S>) -> Vec<WorkerView> {
    api.store.workers().await.unwrap_or_default().into_iter().map(|(id, age_ms, info)| WorkerView { id, age_ms, info: serde_json::from_str(&info).unwrap_or(Value::Null) }).collect()
}

/// `[{"status":"ok", ...}, 200]` is what DOC's FastAPI serialises (a Python tuple); the first element carries
/// more: Redis, the queue and its lag, the processes and their pools, the last catalog fetch, disk. `status`
/// stays `ok` while the process answers (liveness); `ready` and `problems` say whether it can do its job.
async fn get_health<S: JobStore>(State(api): State<Arc<JobsApi<S>>>) -> Response {
    let t = Instant::now();
    let health = api.store.health().await;
    let ping_ms = t.elapsed().as_millis() as u64;
    let mut problems: Vec<String> = Vec::new();
    let queue = match &health {
        Ok(h) => json!({"ok": true, "store": api.store_kind, "ping_ms": ping_ms, "queued": h.queued, "running": h.running, "delayed": h.delayed, "stalled_recovered_total": h.stalled_recovered_total, "oldest_entry_age_ms": h.oldest_queued_age_ms, "lag_s": h.oldest_queued_age_ms as f64 / 1000.0}),
        Err(e) => {
            problems.push(format!("the job store is unreachable: {e}"));
            json!({"ok": false, "store": api.store_kind, "error": e.to_string()})
        }
    };
    let workers = workers_view(&api).await;
    let alive_ms = u64::try_from(api.alive_within.as_millis()).unwrap_or(u64::MAX);
    let procs: Vec<Value> = workers.iter().map(|w| json!({"id": w.id, "role": w.info.get("role"), "age_ms": w.age_ms, "alive": w.age_ms <= alive_ms, "docx": w.info.get("docx"), "catalog": w.info.get("catalog"), "running_jobs": w.info.get("running_jobs"), "uptime_s": w.info.get("uptime_s")})).collect();
    let live_workers = workers.iter().filter(|w| w.age_ms <= alive_ms && w.info.get("role").and_then(Value::as_str).is_some_and(|r| r != "api")).count();
    if health.is_ok() && live_workers == 0 {
        problems.push("no live worker process is reporting".to_string());
    }
    let last_ok = workers.iter().filter(|w| w.age_ms <= alive_ms).filter_map(|w| w.info.pointer("/catalog/last_ok_unix_s").and_then(Value::as_u64)).max().unwrap_or(0);
    let catalog = json!({"last_success_unix_s": last_ok, "age_s": if last_ok == 0 { Value::Null } else { json!(now_s().saturating_sub(last_ok)) }});
    let free = crate::output::Dirs::new(api.output_dir.clone()).free_bytes();
    let disk_ok = free.is_none_or(|f| f >= api.limits.min_free_bytes);
    if !disk_ok {
        problems.push("the output filesystem is nearly full".to_string());
    }
    let disk = json!({"free_mb": free.map(|f| f / 1_048_576), "min_free_mb": api.limits.min_free_bytes / 1_048_576, "ok": disk_ok});
    Json(json!([{"status": "ok", "ready": problems.is_empty(), "problems": problems, "queue": queue, "redis": {"ok": health.is_ok(), "ping_ms": ping_ms}, "processes": procs, "catalog": catalog, "disk": disk, "uptime_s": api.started.elapsed().as_secs()}, 200])).into_response()
}

async fn get_metrics<S: JobStore>(State(api): State<Arc<JobsApi<S>>>) -> Response {
    let counters = api.store.metrics().await;
    let health = api.store.health().await;
    let up = counters.is_ok() && health.is_ok();
    let workers = workers_view(&api).await;
    let counters = counters.unwrap_or_default();
    let queue: QueueHealth = health.unwrap_or_default();
    let mut text = metrics::render(&Snapshot { counters: &counters, queue: &queue, workers: &workers, alive_within_ms: u64::try_from(api.alive_within.as_millis()).unwrap_or(u64::MAX), now_unix_s: now_s() });
    text.push_str("# HELP doc_store_up 1 when the job store answered this scrape.\n# TYPE doc_store_up gauge\n");
    text.push_str(&format!("doc_store_up {}\n", u8::from(up)));
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], text).into_response()
}

pub fn routes<S: JobStore>(api: Arc<JobsApi<S>>) -> Router {
    let limit = api.limits.max_body_bytes;
    Router::new()
        .route("/documents_docx", post(post_documents_docx::<S>))
        .route("/documents", post(post_documents::<S>))
        .route("/task_status/{task_id}", get(get_task_status::<S>))
        .route("/health/status", get(get_health::<S>))
        .route("/metrics", get(get_metrics::<S>))
        .layer(DefaultBodyLimit::max(limit))
        .with_state(api)
}

/// Only the operational routes (a worker-only process serves these on `DOC_JOBD_OPS_ADDR`).
pub fn ops_routes<S: JobStore>(api: Arc<JobsApi<S>>) -> Router {
    Router::new().route("/health/status", get(get_health::<S>)).route("/metrics", get(get_metrics::<S>)).with_state(api)
}
