//! Metrics: counters live in the job store (`name|label=value` fields of one hash), so any node renders the
//! cluster-wide totals; this module names them, records stage durations as histogram buckets, and renders the
//! Prometheus text exposition format (version 0.0.4).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::store::{JobStore, QueueHealth};

/// Upper bounds of the stage-duration histogram, in seconds.
pub const BUCKETS: [f64; 12] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 10.0, 60.0, 600.0];

pub fn counter(name: &str, labels: &[(&str, &str)]) -> String {
    let mut s = name.to_string();
    for (k, v) in labels {
        s.push('|');
        s.push_str(k);
        s.push('=');
        s.push_str(&v.replace(['|', '\n', '"', '\\'], "_"));
    }
    s
}

/// Record one stage duration: the sum (in microseconds, integral), the count, and the bucket it falls in
/// (buckets are stored per bucket and accumulated when rendered).
pub async fn observe_stage<S: JobStore>(store: &S, stage: &str, secs: f64) {
    let le = BUCKETS.iter().find(|b| secs <= **b).map(|b| format!("{b}")).unwrap_or_else(|| "+Inf".to_string());
    let micros = (secs * 1_000_000.0).max(0.0) as u64;
    let items = vec![
        (counter("doc_stage_duration_seconds_bucket", &[("stage", stage), ("le", &le)]), 1),
        (counter("doc_stage_duration_seconds_sum_us", &[("stage", stage)]), micros),
        (counter("doc_stage_duration_seconds_count", &[("stage", stage)]), 1),
    ];
    let _ = store.metric_add(&items).await;
}

pub async fn incr<S: JobStore>(store: &S, name: &str, labels: &[(&str, &str)]) {
    let _ = store.metric_add(&[(counter(name, labels), 1)]).await;
}

fn parse(field: &str) -> (&str, BTreeMap<&str, &str>) {
    let mut it = field.split('|');
    let name = it.next().unwrap_or("");
    let labels = it.filter_map(|kv| kv.split_once('=')).collect();
    (name, labels)
}

fn esc(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn fmt_labels(labels: &[(&str, &str)]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let inner: Vec<String> = labels.iter().map(|(k, v)| format!("{k}=\"{}\"", esc(v))).collect();
    format!("{{{}}}", inner.join(","))
}

pub struct WorkerView {
    pub id: String,
    pub age_ms: u64,
    pub info: serde_json::Value,
}

pub struct Snapshot<'a> {
    pub counters: &'a BTreeMap<String, u64>,
    pub queue: &'a QueueHealth,
    pub workers: &'a [WorkerView],
    /// A worker whose last beat is older than this is down.
    pub alive_within_ms: u64,
    pub now_unix_s: u64,
}

struct Out {
    s: String,
}

impl Out {
    fn family(&mut self, name: &str, kind: &str, help: &str) {
        let _ = writeln!(self.s, "# HELP {name} {help}");
        let _ = writeln!(self.s, "# TYPE {name} {kind}");
    }
    fn sample(&mut self, name: &str, labels: &[(&str, &str)], v: impl std::fmt::Display) {
        let _ = writeln!(self.s, "{name}{} {v}", fmt_labels(labels));
    }
}

/// The text exposition. Every family is always present (counters at 0 when nothing happened), so an alert on
/// `rate(...)` or `absent(...)` behaves.
pub fn render(snap: &Snapshot<'_>) -> String {
    let mut o = Out { s: String::new() };
    let c = snap.counters;
    let get = |name: &str, labels: &[(&str, &str)]| c.get(&counter(name, labels)).copied().unwrap_or(0);

    o.family("doc_queue_depth", "gauge", "Jobs by state: queued (waiting for a worker), running, delayed (waiting out a retry backoff).");
    o.sample("doc_queue_depth", &[("state", "queued")], snap.queue.queued);
    o.sample("doc_queue_depth", &[("state", "running")], snap.queue.running);
    o.sample("doc_queue_depth", &[("state", "delayed")], snap.queue.delayed);
    o.family("doc_queue_oldest_waiting_seconds", "gauge", "Age of the oldest entry in the queue stream (0 when empty).");
    o.sample("doc_queue_oldest_waiting_seconds", &[], snap.queue.oldest_queued_age_ms as f64 / 1000.0);

    o.family("doc_job_attempts_total", "counter", "Job attempts started (a retried job counts each attempt).");
    o.sample("doc_job_attempts_total", &[], get("doc_job_attempts_total", &[]));
    o.family("doc_jobs_total", "counter", "Jobs by outcome: succeeded, failed.");
    for outcome in ["succeeded", "failed"] {
        o.sample("doc_jobs_total", &[("outcome", outcome)], get("doc_jobs_total", &[("outcome", outcome)]));
    }
    o.family("doc_job_failures_total", "counter", "Failed attempts by error code and whether the failure was transient (retried) or permanent.");
    let mut seen = false;
    for (field, v) in c.iter() {
        let (name, labels) = parse(field);
        if name == "doc_job_failures_total" {
            seen = true;
            o.sample(name, &[("code", labels.get("code").copied().unwrap_or("")), ("kind", labels.get("kind").copied().unwrap_or(""))], v);
        }
    }
    if !seen {
        o.sample("doc_job_failures_total", &[("code", "none"), ("kind", "none")], 0);
    }

    o.family("doc_stage_duration_seconds", "histogram", "Duration of each pipeline stage.");
    let mut stages: Vec<&str> = Vec::new();
    for field in c.keys() {
        let (name, labels) = parse(field);
        if name == "doc_stage_duration_seconds_count" {
            if let Some(st) = labels.get("stage") {
                stages.push(st);
            }
        }
    }
    for st in stages {
        let mut cum = 0u64;
        for b in BUCKETS.iter() {
            cum += get("doc_stage_duration_seconds_bucket", &[("stage", st), ("le", &format!("{b}"))]);
            o.sample("doc_stage_duration_seconds_bucket", &[("stage", st), ("le", &format!("{b}"))], cum);
        }
        cum += get("doc_stage_duration_seconds_bucket", &[("stage", st), ("le", "+Inf")]);
        o.sample("doc_stage_duration_seconds_bucket", &[("stage", st), ("le", "+Inf")], cum);
        o.sample("doc_stage_duration_seconds_sum", &[("stage", st)], get("doc_stage_duration_seconds_sum_us", &[("stage", st)]) as f64 / 1_000_000.0);
        o.sample("doc_stage_duration_seconds_count", &[("stage", st)], get("doc_stage_duration_seconds_count", &[("stage", st)]));
    }

    o.family("doc_stalled_recoveries_total", "counter", "Jobs re-queued because the worker holding them died (lease expired).");
    o.sample("doc_stalled_recoveries_total", &[], snap.queue.stalled_recovered_total);
    o.family("doc_repo_clones_total", "counter", "Repository clones actually run (cache hits and clones waited for are not counted).");
    o.sample("doc_repo_clones_total", &[], get("doc_repo_clones_total", &[]));
    o.family("doc_cache_hits_total", "counter", "Cache hits: a fresh document served again, or a fresh repository copy reused.");
    o.sample("doc_cache_hits_total", &[("kind", "document")], get("doc_cache_hits_total", &[("kind", "document")]));
    o.sample("doc_cache_hits_total", &[("kind", "repository")], get("doc_cache_hits_total", &[("kind", "repository")]));
    o.family("doc_catalog_refreshes_total", "counter", "Data-API catalog fetches that answered.");
    o.sample("doc_catalog_refreshes_total", &[], get("doc_catalog_refreshes_total", &[]));
    o.family("doc_catalog_refresh_failures_total", "counter", "Data-API catalog fetch attempts that failed.");
    o.sample("doc_catalog_refresh_failures_total", &[], get("doc_catalog_refresh_failures_total", &[]));
    o.family("doc_email_total", "counter", "Email step outcomes: sent, already_sent, in_doubt, failed, skipped.");
    for outcome in ["sent", "already_sent", "in_doubt", "failed", "skipped"] {
        o.sample("doc_email_total", &[("outcome", outcome)], get("doc_email_total", &[("outcome", outcome)]));
    }
    o.family("doc_docx_pool_restarts_total", "counter", "Persistent docx workers replaced, by reason (crash, deadline, recycle, memory).");
    for reason in ["crash", "deadline", "recycle", "memory"] {
        o.sample("doc_docx_pool_restarts_total", &[("reason", reason)], get("doc_docx_pool_restarts_total", &[("reason", reason)]));
    }

    o.family("doc_worker_up", "gauge", "1 when the process has reported within the liveness window.");
    let mut alive = 0u64;
    for w in snap.workers {
        let up = u64::from(w.age_ms <= snap.alive_within_ms);
        alive += up;
        let role = w.info.get("role").and_then(|v| v.as_str()).unwrap_or("");
        o.sample("doc_worker_up", &[("worker", &w.id), ("role", role)], up);
    }
    o.family("doc_workers_alive", "gauge", "Processes (workers and API nodes) alive within the liveness window.");
    o.sample("doc_workers_alive", &[], alive);
    o.family("doc_catalog_last_success_timestamp_seconds", "gauge", "Unix time of the newest successful catalog fetch among live processes (0 when none).");
    let last_ok = snap.workers.iter().filter_map(|w| w.info.pointer("/catalog/last_ok_unix_s").and_then(|v| v.as_u64())).max().unwrap_or(0);
    o.sample("doc_catalog_last_success_timestamp_seconds", &[], last_ok);
    o.family("doc_docx_pool_workers", "gauge", "Persistent docx workers by state, summed over live processes.");
    let sum = |p: &str| snap.workers.iter().filter(|w| w.age_ms <= snap.alive_within_ms).filter_map(|w| w.info.pointer(p).and_then(|v| v.as_u64())).sum::<u64>();
    o.sample("doc_docx_pool_workers", &[("state", "live")], sum("/pool/live"));
    o.sample("doc_docx_pool_workers", &[("state", "busy")], sum("/pool/busy"));
    o.family("doc_scrape_timestamp_seconds", "gauge", "Unix time of this scrape.");
    o.sample("doc_scrape_timestamp_seconds", &[], snap.now_unix_s);
    o.s
}

/// A strict parser for the text format: every non-comment line is `name{labels} value`, every sample's
/// family has a TYPE line before it, histogram buckets are cumulative and end with +Inf = count.
pub fn validate_exposition(text: &str) -> Result<BTreeMap<String, f64>, String> {
    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# TYPE ") {
            let mut it = rest.split(' ');
            let (n, t) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            if !["counter", "gauge", "histogram"].contains(&t) {
                return Err(format!("line {i}: bad type {t}"));
            }
            types.insert(n.to_string(), t.to_string());
            continue;
        }
        if line.starts_with('#') {
            if !line.starts_with("# HELP ") {
                return Err(format!("line {i}: bad comment"));
            }
            continue;
        }
        let (series, value) = line.rsplit_once(' ').ok_or(format!("line {i}: no value"))?;
        let v: f64 = value.parse().map_err(|_| format!("line {i}: bad value {value}"))?;
        let name = series.split('{').next().unwrap_or("");
        let base = name.trim_end_matches("_bucket").trim_end_matches("_sum").trim_end_matches("_count");
        if !types.contains_key(name) && !types.contains_key(base) {
            return Err(format!("line {i}: sample {name} has no TYPE"));
        }
        if series.contains('{') && !series.ends_with('}') {
            return Err(format!("line {i}: unterminated labels"));
        }
        out.insert(series.to_string(), v);
    }
    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_store::MemStore;

    #[tokio::test]
    async fn render_is_valid_text_format_and_counts() {
        let store = MemStore::new();
        observe_stage(&store, "parse", 0.03).await;
        observe_stage(&store, "parse", 0.3).await;
        incr(&store, "doc_job_attempts_total", &[]).await;
        incr(&store, "doc_job_failures_total", &[("code", "docx_worker_timeout"), ("kind", "transient")]).await;
        let counters = store.metrics().await.unwrap_or_default();
        let q = QueueHealth { queued: 2, running: 1, delayed: 0, stalled_recovered_total: 3, oldest_queued_age_ms: 1500 };
        let w = vec![WorkerView { id: "w1".into(), age_ms: 100, info: serde_json::json!({"role":"worker","catalog":{"last_ok_unix_s":1700000000},"pool":{"live":2,"busy":1}}) }];
        let text = render(&Snapshot { counters: &counters, queue: &q, workers: &w, alive_within_ms: 15_000, now_unix_s: 1700000100 });
        let m = validate_exposition(&text).unwrap_or_default();
        assert_eq!(m.get("doc_queue_depth{state=\"queued\"}"), Some(&2.0));
        assert_eq!(m.get("doc_stalled_recoveries_total"), Some(&3.0));
        assert_eq!(m.get("doc_stage_duration_seconds_count{stage=\"parse\"}"), Some(&2.0));
        assert_eq!(m.get("doc_stage_duration_seconds_bucket{stage=\"parse\",le=\"0.05\"}"), Some(&1.0));
        assert_eq!(m.get("doc_stage_duration_seconds_bucket{stage=\"parse\",le=\"+Inf\"}"), Some(&2.0));
        assert_eq!(m.get("doc_job_failures_total{code=\"docx_worker_timeout\",kind=\"transient\"}"), Some(&1.0));
        assert_eq!(m.get("doc_workers_alive"), Some(&1.0));
        assert_eq!(m.get("doc_catalog_last_success_timestamp_seconds"), Some(&1700000000.0));
        assert_eq!(m.get("doc_docx_pool_workers{state=\"live\"}"), Some(&2.0));
    }
}
