#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The fault-injection suite: every production requirement of the brief, on this host, no docker.
//! Real Redis 8.12 (built here), real `doc-jobd` subprocesses, a real Python docx worker; only git
//! (a script that copies fixtures) and the catalog (a scripted HTTP server) are fakes.

mod support;
use std::time::{Duration, Instant};

use futures::future::join_all;
use serde_json::{json, Value};
use support::*;

const GOLDEN: [&str; 6] = ["Receiving request", "Locating assets", "Provisioning asset files", "Parsing asset files", "Assembling content", "Converting to Docx"];

struct Rig {
    dir: TempDir,
    redis: Redis,
    catalog: Catalog,
    git: FakeGit,
    c: reqwest::Client,
}

impl Rig {
    async fn new(tag: &str) -> Rig {
        let dir = TempDir::new(tag);
        let redis = Redis::start(&dir.path().join("redis"), true).await;
        let catalog = Catalog::start(vec![english_catalog()]).await;
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
    async fn hget(&self, id: &str, field: &str) -> Option<String> {
        let mut c = self.redis.conn().await;
        redis::cmd("HGET").arg(format!("t:job:{id}")).arg(field).query_async(&mut c).await.unwrap()
    }
    async fn hist(&self, id: &str) -> Vec<String> {
        let mut c = self.redis.conn().await;
        redis::cmd("LRANGE").arg(format!("t:hist:{id}")).arg(0).arg(-1).query_async(&mut c).await.unwrap()
    }
    async fn wait_hist_contains(&self, id: &str, what: &str, within: Duration) {
        let end = Instant::now() + within;
        while !self.hist(id).await.iter().any(|h| h == what) {
            assert!(Instant::now() < end, "never reached '{what}'");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    fn docx_files(&self) -> Vec<String> {
        std::fs::read_dir(self.setup().output).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".docx")).collect()).unwrap_or_default()
    }
    fn scratch_dirs(&self) -> Vec<String> {
        std::fs::read_dir(self.setup().output.join(".scratch")).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default()
    }
}

fn pids_in(path: &std::path::Path) -> Vec<i32> {
    std::fs::read_to_string(path).unwrap_or_default().split_whitespace().filter_map(|s| s.parse().ok()).collect()
}

async fn wait_gone(pids: &[i32], within: Duration) -> bool {
    let end = Instant::now() + within;
    loop {
        if pids.iter().all(|p| !pid_alive(*p)) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn retry_delays(log: &str) -> Vec<(u32, u64)> {
    log_events(log)
        .into_iter()
        .filter(|(m, _)| m == "attempt failed (transient); will retry")
        .filter_map(|(_, f)| Some((u32::try_from(f["attempt"].as_u64()?).ok()?, f["retry_in_ms"].as_u64()?)))
        .collect()
}

fn log_has(log: &str, code: &str) -> bool {
    log_events(log).iter().any(|(_, f)| f["code"].as_str() == Some(code))
}

// ----------------------------------------------------------------------------- coalescing

#[tokio::test(flavor = "multi_thread")]
async fn identical_requests_in_flight_coalesce_to_one_job_and_one_output() {
    let rig = Rig::new("coalesce").await;
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "slow:1.5"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &rig.dir.path().join("fs").display().to_string())]).await;
    let body = jude_request(json!({}));
    let base = d.base();
    let results = join_all((0..8).map(|_| post_docx(&rig.c, &base, &body))).await;
    let ids: std::collections::BTreeSet<String> = results.iter().map(|(_, b, _)| b["task_id"].as_str().unwrap().to_string()).collect();
    let coalesced = results.iter().filter(|(_, _, h)| h.contains_key("x-doc-coalesced")).count();
    assert_eq!(ids.len(), 1, "8 identical submissions must share one task id: {ids:?}");
    assert_eq!(coalesced, 7, "all but the first are marked coalesced");
    let id = ids.into_iter().next().unwrap();
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("1"), "one run");
    assert_eq!(rig.hist(&id).await, GOLDEN.to_vec());
    assert_eq!(rig.docx_files().len(), 1, "one output");
    assert_eq!(rig.git.calls(), 2, "one clone per repository, once");

    // the in-flight claim is released on completion: a new identical request is a new task, and a cache hit
    let id2 = rig.submit(&d, &body).await;
    assert_ne!(id, id2);
    let (_, last2) = watch(&rig.c, &d.base(), &id2, Duration::from_secs(30)).await;
    assert_eq!(last2["state"], "SUCCESS");
    assert_eq!(last2["result"], last["result"]);
    assert_eq!(rig.hist(&id2).await, vec!["Receiving request"], "a fresh document is served again without rebuilding");
    assert_eq!(rig.git.calls(), 2);
    println!("coalesced: ids=1 marked={coalesced}; second submission hist={:?}", rig.hist(&id2).await);
}

// ----------------------------------------------------------------------------- errors: transient and permanent

#[tokio::test(flavor = "multi_thread")]
async fn transient_failures_retry_with_jittered_backoff_and_the_job_still_ends_with_a_document() {
    let rig = Rig::new("retry").await;
    rig.git.flaky(3);
    let d = rig.daemon("d", &[("DOC_SHELL_CLONE_ATTEMPTS", "1"), ("DOC_JOBD_BACKOFF_BASE_MS", "400"), ("DOC_JOBD_BACKOFF_CAP_MS", "2000")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let (seen, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS", "{seen:?}");
    assert!(!seen.iter().any(|s| s == "FAILURE"), "the UI ends polling on FAILURE; a retrying job must never show it: {seen:?}");
    let attempt: u32 = rig.hget(&id, "attempt").await.unwrap().parse().unwrap();
    assert!(attempt >= 2, "attempts {attempt}");
    let delays = retry_delays(&d.log_text());
    assert!(!delays.is_empty());
    for (a, ms) in &delays {
        let ceiling = (400u64 << (a - 1)).min(2000);
        assert!(*ms >= 100 && *ms <= ceiling, "attempt {a}: wait {ms} ms outside [100, {ceiling}]");
    }
    println!("attempts={attempt} retry waits (attempt, ms): {delays:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn permanent_errors_fail_at_once_with_the_extended_result_and_the_legacy_shape_elsewhere() {
    let rig = Rig::new("perm").await;
    let d = rig.daemon("d", &[]).await;
    // a language the catalog does not have: DOC leaves the request out and builds the document anyway (its own e2e test expects a file);
    // so does this build, and the findings say why
    let body = json!({"assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": true, "resource_requests": [{"lang_code": "fr", "resource_type": "ulb", "book_code": "jud"}]});
    let id = rig.submit(&d, &body).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(30)).await;
    assert_eq!(last["state"], "SUCCESS", "{last}");
    let key = last["result"].as_str().unwrap().to_string();
    let findings = std::fs::read_to_string(rig.setup().output.join(format!("{key}.findings.json"))).unwrap();
    assert!(findings.contains("resource_not_in_catalog"), "the request left out is reported: {findings}");
    // a repository the catalog has, with no file for the requested book: nothing to build a document from, and waiting will not change that: permanent
    let body = json!({"assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": true, "resource_requests": [{"lang_code": "en", "resource_type": "ulb", "book_code": "mat"}]});
    let id = rig.submit(&d, &body).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(30)).await;
    assert_eq!(last["state"], "FAILURE");
    assert_eq!(last["result"]["code"], "resource_unavailable");
    assert_eq!(last["result"]["retryable"], false);
    assert!(last["result"]["message"].as_str().unwrap().contains("none of the requested resources"));
    assert_eq!(last.as_object().unwrap().len(), 2, "legacy shape: state and result only");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("1"), "no retry for a permanent error");
    assert!(retry_delays(&d.log_text()).is_empty());
    // an invalid request never becomes a job: 422 with FastAPI's `detail`
    let bad = json!({"assembly_strategy_kind": "lbo", "resource_requests": [{"lang_code": "en", "resource_type": "ulb", "book_code": "zzz"}]});
    let (code, b, _) = post_docx(&rig.c, &d.base(), &bad).await;
    assert_eq!(code, 422, "{b}");
    assert!(b["detail"].is_array());
    // unknown task id: Celery says PENDING
    let (st, unknown) = status_of(&rig.c, &d.base(), "no-such-task").await;
    assert_eq!((st, unknown), (200, json!({"state": "PENDING"})));
    // health: legacy tuple with the queue added
    let h: Value = rig.c.get(format!("{}/health/status", d.base())).send().await.unwrap().json().await.unwrap();
    assert_eq!(h[0]["status"], "ok");
    assert_eq!(h[1], 200);
    assert_eq!(h[0]["queue"]["ok"], true);
    println!("failure body: {last}\nhealth: {h}");
}

#[tokio::test(flavor = "multi_thread")]
async fn attempts_run_out_as_retryable_failure_and_a_later_resubmission_succeeds() {
    let rig = Rig::new("exhaust").await;
    rig.git.mode("fail");
    let d = rig.daemon("d", &[("DOC_JOBD_MAX_ATTEMPTS", "3"), ("DOC_SHELL_CLONE_ATTEMPTS", "1"), ("DOC_JOBD_BACKOFF_BASE_MS", "100"), ("DOC_JOBD_BACKOFF_CAP_MS", "300")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "FAILURE");
    assert_eq!(last["result"]["code"], "repo_unavailable");
    assert_eq!(last["result"]["retryable"], true, "the cause was transient: the client may resubmit");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("3"));
    rig.git.mode("ok");
    let id2 = rig.submit(&d, &jude_request(json!({}))).await;
    assert_ne!(id, id2, "a failed job releases its in-flight claim");
    let (_, ok) = watch(&rig.c, &d.base(), &id2, Duration::from_secs(60)).await;
    assert_eq!(ok["state"], "SUCCESS");
}

#[tokio::test(flavor = "multi_thread")]
async fn catalog_outage_is_waited_out_not_failed() {
    let rig = Rig::new("catalog").await;
    rig.catalog.set(vec!["status:503".into()]);
    let d = rig.daemon("d", &[("DOC_SHELL_CATALOG_ATTEMPTS", "1"), ("DOC_JOBD_BACKOFF_BASE_MS", "200"), ("DOC_JOBD_BACKOFF_CAP_MS", "600"), ("DOC_JOBD_MAX_ATTEMPTS", "20")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let (_, mid) = status_of(&rig.c, &d.base(), &id).await;
    assert_ne!(mid["state"], "FAILURE", "still trying: {mid}");
    rig.catalog.set(vec![english_catalog()]);
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    let attempt: u32 = rig.hget(&id, "attempt").await.unwrap().parse().unwrap();
    assert!(attempt >= 3, "attempts {attempt}");
    println!("catalog outage: {attempt} attempts, then SUCCESS");
}

// ----------------------------------------------------------------------------- deadlines kill hung work

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_docx_worker_and_its_grandchild_are_killed_and_the_job_completes_on_retry() {
    let rig = Rig::new("hang").await;
    let pidfile = rig.dir.path().join("docx.pid");
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "hang-grandchild"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string()), ("DOCX_WORKER_PIDFILE", &pidfile.display().to_string()), ("DOC_JOBD_DOCX_DEADLINE_MS", "1500")]).await;
    let t0 = Instant::now();
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    // wait until the hung worker exists, remember both pids
    let end = Instant::now() + Duration::from_secs(20);
    let pids = loop {
        let p = pids_in(&pidfile);
        if p.len() == 2 {
            break p;
        }
        assert!(Instant::now() < end, "worker never started");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(pids.iter().all(|p| pid_alive(*p)), "alive while hung: {pids:?}");
    assert!(wait_gone(&pids, Duration::from_secs(8)).await, "worker {} and grandchild {} must be gone after the deadline", pids[0], pids[1]);
    let killed_after = t0.elapsed();
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("2"));
    assert!(log_has(&d.log_text(), "docx_worker_timeout"));
    println!("hung worker pid {} + grandchild {} gone {} ms after submit (deadline 1500 ms); job completed on attempt 2", pids[0], pids[1], killed_after.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_clone_is_killed_at_the_clone_timeout_and_the_job_completes_once_git_recovers() {
    let rig = Rig::new("stage").await;
    rig.git.mode("hang");
    // the acquirer works to its own deadline and keeps a hung fetch alive in the background for the next
    // request (P4 design), so what kills the hung git is the per-attempt clone timeout, here 2 s
    let d = rig.daemon("d", &[("DOC_SHELL_CLONE_TIMEOUT_SECS", "2"), ("DOC_SHELL_CLONE_ATTEMPTS", "1"), ("DOC_JOBD_BACKOFF_BASE_MS", "300")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let end = Instant::now() + Duration::from_secs(15);
    let hung = loop {
        let p = pids_in(&rig.git.state.join("hang.pid"));
        if !p.is_empty() {
            break p;
        }
        assert!(Instant::now() < end, "git never started");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(pid_alive(hung[0]));
    assert!(wait_gone(&hung, Duration::from_secs(8)).await, "hung git {hung:?} must be killed at the clone timeout");
    let (seen, mid) = (status_of(&rig.c, &d.base(), &id).await.1, ());
    let _ = mid;
    assert_ne!(seen["state"], "FAILURE");
    rig.git.mode("ok");
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    println!("hung git pid {hung:?} killed at the clone timeout; job completed after git recovered");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_per_attempt_ceiling_kills_a_slow_docx_worker() {
    let rig = Rig::new("ceiling").await;
    let pidfile = rig.dir.path().join("docx.pid");
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "slow:30"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string()), ("DOCX_WORKER_PIDFILE", &pidfile.display().to_string()), ("DOC_JOBD_JOB_CEILING_MS", "2500")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let end = Instant::now() + Duration::from_secs(20);
    let pids = loop {
        let p = pids_in(&pidfile);
        if !p.is_empty() {
            break p;
        }
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(wait_gone(&pids, Duration::from_secs(10)).await, "worker {pids:?} must die with its attempt");
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert!(log_has(&d.log_text(), "job_ceiling"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crashing_docx_worker_is_transient_and_a_rejecting_one_is_permanent() {
    let rig = Rig::new("crash").await;
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "crash"), ("DOCX_WORKER_FAULT_TIMES", "2"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string())]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("3"), "two segfaults, then success");
    assert!(log_has(&d.log_text(), "docx_worker_crashed"));
    // a version the worker does not know: exit 2, permanent
    let rig2 = Rig::new("reject").await;
    let spec = rig2.dir.path().join("worker.py");
    std::fs::write(&spec, "import sys\nprint('unsupported DocxJob version', file=sys.stderr)\nsys.exit(2)\n").unwrap();
    let d2 = rig2.daemon("d", &[("DOC_JOBD_DOCX_WORKER", &spec.display().to_string())]).await;
    let id2 = rig2.submit(&d2, &jude_request(json!({}))).await;
    let (_, last2) = watch(&rig2.c, &d2.base(), &id2, Duration::from_secs(60)).await;
    assert_eq!(last2["state"], "FAILURE");
    assert_eq!(last2["result"]["code"], "docx_input_rejected");
    assert_eq!(last2["result"]["retryable"], false);
    assert_eq!(rig2.hget(&id2, "attempt").await.as_deref(), Some("1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_truncated_docx_is_never_placed() {
    let rig = Rig::new("truncated").await;
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "bad-output"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string())]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert!(log_has(&d.log_text(), "docx_output_invalid"), "the first, truncated output was refused");
    let key = last["result"].as_str().unwrap();
    assert_eq!(check_docx(&rig.setup().output.join(format!("{key}.docx")))["ok"], true);
}

// ----------------------------------------------------------------------------- worker death, Redis restart, shutdown

#[tokio::test(flavor = "multi_thread")]
async fn sigkill_of_the_worker_mid_job_the_job_completes_once_and_the_output_is_valid() {
    let rig = Rig::new("sigkill").await;
    let pidfile = rig.dir.path().join("docx.pid");
    let state = rig.dir.path().join("fault.state");
    let faults: Vec<(&str, String)> = vec![("DOCX_WORKER_FAULT", "slow:6".into()), ("DOCX_WORKER_FAULT_TIMES", "1".into()), ("DOCX_WORKER_FAULT_STATE", state.display().to_string()), ("DOCX_WORKER_PIDFILE", pidfile.display().to_string())];
    let ex: Vec<(&str, &str)> = faults.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut a = rig.daemon("a", &ex).await;
    let id = rig.submit(&a, &jude_request(json!({}))).await;
    rig.wait_hist_contains(&id, "Converting to Docx", Duration::from_secs(30)).await;
    let end = Instant::now() + Duration::from_secs(10);
    let orphan = loop {
        let p = pids_in(&pidfile);
        if p.len() == 1 {
            break p;
        }
        assert!(Instant::now() < end, "docx worker never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let t_kill = Instant::now();
    a.sigkill();
    // nobody is left to kill the python worker's group: it must notice and leave by itself
    assert!(wait_gone(&orphan, Duration::from_secs(5)).await, "orphaned docx worker {orphan:?} must exit when its parent dies");
    let orphan_gone_ms = t_kill.elapsed().as_millis();
    // a second worker takes over after the lease lapses
    let b = rig.daemon("b", &ex).await;
    let (_, last) = watch(&rig.c, &b.base(), &id, Duration::from_secs(60)).await;
    let recovery_ms = t_kill.elapsed().as_millis();
    assert_eq!(last["state"], "SUCCESS");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("2"), "started twice, finished once");
    let mut rc = rig.redis.conn().await;
    let stalled: Option<u64> = redis::cmd("HGET").arg("t:stats").arg("stalled_recovered").query_async(&mut rc).await.unwrap();
    assert!(stalled.unwrap_or(0) >= 1);
    let key = last["result"].as_str().unwrap();
    let v = check_docx(&rig.setup().output.join(format!("{key}.docx")));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(rig.docx_files().len(), 1, "one output");
    let end = Instant::now() + Duration::from_secs(5);
    while !rig.scratch_dirs().is_empty() && Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(rig.scratch_dirs().is_empty(), "the dead attempt's scratch was removed: {:?}", rig.scratch_dirs());
    println!("SIGKILL: orphan docx worker gone after {orphan_gone_ms} ms; SUCCESS {recovery_ms} ms after the kill (lease ttl 2000 ms); attempt 2; docx {v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn redis_restart_mid_job_the_job_finishes_and_state_survives() {
    let mut rig = Rig::new("redisrestart").await;
    let state = rig.dir.path().join("fault.state");
    let d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "slow:4"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string()), ("DOC_JOBD_LEASE_TTL_MS", "6000")]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.wait_hist_contains(&id, "Converting to Docx", Duration::from_secs(30)).await;
    // a second request queued behind it must survive too
    let id2 = rig.submit(&d, &jude_request(json!({"use_chapter_labels": true}))).await;
    rig.redis.shutdown().await;
    // while Redis is down: the API answers typed 503s, it does not crash or hang
    let (code, body, _) = post_docx(&rig.c, &d.base(), &jude_request(json!({"layout_for_print": true}))).await;
    assert_eq!(code, 503, "{body}");
    assert_eq!(body["code"], "queue_unavailable");
    assert_eq!(body["retryable"], true);
    let (st, _) = status_of(&rig.c, &d.base(), &id).await;
    assert_eq!(st, 503);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    rig.redis.restart(true).await;
    let (_, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    let (_, last2) = watch(&rig.c, &d.base(), &id2, Duration::from_secs(60)).await;
    assert_eq!(last2["state"], "SUCCESS");
    let key = last["result"].as_str().unwrap();
    assert_eq!(check_docx(&rig.setup().output.join(format!("{key}.docx")))["ok"], true);
    let attempt = rig.hget(&id, "attempt").await.unwrap();
    println!("redis restart: job 1 attempt {attempt} (1 = it never noticed), job 2 SUCCESS; docx files {}", rig.docx_files().len());
}

#[tokio::test(flavor = "multi_thread")]
async fn sigterm_drains_the_running_job_before_exit() {
    let rig = Rig::new("drain").await;
    let state = rig.dir.path().join("fault.state");
    let mut d = rig.daemon("d", &[("DOCX_WORKER_FAULT", "slow:3"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &state.display().to_string())]).await;
    let id = rig.submit(&d, &jude_request(json!({}))).await;
    rig.wait_hist_contains(&id, "Converting to Docx", Duration::from_secs(30)).await;
    let t = Instant::now();
    d.sigterm();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(d.child.try_wait().unwrap().is_none(), "the process must still be running while the job drains");
    let status = d.wait_exit(Duration::from_secs(30)).await.expect("exits after the drain");
    assert!(status.success(), "{status:?}");
    let drained_ms = t.elapsed().as_millis();
    // the job finished before the process went away
    let mut rc = rig.redis.conn().await;
    let st: Option<String> = redis::cmd("HGET").arg(format!("t:job:{id}")).arg("st").query_async(&mut rc).await.unwrap();
    assert_eq!(st.as_deref(), Some("done"));
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("1"));
    assert!(log_events(&d.log_text()).iter().any(|(m, _)| m == "draining"));
    println!("SIGTERM: exited {drained_ms} ms after the signal, with the job done (attempt 1)");
}

#[tokio::test(flavor = "multi_thread")]
async fn when_the_drain_deadline_passes_the_job_goes_back_to_the_queue_for_another_worker() {
    let rig = Rig::new("drainlate").await;
    let pidfile = rig.dir.path().join("docx.pid");
    let state = rig.dir.path().join("fault.state");
    let ex = [("DOCX_WORKER_FAULT", "slow:20"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", &*state.display().to_string()), ("DOCX_WORKER_PIDFILE", &*pidfile.display().to_string()), ("DOC_JOBD_DRAIN_DEADLINE_MS", "800")];
    let mut a = rig.daemon("a", &ex).await;
    let id = rig.submit(&a, &jude_request(json!({}))).await;
    rig.wait_hist_contains(&id, "Converting to Docx", Duration::from_secs(30)).await;
    let pids = pids_in(&pidfile);
    a.sigterm();
    let status = a.wait_exit(Duration::from_secs(20)).await.expect("exits after the short drain");
    assert!(status.success());
    assert!(wait_gone(&pids, Duration::from_secs(3)).await, "the docx worker died with the drain");
    let mut rc = rig.redis.conn().await;
    let st: Option<String> = redis::cmd("HGET").arg(format!("t:job:{id}")).arg("st").query_async(&mut rc).await.unwrap();
    assert_eq!(st.as_deref(), Some("queued"), "released, not failed");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("0"), "a release does not count as an attempt");
    let b = rig.daemon("b", &ex).await;
    let (_, last) = watch(&rig.c, &b.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    assert_eq!(rig.hget(&id, "attempt").await.as_deref(), Some("1"));
}

// ----------------------------------------------------------------------------- isolation

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_jobs_get_separate_scratch_directories_and_separate_outputs() {
    let rig = Rig::new("scratch").await;
    let d = rig.daemon("d", &[("DOC_JOBD_CONCURRENCY", "4"), ("DOCX_WORKER_FAULT", "slow:1.5")]).await;
    let bodies: Vec<Value> = vec![
        jude_request(json!({})),
        jude_request(json!({"use_chapter_labels": true})),
        jude_request(json!({"layout_for_print": true})),
        jude_request(json!({"show_tn_book_intro": false})),
        jude_request(json!({"use_section_visual_separator": true})),
        jude_request(json!({"use_chapter_labels": true, "layout_for_print": true})),
    ];
    let ids: Vec<String> = join_all(bodies.iter().map(|b| rig.submit(&d, b))).await;
    let mut max_concurrent = 0;
    let mut names = std::collections::BTreeSet::new();
    let end = Instant::now() + Duration::from_secs(90);
    loop {
        let now = rig.scratch_dirs();
        max_concurrent = max_concurrent.max(now.len());
        names.extend(now);
        let mut done = 0;
        for id in &ids {
            let (_, b) = status_of(&rig.c, &d.base(), id).await;
            if b["state"] == "SUCCESS" {
                done += 1;
            }
            assert_ne!(b["state"], "FAILURE", "{b}");
        }
        if done == ids.len() {
            break;
        }
        assert!(Instant::now() < end, "jobs did not finish");
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(max_concurrent >= 3, "jobs must have overlapped (saw {max_concurrent} scratch dirs at once)");
    assert!(names.len() >= 6, "every attempt had its own directory: {names:?}");
    assert_eq!(rig.docx_files().len(), 6, "six different keys, six outputs");
    for f in rig.docx_files() {
        assert_eq!(check_docx(&rig.setup().output.join(&f))["ok"], true, "{f}");
    }
    assert!(rig.scratch_dirs().is_empty());
    println!("scratch: up to {max_concurrent} at once, {} distinct names, 6 valid outputs", names.len());
}
