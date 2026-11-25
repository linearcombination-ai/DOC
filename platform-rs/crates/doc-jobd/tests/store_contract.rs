#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! One contract, two stores: the same scenarios run against the in-memory store and against Redis.
//! What the memory store passes here is what its single-process mode can promise; what only Redis can
//! show (a second process, a SIGKILL, a restart) is in `faults.rs`.

mod support;
use std::time::Duration;

use doc_jobs::mem_store::MemStore;
use doc_jobs::model::{JobFailure, FAILURE, PENDING, SUCCESS};
use doc_jobs::redis_store::RedisStore;
use doc_jobs::store::{JobStore, NewJob};
use support::*;

const TTL: Duration = Duration::from_secs(60);

fn job(id: &str, dedup: &str) -> NewJob {
    NewJob { task_id: id.into(), dedup_key: dedup.into(), kind: "docx".into(), payload: "{}".into(), request_id: "req-test".into() }
}

async fn contract<S: JobStore>(s: &S, label: &str) {
    // enqueue, coalesce, claim
    let a = s.enqueue(job("t1", "k1"), TTL).await.unwrap();
    let b = s.enqueue(job("t2", "k1"), TTL).await.unwrap();
    assert_eq!((a.task_id.as_str(), a.coalesced, b.task_id.as_str(), b.coalesced), ("t1", false, "t1", true), "{label}: coalesce onto the in-flight job");
    assert_eq!(s.status("t1").await.unwrap().unwrap().state, PENDING);
    assert!(s.status("nope").await.unwrap().is_none());
    let c1 = s.next("w", 1, Duration::from_millis(200), Duration::from_millis(300)).await.unwrap().expect("claimed");
    assert_eq!((c1.task_id.as_str(), c1.attempt), ("t1", 1), "{label}");
    assert!(s.next("w", 2, Duration::from_millis(100), TTL).await.unwrap().is_none(), "{label}: nothing else queued");

    // progress is ordered state; the lease holder may write, a stranger may not
    assert!(s.set_progress(&c1.lease, "Locating assets").await.unwrap());
    assert_eq!(s.status("t1").await.unwrap().unwrap().state, "Locating assets");
    let mut stranger = c1.lease.clone();
    stranger.owner = "someone-else#9".into();
    assert!(!s.set_progress(&stranger, "evil").await.unwrap(), "{label}: fencing on progress");
    assert!(!s.complete(&stranger, "evil", TTL).await.unwrap(), "{label}: fencing on complete");

    // the lease lapses (worker died): the job is taken back and a second claim gets attempt 2; the first
    // owner is fenced out of every later write
    tokio::time::sleep(Duration::from_millis(500)).await;
    let n = s.reap_stalled("other", Duration::from_millis(100), 10).await.unwrap();
    // redis' heartbeat may re-take a lapsed lease nobody else claimed (that is a feature); only in that
    // case nothing is re-queued
    if n == 0 {
        assert!(s.heartbeat(&c1.lease, TTL).await.unwrap());
        assert!(s.complete(&c1.lease, "k1-result", TTL).await.unwrap());
    } else {
        assert_eq!(n, 1, "{label}");
        let c2 = s.next("w2", 3, Duration::from_millis(500), TTL).await.unwrap().expect("re-claimed");
        assert_eq!(c2.attempt, 2, "{label}");
        assert!(!s.complete(&c1.lease, "stale", TTL).await.unwrap(), "{label}: the first owner is fenced");
        assert!(!s.heartbeat(&c1.lease, TTL).await.unwrap(), "{label}: and its heartbeat says so");
        assert!(s.complete(&c2.lease, "k1-result", TTL).await.unwrap());
    }
    let v = s.status("t1").await.unwrap().unwrap();
    assert_eq!((v.state.as_str(), v.result.as_deref()), (SUCCESS, Some("k1-result")), "{label}");
    assert_eq!(v.body(), serde_json::json!({"state": "SUCCESS", "result": "k1-result"}), "{label}: legacy body");

    // completion frees the in-flight claim: the same key is a new job
    let again = s.enqueue(job("t3", "k1"), TTL).await.unwrap();
    assert_eq!((again.task_id.as_str(), again.coalesced), ("t3", false), "{label}");
    let c3 = s.next("w", 4, Duration::from_millis(300), TTL).await.unwrap().unwrap();

    // transient failure: parked, then promoted after its delay, attempt counted
    let last = JobFailure { code: "repo_unavailable".into(), message: "x".into(), retryable: true };
    assert!(s.retry_later(&c3.lease, Duration::from_millis(400), &last).await.unwrap());
    let v = s.status("t3").await.unwrap().unwrap();
    assert!(v.retrying, "{label}");
    assert_eq!(s.promote_due(10).await.unwrap(), 0, "{label}: not due yet");
    assert!(s.next("w", 5, Duration::from_millis(100), TTL).await.unwrap().is_none());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(s.promote_due(10).await.unwrap(), 1, "{label}");
    let c4 = s.next("w", 6, Duration::from_millis(500), TTL).await.unwrap().unwrap();
    assert_eq!(c4.attempt, 2, "{label}");

    // release (graceful shutdown) does not count the attempt
    assert!(s.release(&c4.lease).await.unwrap());
    let c5 = s.next("w", 7, Duration::from_millis(500), TTL).await.unwrap().unwrap();
    assert_eq!(c5.attempt, 2, "{label}: released, not counted");

    // permanent failure: legacy FAILURE state, extended result
    let f = JobFailure { code: "resource_not_found".into(), message: "m".into(), retryable: false };
    assert!(s.fail(&c5.lease, &f, TTL).await.unwrap());
    let v = s.status("t3").await.unwrap().unwrap();
    assert_eq!(v.state, FAILURE);
    assert_eq!(v.body(), serde_json::json!({"state": "FAILURE", "result": {"code": "resource_not_found", "message": "m", "retryable": false}}), "{label}");
    let h = s.health().await.unwrap();
    assert_eq!((h.running, h.delayed), (0, 0), "{label}");
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_store_contract() {
    contract(&MemStore::new(), "memory").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn redis_store_contract() {
    let dir = TempDir::new("contract");
    let redis = Redis::start(&dir.path().join("redis"), false).await;
    let store = RedisStore::connect(&redis.url(), "c").await.unwrap();
    contract(&store, "redis").await;
}
