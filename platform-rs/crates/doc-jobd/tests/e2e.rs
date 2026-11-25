#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End to end on this host: real Redis, real doc-jobd, fake git over fixture content, the Python docx worker.

mod support;
use std::time::Duration;
use support::*;

fn expected_key() -> String {
    // scripts/key_oracle.py computed it with DOC's own request_key.py; the fixture holds the docx variant
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/key_cases.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v["slice_docx_key"].as_str().unwrap().to_string()
}

pub const GOLDEN: [&str; 6] = ["Receiving request", "Locating assets", "Provisioning asset files", "Parsing asset files", "Assembling content", "Converting to Docx"];

#[tokio::test(flavor = "multi_thread")]
async fn golden_status_sequence_and_a_valid_document() {
    let dir = TempDir::new("golden");
    let redis = Redis::start(&dir.path().join("redis"), true).await;
    let catalog = Catalog::start(vec![english_catalog()]).await;
    let git = FakeGit::install(dir.path());
    let s = setup(dir.path(), &redis, &catalog, &git);
    let d = Daemon::spawn(&s, "d1", &[]).await;
    let c = reqwest::Client::new();

    let (code, body, _) = post_docx(&c, &d.base(), &jude_request(serde_json::json!({}))).await;
    assert_eq!(code, 200, "{body}");
    let id = body["task_id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 36);

    let (seen, last) = watch(&c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS", "states {seen:?} last {last}");
    let key = last["result"].as_str().unwrap();
    assert_eq!(key, expected_key(), "the key DOC's request_key.py computes for this request (generate_docx is hashed)");

    // what a poller saw: only legal strings, never out of order
    let legal: Vec<&str> = GOLDEN.iter().copied().chain(["PENDING", "SUCCESS"]).collect();
    let rank = |x: &str| if x == "PENDING" { 0 } else { GOLDEN.iter().position(|g| *g == x).map(|i| i + 1).unwrap_or(7) };
    assert!(seen.iter().all(|x| legal.contains(&x.as_str())), "illegal state in {seen:?}");
    assert!(seen.windows(2).all(|w| rank(&w[0]) < rank(&w[1])), "states out of order: {seen:?}");

    // the exact sequence, from the audit trail
    let mut rc = redis.conn().await;
    let hist: Vec<String> = redis::cmd("LRANGE").arg(format!("t:hist:{id}")).arg(0).arg(-1).query_async(&mut rc).await.unwrap();
    assert_eq!(hist, GOLDEN.to_vec(), "progress strings, in order, exactly DOC's");

    // the output
    let docx = s.output.join(format!("{key}.docx"));
    let v = check_docx(&docx);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["has_jude"], true);
    assert!(v["paragraphs"].as_u64().unwrap() > 100, "{v}");
    assert!(s.output.join(format!("{key}.findings.json")).exists());
    // no scratch left
    let left: Vec<_> = std::fs::read_dir(s.output.join(".scratch")).map(|r| r.flatten().collect()).unwrap_or_default();
    assert!(left.is_empty(), "scratch left behind: {left:?}");
    println!("states seen by polling: {seen:?}\nhistory: {hist:?}\ndocx: {v}");
}

/// The Redis-less local mode, end to end: same daemon, `DOC_JOBD_STORE=memory`, no Redis process at all.
#[tokio::test(flavor = "multi_thread")]
async fn memory_store_runs_the_whole_slice_without_redis() {
    let dir = TempDir::new("mem");
    let redis = Redis::start(&dir.path().join("redis"), false).await; // only to build Setup; the daemon never connects
    let catalog = Catalog::start(vec![english_catalog()]).await;
    let git = FakeGit::install(dir.path());
    let s = setup(dir.path(), &redis, &catalog, &git);
    let mut redis = redis;
    redis.kill9();
    let d = Daemon::spawn(&s, "mem", &[("DOC_JOBD_STORE", "memory")]).await;
    let c = reqwest::Client::new();
    let (code, body, _) = post_docx(&c, &d.base(), &jude_request(serde_json::json!({}))).await;
    assert_eq!(code, 200);
    let id = body["task_id"].as_str().unwrap().to_string();
    let (seen, last) = watch(&c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS", "{seen:?}");
    assert_eq!(last["result"], expected_key());
    let rank = |x: &str| if x == "PENDING" { 0 } else { GOLDEN.iter().position(|g| *g == x).map(|i| i + 1).unwrap_or(7) };
    assert!(seen.windows(2).all(|w| rank(&w[0]) < rank(&w[1])), "{seen:?}");
    let v = check_docx(&s.output.join(format!("{}.docx", last["result"].as_str().unwrap())));
    assert_eq!(v["ok"], true);
    let h: serde_json::Value = c.get(format!("{}/health/status", d.base())).send().await.unwrap().json().await.unwrap();
    assert_eq!(h[0]["queue"]["store"], "memory");
}
