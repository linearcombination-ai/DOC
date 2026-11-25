#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The interleaved document model against DOC's own output for the same request: the oracle record
//! `lbo_en_usfm_tn_jud` (DOC's `assemble_content` parts, from the interleave oracle corpus).

mod support;
use std::time::Duration;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn parts_from_the_pipeline_match_docs_oracle_for_en_ulb_with_tn_jude() {
    let dir = TempDir::new("oracle");
    let redis = Redis::start(&dir.path().join("redis"), false).await;
    let catalog = Catalog::start(vec![english_catalog()]).await;
    let git = FakeGit::install(dir.path());
    let s = setup(dir.path(), &redis, &catalog, &git);
    let d = Daemon::spawn(&s, "d", &[("DOC_JOBD_STRUCTURE", "as_built"), ("DOC_JOBD_DUMP_PARTS", "1")]).await;
    let c = reqwest::Client::new();
    let (_, body, _) = post_docx(&c, &d.base(), &jude_request(serde_json::json!({}))).await;
    let id = body["task_id"].as_str().unwrap().to_string();
    let (_, last) = watch(&c, &d.base(), &id, Duration::from_secs(60)).await;
    assert_eq!(last["state"], "SUCCESS");
    let key = last["result"].as_str().unwrap();
    let parts = s.output.join(format!("{key}.parts.json"));
    let oracle = p5_root().join("fixtures/oracle_lbo_en_usfm_tn_jud.json");
    let out = std::process::Command::new("python3").arg("-I").arg(p5_root().join("scripts/compare_parts.py")).arg(&parts).arg(&oracle).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    println!("{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{text}");
}
