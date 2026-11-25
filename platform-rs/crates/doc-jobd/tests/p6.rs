#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The P6 suite: what hardening added, each item with the test that proves it. Real Redis (or Valkey: set
//! DOC_SERVER_BIN), real `doc-jobd` processes, a real Python docx worker; git, the catalog and SMTP are fakes.

mod support;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::*;

struct Rig {
    dir: TempDir,
    redis: Redis,
    catalog: Catalog,
    git: FakeGit,
    c: reqwest::Client,
}

impl Rig {
    async fn new(tag: &str) -> Rig {
        Rig::with_catalog(tag, english_catalog()).await
    }
    async fn with_catalog(tag: &str, cat: String) -> Rig {
        let dir = TempDir::new(tag);
        let redis = Redis::start(&dir.path().join("redis"), true).await;
        let catalog = Catalog::start(vec![cat]).await;
        let git = FakeGit::install(dir.path());
        Rig { dir, redis, catalog, git, c: reqwest::Client::new() }
    }
    fn setup(&self) -> Setup {
        setup(self.dir.path(), &self.redis, &self.catalog, &self.git)
    }
    async fn daemon(&self, name: &str, extra: &[(&str, &str)]) -> Daemon {
        Daemon::spawn(&self.setup(), name, extra).await
    }
    async fn submit(&self, d: &Daemon, body: &Value) -> String {
        let (code, b, _) = post_docx(&self.c, &d.base(), body).await;
        assert_eq!(code, 200, "{b}");
        b["task_id"].as_str().unwrap().to_string()
    }
    async fn done(&self, d: &Daemon, id: &str) -> Value {
        let (seen, last) = watch(&self.c, &d.base(), id, Duration::from_secs(90)).await;
        assert_eq!(last["state"], "SUCCESS", "states {seen:?}; log:\n{}", tail_log(&d.log_text()));
        last
    }
    async fn hget(&self, id: &str, field: &str) -> Option<String> {
        let mut c = self.redis.conn().await;
        redis::cmd("HGET").arg(format!("t:job:{id}")).arg(field).query_async(&mut c).await.unwrap()
    }
    fn out(&self) -> std::path::PathBuf {
        self.setup().output
    }
}

fn tail_log(s: &str) -> String {
    let l: Vec<&str> = s.lines().collect();
    l[l.len().saturating_sub(25)..].join("\n")
}

/// Distinct requests (distinct keys, same repositories): vary the options that enter the key.
fn variant(i: usize) -> Value {
    jude_request(json!({"layout_for_print": i % 2 == 1, "show_tn_book_intro": (i / 2) % 2 == 1, "use_chapter_labels": (i / 4) % 2 == 1}))
}

async fn wait_for(what: &str, within: Duration, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + within;
    while !f() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn events(d: &Daemon) -> Vec<(String, Value)> {
    log_events(&d.log_text())
}

// ============================================================================= A: single flight across processes

#[tokio::test(flavor = "multi_thread")]
async fn two_worker_processes_clone_each_repository_once() {
    let rig = Rig::new("sf2").await;
    std::fs::write(rig.git.state.join("delay"), "1.5").unwrap();
    let a = rig.daemon("a", &[("DOC_JOBD_CONCURRENCY", "4")]).await;
    let b = rig.daemon("b", &[("DOC_JOBD_CONCURRENCY", "4")]).await;
    let mut ids = Vec::new();
    for i in 0..6 {
        let d = if i % 2 == 0 { &a } else { &b };
        ids.push(rig.submit(d, &variant(i)).await);
    }
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 6, "six distinct jobs");
    for id in &ids {
        rig.done(&a, id).await;
    }
    let calls = rig.git.calls();
    println!("6 jobs on 2 worker processes, git invocations: {calls}");
    assert_eq!(calls, 2, "en_ulb and en_tn cloned exactly once each across both processes");
    let ran = |d: &Daemon| events(d).iter().filter(|(m, _)| m == "attempt started").count();
    assert!(ran(&a) >= 1 && ran(&b) >= 1, "both processes ran jobs: a={} b={}", ran(&a), ran(&b));
}

#[tokio::test(flavor = "multi_thread")]
async fn control_without_the_lock_two_processes_clone_twice() {
    let rig = Rig::new("sf2off").await;
    std::fs::write(rig.git.state.join("delay"), "1.5").unwrap();
    let off = [("DOC_JOBD_CLONE_LOCK", "false"), ("DOC_JOBD_CONCURRENCY", "4")];
    let a = rig.daemon("a", &off).await;
    let b = rig.daemon("b", &off).await;
    let ia = rig.submit(&a, &variant(0)).await;
    let ib = rig.submit(&b, &variant(1)).await;
    rig.done(&a, &ia).await;
    rig.done(&b, &ib).await;
    let calls = rig.git.calls();
    println!("control (lock off) git invocations: {calls}");
    assert!(calls > 2, "without the lock the processes race and clone more than once ({calls})");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_killed_lock_holder_is_replaced_when_its_lease_expires() {
    let rig = Rig::new("lockdie").await;
    std::fs::write(rig.git.state.join("delay"), "4").unwrap();
    let env = [("DOC_SHELL_CLONE_LOCK_LEASE_SECS", "2")];
    let mut a = rig.daemon("a", &env).await;
    let id = rig.submit(&a, &jude_request(json!({}))).await;
    wait_for("both clones to start", Duration::from_secs(20), || rig.git.calls() >= 2).await;
    a.sigkill(); // the lock holder dies with the leases held
    let b = rig.daemon("b", &env).await;
    let last = rig.done(&b, &id).await;
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("2"));
    let calls = rig.git.calls();
    assert_eq!(calls, 4, "the dead holder's two clones were repeated by the new holder once its lease expired");
    let key = last["result"].as_str().unwrap();
    assert_eq!(check_docx(&rig.out().join(format!("{key}.docx")))["ok"], true);
    let mut c = rig.redis.conn().await;
    let fences: Vec<String> = redis::cmd("KEYS").arg("t:fence:*").query_async(&mut c).await.unwrap();
    for f in fences {
        let n: u64 = redis::cmd("GET").arg(&f).query_async(&mut c).await.unwrap();
        assert!(n >= 2, "{f}: the fencing token grew with the second grant ({n})");
    }
}

// ============================================================================= A: the core is killable

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_core_is_killed_at_the_stage_deadline_and_the_job_retried() {
    let rig = Rig::new("corehang").await;
    let (state, pidfile) = (rig.dir.path().join("core.state"), rig.dir.path().join("core.pid"));
    let d = rig
        .daemon(
            "d",
            &[("DOC_CORE_FAULT", "hang"), ("DOC_CORE_FAULT_TIMES", "1"), ("DOC_CORE_FAULT_STATE", &state.display().to_string()), ("DOC_CORE_PIDFILE", &pidfile.display().to_string()), ("DOC_JOBD_STAGE_PARSE_MS", "800"), ("DOC_JOBD_STAGE_ASSEMBLE_MS", "400")],
        )
        .await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let last = rig.done(&d, &id).await;
    let pid: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    assert!(!pid_alive(pid), "the hung core process {pid} was killed and reaped");
    assert!(events(&d).iter().any(|(m, f)| m == "stage failed" && f["stage"] == "core" && f["error"].as_str().is_some_and(|e| e.contains("stage_timeout"))), "stage timeout logged");
    assert!(rig.hget(&id, "attempt").await.unwrap().parse::<u32>().unwrap() >= 2);
    let key = last["result"].as_str().unwrap();
    assert_eq!(check_docx(&rig.out().join(format!("{key}.docx")))["ok"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crashing_core_child_is_a_transient_failure_not_a_dead_worker() {
    let rig = Rig::new("corecrash").await;
    let state = rig.dir.path().join("core.state");
    let d = rig.daemon("d", &[("DOC_CORE_FAULT", "crash"), ("DOC_CORE_FAULT_TIMES", "2"), ("DOC_CORE_FAULT_STATE", &state.display().to_string())]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.done(&d, &id).await;
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("3"), "two crashes, then success");
    assert!(events(&d).iter().filter(|(m, f)| m == "attempt failed (transient); will retry" && f["code"] == "core_crashed").count() >= 2);
}

// ============================================================================= A: the persistent docx pool

fn pool_env<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v = vec![("DOC_DOCX_MODE", "persistent"), ("DOC_JOBD_BEAT_EVERY_MS", "200")];
    v.extend_from_slice(extra);
    v
}

async fn pool_info(rig: &Rig, d: &Daemon) -> Value {
    tokio::time::sleep(Duration::from_millis(500)).await; // a beat
    let (_, text) = get_text(&rig.c, &format!("{}/health/status", d.base())).await;
    let v: Value = serde_json::from_str(&text).unwrap();
    v[0]["processes"][0]["docx"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_worker_crash_is_replaced_and_the_job_still_ends_with_a_document() {
    let rig = Rig::new("poolcrash").await;
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &pool_env(&[("DOCX_WORKER_FAULT", "crash"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string())])).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let last = rig.done(&d, &id).await;
    assert!(rig.hget(&id, "attempt").await.unwrap().parse::<u32>().unwrap() >= 2);
    let info = pool_info(&rig, &d).await;
    assert!(info["restarts"]["crash"].as_u64().unwrap() >= 1, "{info}");
    assert!(info["spawned"].as_u64().unwrap() >= 2, "a replacement was started: {info}");
    let key = last["result"].as_str().unwrap();
    assert_eq!(check_docx(&rig.out().join(format!("{key}.docx")))["ok"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_worker_past_its_deadline_is_killed_with_its_group_and_replaced() {
    let rig = Rig::new("poolhang").await;
    let (state, pidfile) = (rig.dir.path().join("fault.state"), rig.dir.path().join("docx.pid"));
    let d = rig
        .daemon(
            "d",
            &pool_env(&[("DOCX_WORKER_FAULT", "hang-grandchild"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string()), ("DOCX_WORKER_PIDFILE", &pidfile.display().to_string()), ("DOC_JOBD_DOCX_DEADLINE_MS", "1500")]),
        )
        .await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.done(&d, &id).await;
    let pids: Vec<i32> = std::fs::read_to_string(&pidfile).unwrap().split_whitespace().filter_map(|s| s.parse().ok()).collect();
    wait_for("hung worker and its grandchild to be gone", Duration::from_secs(5), || pids.iter().all(|p| !pid_alive(*p))).await;
    let info = pool_info(&rig, &d).await;
    assert!(info["restarts"]["deadline"].as_u64().unwrap() >= 1, "{info}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_worker_is_recycled_after_n_jobs_and_when_it_grows_past_the_memory_ceiling() {
    let rig = Rig::new("poolrecycle").await;
    let d = rig.daemon("d", &pool_env(&[("DOC_DOCX_RECYCLE_AFTER", "2")])).await;
    for i in 0..5 {
        let id = rig.submit(&d, &variant(i)).await;
        rig.done(&d, &id).await;
    }
    let info = pool_info(&rig, &d).await;
    assert!(info["restarts"]["recycle"].as_u64().unwrap() >= 2, "5 jobs with recycle_after=2: {info}");
    assert_eq!(info["jobs"], 5);

    // memory: each job leaks 30 MB in the worker; the ceiling is 70 MB (a fresh interpreter is about 25 MB)
    let rig2 = Rig::new("poolmem").await;
    let d2 = rig2.daemon("d", &pool_env(&[("DOCX_WORKER_FAULT", "leak:30000"), ("DOC_DOCX_RSS_CEILING_MB", "70")])).await;
    for i in 0..4 {
        let id = rig2.submit(&d2, &variant(i)).await;
        rig2.done(&d2, &id).await;
    }
    let info = pool_info(&rig2, &d2).await;
    assert!(info["restarts"]["memory"].as_u64().unwrap() >= 1, "the leaking worker was retired: {info}");
}

#[tokio::test(flavor = "multi_thread")]
async fn docx_seam_cost_per_job_in_both_modes() {
    let mut out = serde_json::Map::new();
    for mode in ["per_job", "persistent"] {
        let rig = Rig::new(&format!("cost-{mode}")).await;
        let d = rig.daemon("d", &[("DOC_DOCX_MODE", mode)]).await;
        let mut walls: Vec<u64> = Vec::new();
        let mut totals: Vec<f64> = Vec::new();
        // fresh keys for the measurement: vary a tag in the request that enters the key
        for i in 0..12 {
            let chunk = ["chapter", "verse"][i % 2];
            let req = jude_request(json!({"chunk_size": chunk, "layout_for_print": i % 3 == 0, "show_tn_book_intro": i % 4 == 0, "use_chapter_labels": i % 5 == 0, "limit_words": i % 7 == 0}));
            let t = Instant::now();
            let id = rig.submit(&d, &req).await;
            rig.done(&d, &id).await;
            totals.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        for (m, f) in events(&d) {
            if m == "docx worker finished" || m == "docx worker finished (persistent)" {
                if let Some(w) = f["wall_ms"].as_u64() {
                    walls.push(w);
                }
            }
        }
        walls.sort_unstable();
        totals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = |v: &[u64]| v.get(v.len() / 2).copied().unwrap_or(0);
        println!("{mode}: docx wall p50 {} ms (n={}), job end-to-end p50 {:.0} ms", med(&walls), walls.len(), totals[totals.len() / 2]);
        out.insert(mode.into(), json!({"docx_wall_ms_p50": med(&walls), "docx_wall_ms_all": walls, "job_total_ms_p50": totals[totals.len() / 2], "jobs": totals.len()}));
    }
    let results = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../results/seam-cost.json");
    std::fs::write(results, serde_json::to_string_pretty(&Value::Object(out)).unwrap()).unwrap();
}

// ============================================================================= C: observability

#[tokio::test(flavor = "multi_thread")]
async fn the_request_id_reaches_the_worker_and_the_python_process() {
    for mode in ["per_job", "persistent"] {
        let rig = Rig::new(&format!("rid-{mode}")).await;
        let d = rig.daemon("d", &[("DOC_DOCX_MODE", mode)]).await;
        let resp = rig.c.post(format!("{}/documents_docx", d.base())).header("x-request-id", "req-abc.123").json(&jude_request(json!({}))).send().await.unwrap();
        assert_eq!(resp.headers().get("x-request-id").unwrap(), "req-abc.123", "echoed");
        let id = resp.json::<Value>().await.unwrap()["task_id"].as_str().unwrap().to_string();
        rig.done(&d, &id).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let ev = events(&d);
        let with_rid = |msg: &str| ev.iter().any(|(m, f)| m == msg && f["request_id"] == "req-abc.123");
        assert!(with_rid("request"), "[{mode}] the HTTP request line");
        assert!(with_rid("document request accepted"), "[{mode}] accepted");
        assert!(with_rid("attempt started"), "[{mode}] the worker's attempt line");
        for st in ["locate", "provision", "core", "docx"] {
            assert!(ev.iter().any(|(m, f)| m == "stage finished" && f["stage"] == st && f["request_id"] == "req-abc.123"), "[{mode}] stage {st}");
        }
        assert!(ev.iter().any(|(m, f)| m == "python docx worker" && f["event"] == "docx_start" && f["request_id"] == "req-abc.123"), "[{mode}] the Python process logged the same id");
        assert!(with_rid("attempt succeeded; job done"), "[{mode}] terminal line");
        // an id is made when none is sent
        let r2 = rig.c.get(format!("{}/health/status", d.base())).send().await.unwrap();
        assert!(r2.headers().get("x-request-id").is_some());
        let bad = rig.c.get(format!("{}/health/status", d.base())).header("x-request-id", "bad id with spaces").send().await.unwrap();
        assert_ne!(bad.headers().get("x-request-id").unwrap(), "bad id with spaces", "an unsafe id is replaced");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn metrics_endpoint_is_valid_prometheus_text_and_every_family_moves() {
    let rig = Rig::new("metrics").await;
    let d = rig.daemon("d", &[("DOC_DOCX_MODE", "persistent"), ("DOC_JOBD_BEAT_EVERY_MS", "200")]).await;
    let (_, before) = get_text(&rig.c, &format!("{}/metrics", d.base())).await;
    let m0 = doc_jobs::metrics::validate_exposition(&before).expect("valid before any job");
    for fam in ["doc_queue_depth{state=\"queued\"}", "doc_queue_oldest_waiting_seconds", "doc_job_attempts_total", "doc_stalled_recoveries_total", "doc_repo_clones_total", "doc_catalog_refreshes_total", "doc_workers_alive", "doc_store_up"] {
        assert!(m0.contains_key(fam), "family {fam} present at zero\n{before}");
    }
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.done(&d, &id).await;
    let id2 = rig.submit(&d, &jude_request(json!({}))).await; // a cache hit
    rig.done(&d, &id2).await;
    tokio::time::sleep(Duration::from_millis(600)).await; // a beat flushes the process-local counters
    let (code, text) = get_text(&rig.c, &format!("{}/metrics", d.base())).await;
    assert_eq!(code, 200);
    let m = doc_jobs::metrics::validate_exposition(&text).expect("valid after jobs");
    let v = |k: &str| *m.get(k).unwrap_or_else(|| panic!("missing {k}\n{text}"));
    assert!(v("doc_job_attempts_total") >= 2.0);
    assert!(v("doc_jobs_total{outcome=\"succeeded\"}") >= 2.0);
    assert!(v("doc_cache_hits_total{kind=\"document\"}") >= 1.0, "the second request was a cache hit");
    assert_eq!(v("doc_repo_clones_total"), 2.0, "two repositories cloned");
    assert!(v("doc_catalog_refreshes_total") >= 1.0);
    for st in ["locate", "provision", "core", "docx"] {
        assert!(v(&format!("doc_stage_duration_seconds_count{{stage=\"{st}\"}}")) >= 1.0, "stage {st} observed");
        assert!(v(&format!("doc_stage_duration_seconds_bucket{{stage=\"{st}\",le=\"+Inf\"}}")) >= 1.0);
    }
    assert!(v("doc_workers_alive") >= 1.0);
    assert!(v("doc_docx_pool_workers{state=\"live\"}") >= 1.0);
    assert!(v("doc_catalog_last_success_timestamp_seconds") > 1_700_000_000.0);
    assert_eq!(v("doc_store_up"), 1.0);

    // a failure moves the failure family, with its code
    let bad = jude_request(json!({"resource_requests": [{"lang_code": "xx", "resource_type": "ulb", "book_code": "jud"}]}));
    let fid = rig.submit(&d, &bad).await;
    let (_, last) = watch(&rig.c, &d.base(), &fid, Duration::from_secs(30)).await;
    assert_eq!(last["state"], "FAILURE");
    assert_eq!(last["result"]["code"], "resource_not_found");
    let (_, text) = get_text(&rig.c, &format!("{}/metrics", d.base())).await;
    let m = doc_jobs::metrics::validate_exposition(&text).unwrap();
    assert_eq!(m.get("doc_job_failures_total{code=\"resource_not_found\",kind=\"permanent\"}"), Some(&1.0));
    assert!(m.get("doc_jobs_total{outcome=\"failed\"}").copied().unwrap_or(0.0) >= 1.0);
}

#[tokio::test(flavor = "multi_thread")]
async fn health_reports_redis_queue_lag_catalog_pool_and_disk_and_degrades_honestly() {
    let mut rig = Rig::new("health").await;
    let d = rig.daemon("d", &[("DOC_DOCX_MODE", "persistent"), ("DOC_JOBD_BEAT_EVERY_MS", "200")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.done(&d, &id).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (code, text) = get_text(&rig.c, &format!("{}/health/status", d.base())).await;
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v[1], 200, "DOC's tuple shape is kept");
    let h = &v[0];
    assert_eq!(h["status"], "ok");
    assert_eq!(h["ready"], true, "{h}");
    assert_eq!(h["redis"]["ok"], true);
    assert!(h["queue"]["lag_s"].is_number() && h["queue"]["queued"].is_number());
    assert!(h["catalog"]["last_success_unix_s"].as_u64().unwrap() > 1_700_000_000);
    assert_eq!(h["processes"][0]["alive"], true);
    assert_eq!(h["processes"][0]["docx"]["mode"], "persistent");
    assert!(h["disk"]["free_mb"].as_u64().is_some());
    // Redis down: still answers (liveness), says it is not ready, and the problem is named
    rig.redis.shutdown().await;
    let (code, text) = get_text(&rig.c, &format!("{}/health/status", d.base())).await;
    assert_eq!(code, 200);
    let h = serde_json::from_str::<Value>(&text).unwrap()[0].clone();
    assert_eq!(h["status"], "ok");
    assert_eq!(h["ready"], false);
    assert!(h["problems"].to_string().contains("job store is unreachable"), "{h}");
    let (mcode, mtext) = get_text(&rig.c, &format!("{}/metrics", d.base())).await;
    assert_eq!(mcode, 200);
    assert!(mtext.contains("doc_store_up 0"), "the scrape says the store is down");
}

// ============================================================================= D: operational safety

#[test]
fn bad_configuration_is_reported_all_at_once_and_secrets_are_not_printed() {
    let bin = env!("CARGO_BIN_EXE_doc-jobd");
    let run = |env: &[(&str, &str)]| {
        let mut c = std::process::Command::new(bin);
        c.arg("check-config").env_clear().env("PATH", std::env::var("PATH").unwrap());
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    };
    let o = run(&[("DOC_JOBD_ROLE", "bogus"), ("DOC_JOBD_CONCURRENCY", "0"), ("DOC_JOBD_LEASE_TTL_MS", "abc"), ("DOC_REDIS_CA_FILE", "/nope/ca.pem"), ("SMTP_PORT", "70000"), ("DOC_DOCX_MODE", "turbo"), ("DOC_JOBD_REDIS_URL", "http://x")]);
    assert_eq!(o.status.code(), Some(2));
    let err = String::from_utf8_lossy(&o.stderr);
    for name in ["DATA_API_URL", "DOC_JOBD_ROLE", "DOC_JOBD_CONCURRENCY", "DOC_JOBD_LEASE_TTL_MS", "DOC_REDIS_CA_FILE", "SMTP_PORT", "DOC_DOCX_MODE", "DOC_JOBD_REDIS_URL"] {
        assert!(err.contains(name), "problem naming {name} in:\n{err}");
    }
    assert!(err.matches("\n  ").count() >= 8, "every problem on its own line:\n{err}");
    println!("{err}");
    let dir = TempDir::new("cfg");
    let ok = run(&[("DATA_API_URL", "http://127.0.0.1:1/v1"), ("DOC_JOBD_OUTPUT_DIR", &dir.path().join("o").display().to_string()), ("DOC_JOBD_REDIS_URL", "rediss://user:topsecret@redis:6380/0"), ("DOC_REDIS_PASSWORD", "alsosecret"), ("DOC_JOBD_DOCX_WORKER", &p5_root().join("python/docx_worker.py").display().to_string())]);
    assert_eq!(ok.status.code(), Some(0), "{}", String::from_utf8_lossy(&ok.stderr));
    let out = String::from_utf8_lossy(&ok.stdout);
    assert!(out.contains("configuration ok") && !out.contains("topsecret") && !out.contains("alsosecret"), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn redis_password_and_tls_are_supported() {
    use doc_jobs::redis_store::{RedisOptions, RedisStore};
    let dir = TempDir::new("tls");
    make_certs(&dir.path().join("certs"));
    let c = dir.path().join("certs");
    let port = free_port();
    let _srv = Redis::start_with(
        &dir.path().join("srv"),
        port,
        &["--port", "0", "--tls-port", &port.to_string(), "--tls-cert-file", &c.join("srv.crt").display().to_string(), "--tls-key-file", &c.join("srv.key").display().to_string(), "--tls-ca-cert-file", &c.join("ca.crt").display().to_string(), "--tls-auth-clients", "no", "--requirepass", "s3cret"],
    )
    .await;
    let url = format!("rediss://127.0.0.1:{port}/0");
    let good = RedisOptions { url: url.clone(), username: None, password: Some("s3cret".into()), ca_file: Some(c.join("ca.crt")) };
    assert!(RedisStore::connect_with(&good, "tls").await.is_ok(), "right password, right CA");
    let no_pw = RedisOptions { password: None, ..good.clone() };
    let e = RedisStore::connect_with(&no_pw, "tls").await.err().expect("must fail without the password");
    println!("no password: {e}");
    let wrong_pw = RedisOptions { password: Some("nope".into()), ..good.clone() };
    assert!(RedisStore::connect_with(&wrong_pw, "tls").await.is_err(), "wrong password");
    let wrong_ca = RedisOptions { ca_file: Some(c.join("other.crt")), ..good.clone() };
    let e = RedisStore::connect_with(&wrong_ca, "tls").await.err().expect("must fail with an unrelated CA");
    println!("wrong CA: {e}");
    let plain = RedisOptions { url: format!("redis://127.0.0.1:{port}/0"), ca_file: None, ..good.clone() };
    assert!(RedisStore::connect_with(&plain, "tls").await.is_err(), "plain text to a TLS port");
    // and the whole service over TLS with a password
    let catalog = Catalog::start(vec![english_catalog()]).await;
    let git = FakeGit::install(dir.path());
    let mut s = setup(dir.path(), &Redis { port: 0, dir: dir.path().to_path_buf(), child: None }, &catalog, &git);
    s.redis_url = url;
    let d = Daemon::spawn(&s, "d", &[("DOC_REDIS_PASSWORD", "s3cret"), ("DOC_REDIS_CA_FILE", &c.join("ca.crt").display().to_string())]).await;
    let cl = reqwest::Client::new();
    let (code, b, _) = post_docx(&cl, &d.base(), &jude_request(json!({}))).await;
    assert_eq!(code, 200, "{b}");
    let (_, last) = watch(&cl, &d.base(), b["task_id"].as_str().unwrap(), Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert!(!d.log_text().contains("s3cret"), "the password never reaches the log");
}

#[tokio::test(flavor = "multi_thread")]
async fn size_limit_path_validation_and_the_disk_guard() {
    let rig = Rig::new("limits").await;
    let d = rig.daemon("d", &[("DOC_JOBD_MAX_BODY_BYTES", "2048")]).await;
    // 413: the body is bigger than the limit
    let big = json!({"pad": "x".repeat(5000)});
    let r = rig.c.post(format!("{}/documents_docx", d.base())).json(&big).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 413);
    // 422: language codes 1 to 35 letters, digits and hyphens
    for bad in ["", &"a".repeat(36), "en/../x", "en us", "ñ"] {
        let req = jude_request(json!({"resource_requests": [{"lang_code": bad, "resource_type": "ulb", "book_code": "jud"}]}));
        let (code, body, _) = post_docx(&rig.c, &d.base(), &req).await;
        assert_eq!(code, 422, "lang_code {bad:?}: {body}");
    }
    let edge = jude_request(json!({"resource_requests": [{"lang_code": "a".repeat(35), "resource_type": "ulb", "book_code": "jud"}]}));
    let (code, _, _) = post_docx(&rig.c, &d.base(), &edge).await;
    assert_eq!(code, 200, "35 characters is accepted");
    // 422: at most 100 resources (the book-code bound)
    let d2 = rig.daemon("d2", &[("DOC_JOBD_MAX_RESOURCES", "100"), ("DOC_JOBD_MAX_BODY_BYTES", "1048576")]).await;
    let many: Vec<Value> = (0..101).map(|_| json!({"lang_code": "en", "resource_type": "ulb", "book_code": "jud"})).collect();
    let (code, body, _) = post_docx(&rig.c, &d2.base(), &jude_request(json!({"resource_requests": many}))).await;
    assert_eq!(code, 422, "{body}");
    let hundred: Vec<Value> = (0..100).map(|_| json!({"lang_code": "en", "resource_type": "ulb", "book_code": "jud"})).collect();
    let (code, _, _) = post_docx(&rig.c, &d2.base(), &jude_request(json!({"resource_requests": hundred}))).await;
    assert_eq!(code, 200, "exactly 100 is accepted");
    // disk guard: a typed, retryable 503 when the filesystem has less free space than required
    let d3 = rig.daemon("d3", &[("DOC_JOBD_MIN_FREE_MB", "999999999")]).await;
    let (code, body, headers) = post_docx(&rig.c, &d3.base(), &jude_request(json!({}))).await;
    assert_eq!(code, 503, "{body}");
    assert_eq!(body["code"], "disk_space_low");
    assert_eq!(body["retryable"], true);
    assert!(headers.get("retry-after").is_some());
    let (_, t) = get_text(&rig.c, &format!("{}/health/status", d3.base())).await;
    assert!(t.contains("the output filesystem is nearly full"), "{t}");
}

#[tokio::test(flavor = "multi_thread")]
async fn start_up_removes_dead_scratch_and_the_janitor_expires_old_outputs_only() {
    let rig = Rig::new("janitor").await;
    let out = rig.out();
    std::fs::create_dir_all(out.join(".scratch/dead-task-a1-deadbeef")).unwrap();
    std::fs::create_dir_all(out.join(".scratch/live-task-a1-cafebabe")).unwrap();
    std::fs::write(out.join(".scratch/dead-task-a1-deadbeef/x"), b"x").unwrap();
    let age = |p: &Path, secs: u64| {
        let t = std::time::SystemTime::now() - Duration::from_secs(secs);
        std::fs::File::open(p).unwrap().set_modified(t).unwrap();
    };
    age(&out.join(".scratch/dead-task-a1-deadbeef"), 7200);
    for (name, secs) in [("old.docx", 7200), ("old.findings.json", 7200), ("fresh.docx", 5), ("notes.txt", 7200)] {
        std::fs::write(out.join(name), b"x").unwrap();
        age(&out.join(name), secs);
    }
    let d = rig.daemon("d", &[("DOC_JOBD_JOB_CEILING_MS", "60000"), ("ASSET_CACHING_PERIOD", "1"), ("DOC_JOBD_OUTPUT_GRACE_MS", "1000"), ("DOC_JOBD_JANITOR_EVERY_MS", "300")]).await;
    assert!(!out.join(".scratch/dead-task-a1-deadbeef").exists(), "the dead attempt's scratch was removed at start");
    assert!(out.join(".scratch/live-task-a1-cafebabe").exists(), "a recent scratch directory is kept");
    wait_for("the janitor to expire old outputs", Duration::from_secs(10), || !out.join("old.docx").exists() && !out.join("old.findings.json").exists()).await;
    assert!(out.join("fresh.docx").exists(), "a fresh output stays");
    assert!(out.join("notes.txt").exists(), "files that are not outputs are never touched");
    assert!(log_events(&d.log_text()).iter().any(|(m, _)| m == "janitor removed files"));
}

// ============================================================================= E: HTML and other shapes

#[tokio::test(flavor = "multi_thread")]
async fn html_output_for_post_documents_and_the_typed_501_for_pdf() {
    let rig = Rig::new("html").await;
    let d = rig.daemon("d", &[("DOC_JOBD_DUMP_PARTS", "1")]).await;
    let req = jude_request(json!({"generate_docx": false, "generate_pdf": false, "layout_for_print": false}));
    let resp = rig.c.post(format!("{}/documents", d.base())).json(&req).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let id = resp.json::<Value>().await.unwrap()["task_id"].as_str().unwrap().to_string();
    let (seen, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS", "{seen:?}");
    assert!(!seen.iter().any(|s| s == "Converting to Docx"), "an HTML job does not convert to docx: {seen:?}");
    let key = last["result"].as_str().unwrap();
    let html = std::fs::read_to_string(rig.out().join(format!("{key}.html"))).unwrap();
    std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../results/sample-html-output.html"), &html).unwrap();
    std::fs::copy(rig.out().join(format!("{key}.parts.json")), Path::new(env!("CARGO_MANIFEST_DIR")).join("../../results/sample-html-parts.json")).unwrap();
    assert!(html.starts_with("<!DOCTYPE html"), "DOC's header template");
    assert!(html.ends_with("</body></html>"));
    assert!(html.contains("<h2>English</h2>") && html.contains("generated on "), "title page and timestamp rendered: {}", &html[html.len().saturating_sub(300)..]);
    assert!(html.contains("class=\"verse\""), "verses are in the document");
    assert!(html.contains("Jude") || html.contains("jud"));
    assert!(rig.out().join(format!("{key}.findings.json")).exists());
    let (code, body, _) = {
        let r = rig.c.post(format!("{}/documents", d.base())).json(&jude_request(json!({"generate_pdf": true}))).send().await.unwrap();
        (r.status().as_u16(), r.json::<Value>().await.unwrap(), ())
    };
    assert_eq!(code, 501, "{body}");
    assert_eq!(body["code"], "pdf_not_implemented");
    assert_eq!(body["retryable"], false);
}

// ============================================================================= E: email

fn email_env<'a>(smtp: &'a FakeSmtp, port: &'a str, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let _ = smtp;
    let mut v = vec![("SEND_EMAIL", "true"), ("SMTP_HOST", "127.0.0.1"), ("SMTP_PORT", port), ("SMTP_TLS", "none"), ("FROM_EMAIL_ADDRESS", "doc@example.org"), ("EMAIL_SEND_SUBJECT", "Your DOC document")];
    v.extend_from_slice(extra);
    v
}

fn mail_request() -> Value {
    jude_request(json!({"email_address": "translator@example.org"}))
}

#[tokio::test(flavor = "multi_thread")]
async fn email_is_sent_once_with_the_document_attached() {
    let rig = Rig::new("mail1").await;
    let smtp = FakeSmtp::start().await;
    let port = smtp.port.to_string();
    let d = rig.daemon("d", &email_env(&smtp, &port, &[])).await;
    let id = rig.submit(&d, &mail_request()).await;
    let (seen, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS", "{seen:?}");
    assert!(seen.iter().any(|s| s == "Sending email"), "the legacy status string for the email step: {seen:?}");
    assert_eq!(smtp.count(), 1);
    let msg = smtp.received.lock().unwrap()[0].clone();
    assert!(msg.contains("To: translator@example.org") && msg.contains("Subject: Your DOC document") && msg.contains("From: doc@example.org"));
    assert!(msg.contains("filename=\"") && msg.contains(".docx\"") && msg.contains("Content-Transfer-Encoding: base64"));
    assert!(msg.contains(&format!("Message-ID: <{id}.")), "a stable Message-ID derived from the task id");
    // the same request again is a new task and a cache hit: it is a new email request and does send
    let id2 = rig.submit(&d, &mail_request()).await;
    rig.done(&d, &id2).await;
    assert_eq!(smtp.count(), 2, "a second request is a second email; a retry of the first would not be");
    // no address, no email
    let id3 = rig.submit(&d, &jude_request(json!({}))).await;
    rig.done(&d, &id3).await;
    assert_eq!(smtp.count(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusing_server_is_retried_and_the_email_goes_out_once() {
    let rig = Rig::new("mail2").await;
    let smtp = FakeSmtp::start().await;
    smtp.knobs.lock().unwrap().rcpt_refusals = 2;
    let port = smtp.port.to_string();
    let d = rig.daemon("d", &email_env(&smtp, &port, &[])).await;
    let id = rig.submit(&d, &mail_request()).await;
    rig.done(&d, &id).await;
    assert_eq!(smtp.count(), 1, "two refusals, then one message");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("3"));
    assert!(events(&d).iter().filter(|(m, f)| m == "attempt failed (transient); will retry" && f["code"] == "email_retry").count() >= 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redelivered_job_never_emails_twice() {
    // the worker dies right after the server accepted the message and the record was written
    for point in ["after_email_recorded", "after_email_accepted"] {
        let rig = Rig::new(&format!("mail3-{point}")).await;
        let smtp = FakeSmtp::start().await;
        let port = smtp.port.to_string();
        let state = rig.dir.path().join("fault.state");
        let state_s = state.display().to_string();
        let env = email_env(&smtp, &port, &[("DOC_FAULT_POINT", point), ("DOC_FAULT_STATE", &state_s)]);
        let mut a = rig.daemon("a", &env).await;
        let id = rig.submit(&a, &mail_request()).await;
        wait_for("the first worker to die at the fault point", Duration::from_secs(30), || state.exists()).await;
        let _ = a.wait_exit(Duration::from_secs(5)).await;
        assert_eq!(smtp.count(), 1, "[{point}] the first attempt sent it");
        let b = rig.daemon("b", &env).await;
        let last = rig.done(&b, &id).await;
        assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("2"), "[{point}] redelivered");
        assert_eq!(smtp.count(), 1, "[{point}] still one message after the redelivery");
        let key = last["result"].as_str().unwrap();
        let findings = std::fs::read_to_string(rig.out().join(format!("{key}.findings.json"))).unwrap();
        if point == "after_email_accepted" {
            assert!(findings.contains("email_in_doubt"), "the unconfirmed claim is reported, not resent");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crash_between_the_claim_and_the_send_sends_at_most_once() {
    let rig = Rig::new("mail4").await;
    let smtp = FakeSmtp::start().await;
    let port = smtp.port.to_string();
    let state = rig.dir.path().join("fault.state");
    let state_s = state.display().to_string();
    let env = email_env(&smtp, &port, &[("DOC_FAULT_POINT", "after_email_claim"), ("DOC_FAULT_STATE", &state_s)]);
    let mut a = rig.daemon("a", &env).await;
    let id = rig.submit(&a, &mail_request()).await;
    wait_for("the worker to die after the claim", Duration::from_secs(30), || state.exists()).await;
    let _ = a.wait_exit(Duration::from_secs(5)).await;
    assert_eq!(smtp.count(), 0);
    let b = rig.daemon("b", &env).await;
    let last = rig.done(&b, &id).await;
    assert_eq!(smtp.count(), 0, "in doubt means not sent again: at most once, never twice");
    let key = last["result"].as_str().unwrap();
    assert!(std::fs::read_to_string(rig.out().join(format!("{key}.findings.json"))).unwrap().contains("email_in_doubt"));
    assert_eq!(check_docx(&rig.out().join(format!("{key}.docx")))["ok"], true, "the document is delivered regardless");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_answer_after_the_message_is_ambiguous_and_is_not_retried() {
    let rig = Rig::new("mail5").await;
    let smtp = FakeSmtp::start().await;
    smtp.knobs.lock().unwrap().drop_after_data = true;
    let port = smtp.port.to_string();
    let d = rig.daemon("d", &email_env(&smtp, &port, &[])).await;
    let id = rig.submit(&d, &mail_request()).await;
    let last = rig.done(&d, &id).await;
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("1"), "not retried");
    assert_eq!(smtp.count(), 1);
    let key = last["result"].as_str().unwrap();
    assert!(std::fs::read_to_string(rig.out().join(format!("{key}.findings.json"))).unwrap().contains("email_in_doubt"));
}

// ============================================================================= E: the request shapes

fn rr(lang: &str, rt: &str, book: &str) -> Value {
    json!({"lang_code": lang, "resource_type": rt, "book_code": book})
}

/// Every shape the UI can post (read from frontend/src/routes/settings and the stores) plus the capabilities the
/// backend keeps: each runs as a docx job and as an HTML job; the outcome table goes to results/shapes.json.
#[tokio::test(flavor = "multi_thread")]
async fn the_request_shapes_the_ui_can_post_end_to_end_as_docx_and_html() {
    let rig = Rig::with_catalog("shapes", full_catalog()).await;
    let d = rig.daemon("d", &[("DOC_JOBD_CONCURRENCY", "4"), ("DOC_DOCX_MODE", "persistent"), ("DOC_JOBD_MAX_ATTEMPTS", "2")]).await;
    // (name, body fields, must succeed)
    let shapes: Vec<(&str, Value, bool)> = vec![
        ("ulb only, lbo, 1 column", json!({"resource_requests": [rr("en", "ulb", "jud")]}), true),
        ("ulb + tn, lbo", json!({"resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), true),
        ("ulb + tn, blo (by chapter)", json!({"assembly_strategy_kind": "blo", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), true),
        ("ulb + tn, layout for print (compact)", json!({"layout_for_print": true, "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), true),
        ("ulb + tn + bc", json!({"resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud"), rr("en", "bc", "jud")]}), true),
        ("tn only", json!({"resource_requests": [rr("en", "tn", "jud")]}), true),
        ("tn-condensed only", json!({"resource_requests": [rr("en", "tn-condensed", "jud")]}), true),
        ("bc only", json!({"resource_requests": [rr("en", "bc", "jud")]}), true),
        ("ulb + tw, links (default)", json!({"resource_requests": [rr("en", "ulb", "jud"), rr("en", "tw", "jud")]}), true),
        ("ulb + tw, definitions included", json!({"tw_mode": "include", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tw", "jud")]}), true),
        ("ulb + tw, only the words used", json!({"tw_mode": "include_used", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tw", "jud")]}), true),
        ("es-419 ulb + tq", json!({"resource_requests": [rr("es-419", "ulb", "jud"), rr("es-419", "tq", "jud")]}), true),
        ("two languages, lbo", json!({"resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud"), rr("es-419", "ulb", "jud"), rr("es-419", "tn", "jud")]}), true),
        ("two languages, blo, 2 columns scripture left/right", json!({"assembly_strategy_kind": "blo", "assembly_layout_kind": "2c_sl_sr", "resource_requests": [rr("en", "ulb", "jud"), rr("es-419", "ulb", "jud")]}), true),
        ("the UI's body as sent (all fields, epub flag, an extra field)", json!({"email_address": null, "generate_epub": false, "use_prince": true, "show_rg_chapter_commentary": true, "use_two_column_layout_for_tn_notes": false, "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), true),
        ("by verse (lvo), backend capability the UI hides", json!({"assembly_strategy_kind": "lvo", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), false),
        ("by verse (bvo), backend capability the UI hides", json!({"assembly_strategy_kind": "bvo", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}), false),
        ("a resource the catalog does not have (es-419 bc)", json!({"resource_requests": [rr("es-419", "bc", "jud")]}), false),
    ];
    let mut rows: Vec<Value> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (kind, path) in [("docx", "documents_docx"), ("html", "documents")] {
        let mut jobs = Vec::new();
        for (name, extra, must) in &shapes {
            let mut body = jude_request(json!({"generate_docx": kind == "docx", "generate_pdf": false}));
            if let (Some(o), Some(e)) = (body.as_object_mut(), extra.as_object()) {
                for (k, v) in e {
                    o.insert(k.clone(), v.clone());
                }
            }
            let t = Instant::now();
            let r = rig.c.post(format!("{}/{path}", d.base())).json(&body).send().await.unwrap();
            let code = r.status().as_u16();
            let jb: Value = r.json().await.unwrap_or(Value::Null);
            jobs.push((*name, *must, t, code, jb));
        }
        for (name, must, t, code, jb) in jobs {
            let (outcome, detail, bytes_ok) = if code != 200 {
                (format!("HTTP {code}"), jb.to_string().chars().take(160).collect::<String>(), false)
            } else {
                let id = jb["task_id"].as_str().unwrap().to_string();
                let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(120)).await;
                if last["state"] == "SUCCESS" {
                    let key = last["result"].as_str().unwrap();
                    let has_scripture = name.contains("ulb");
                    let ok = if kind == "docx" { check_docx(&rig.out().join(format!("{key}.docx")))["ok"] == true } else { std::fs::read_to_string(rig.out().join(format!("{key}.html"))).map(|h| h.ends_with("</body></html>") && (!has_scripture || h.contains("class=\"verse\""))).unwrap_or(false) };
                    ("SUCCESS".to_string(), if ok { String::new() } else { "output failed its check".into() }, ok)
                } else {
                    (format!("FAILURE {}", last["result"]["code"]), last["result"]["message"].as_str().unwrap_or("").chars().take(160).collect(), false)
                }
            };
            println!("{kind:5} | {name:62} | {outcome} {detail}");
            if must && !(outcome == "SUCCESS" && bytes_ok) {
                failures.push(format!("{kind} / {name}: {outcome} {detail}"));
            }
            rows.push(json!({"shape": name, "output": kind, "outcome": outcome, "detail": detail, "output_valid": bytes_ok, "seconds": t.elapsed().as_secs_f64()}));
        }
    }
    std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../results/shapes.json"), serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    assert!(failures.is_empty(), "shapes that must work did not:\n{}", failures.join("\n"));
}

// ============================================================================= B: Redis persistence and loss

#[tokio::test(flavor = "multi_thread")]
async fn losing_redis_without_persistence_loses_queued_state_but_the_service_heals() {
    let dir = TempDir::new("redisloss");
    let mut redis = Redis::start(&dir.path().join("redis"), false).await; // no AOF, no snapshots
    let catalog = Catalog::start(vec![english_catalog()]).await;
    let git = FakeGit::install(dir.path());
    let s = setup(dir.path(), &redis, &catalog, &git);
    let cl = reqwest::Client::new();
    let state = dir.path().join("fault.state");
    let d = Daemon::spawn(&s, "d", &[("DOCX_WORKER_FAULT", "slow:4"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string())]).await;
    let (_, b, _) = post_docx(&cl, &d.base(), &jude_request(json!({}))).await;
    let lost_id = b["task_id"].as_str().unwrap().to_string();
    wait_for("the job to reach the docx stage", Duration::from_secs(20), || log_events(&d.log_text()).iter().any(|(m, f)| m == "stage finished" && f["stage"] == "core")).await;
    redis.kill9(); // everything in Redis is gone
    redis.restart(false).await;
    // the service keeps running and finds Redis again; the job's state is gone, so its id answers PENDING (as an unknown Celery id does)
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let (_, t) = get_text(&cl, &format!("{}/health/status", d.base())).await;
        if serde_json::from_str::<Value>(&t).map(|v| v[0]["ready"] == true).unwrap_or(false) {
            break;
        }
        assert!(Instant::now() < end, "the service never became ready again: {t}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let (code, body) = status_of(&cl, &d.base(), &lost_id).await;
    assert_eq!(code, 200);
    assert_eq!(body["state"], "PENDING", "the lost job is unknown to the new store");
    // a resubmission is a new job and completes (the documents already on disk are cache hits)
    let (code, b, _) = post_docx(&cl, &d.base(), &jude_request(json!({}))).await;
    assert_eq!(code, 200, "{b}");
    let (_, last) = watch(&cl, &d.base(), b["task_id"].as_str().unwrap(), Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    let ev = log_events(&d.log_text());
    assert!(ev.iter().any(|(m, _)| m.contains("lease lost") || m.contains("store unavailable")), "the worker noticed");
    assert!(std::path::Path::new(&format!("/proc/{}", d.pid())).exists(), "the daemon never died");
}

// ============================================================================= E: PDF through the Python seam (WeasyPrint)

fn weasy_python() -> Option<String> {
    let p = p5_root().join("../_exp/venv/bin/python");
    let ok = std::process::Command::new(&p).args(["-I", "-c", "import weasyprint"]).output().map(|o| o.status.success()).unwrap_or(false);
    ok.then(|| p.display().to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn pdf_through_the_python_seam_and_a_hung_engine_is_killed() {
    let Some(py) = weasy_python() else {
        println!("SKIPPED: no interpreter with weasyprint on this host");
        return;
    };
    let rig = Rig::new("pdf").await;
    let smtp = FakeSmtp::start().await;
    let port = smtp.port.to_string();
    let (state, _) = (rig.dir.path().join("pdf.state"), 0);
    let state_s = state.display().to_string();
    let mut env = email_env(&smtp, &port, &[("DOC_PDF_ENGINE", "weasyprint"), ("DOC_PDF_PYTHON", &py), ("PDF_WORKER_FAULT", "hang"), ("PDF_WORKER_FAULT_TIMES", "1"), ("PDF_WORKER_FAULT_STATE", &state_s), ("DOC_PDF_DEADLINE_MS", "20000")]);
    env.push(("DOC_JOBD_MAX_ATTEMPTS", "4"));
    let d = rig.daemon("d", &env).await;
    let req = jude_request(json!({"generate_docx": false, "generate_pdf": true, "email_address": "t@example.org"}));
    let r = rig.c.post(format!("{}/documents", d.base())).json(&req).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 200);
    let id = r.json::<Value>().await.unwrap()["task_id"].as_str().unwrap().to_string();
    let (seen, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(180)).await;
    assert_eq!(last["state"], "SUCCESS", "{seen:?}\n{}", tail_log(&d.log_text()));
    assert!(seen.iter().any(|s| s == "Converting to PDF"), "{seen:?}");
    let key = last["result"].as_str().unwrap();
    let pdf = std::fs::read(rig.out().join(format!("{key}.pdf"))).unwrap();
    assert!(pdf.starts_with(b"%PDF-") && pdf.len() > 1000, "{} bytes", pdf.len());
    assert!(rig.out().join(format!("{key}.html")).exists());
    assert_eq!(smtp.count(), 1, "the PDF was emailed once");
    assert!(smtp.received.lock().unwrap()[0].contains(".pdf"));
}
