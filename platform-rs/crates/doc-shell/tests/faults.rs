#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Fault-injection tests: the production requirements of the brief, each against the real
//! adapters, a scriptable mock data API and a fake `git` executable.
//!
//! The requirement they prove: a transient condition ends with the resource available (retry,
//! stale-if-error, background completion), a failure is never cached, and what could not be fetched
//! is reported, not dropped.

mod support;

use std::time::{Duration, Instant};

use doc_contract::Mode;
use serde_json::json;
use support::*;

fn book_names(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|a| a.iter().map(|e| (e[0].as_str().unwrap_or("").to_string(), e[1].as_str().unwrap_or("").to_string())).collect())
        .unwrap_or_default()
}

fn findings_codes(v: &serde_json::Value) -> Vec<String> {
    v["findings"].as_array().map(|a| a.iter().filter_map(|f| f["code"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

const BOOKS: &str = "/book_codes_for_lang/xx";
const BOOKS_F: &str = "/findings/book_codes_for_lang/xx";

// ----------------------------------------------------------------------------- data API

#[tokio::test]
async fn data_api_500_then_recovering_succeeds_within_one_request() {
    let env = start(vec![Step::Status(500), Step::Status(503), Step::Ok(two_book_catalog())], Knobs::default()).await;
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(book_names(&body), vec![("mat".into(), "Title-xx_mat_text_reg".into()), ("luk".into(), "Title-xx_luk_text_reg".into())]);
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), 3, "two failures, then the success");
}

#[tokio::test]
async fn data_api_down_gives_a_typed_503_then_the_next_request_heals_and_nothing_was_cached() {
    let env = start(vec![Step::Status(500)], Knobs::default()).await;
    let (status, body, headers) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 503);
    assert_eq!(body["kind"], "catalog_unavailable");
    assert_eq!(body["retryable"], true);
    assert!(headers.contains_key("retry-after"));
    let after_failure = env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(after_failure, 4, "bounded: exactly max_attempts");
    // the data API recovers: the very next request succeeds (a cached failure would still answer 503 or [])
    env.api.set_script(vec![Step::Ok(two_book_catalog())]);
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert_eq!(book_names(&body).len(), 2);
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), after_failure + 1, "one new call; the failure was not cached");
}

#[tokio::test]
async fn as_built_replicates_docs_cached_failure() {
    let env = start(vec![Step::Status(500)], Knobs::default()).await;
    let (status, body, _) = env.get(BOOKS, Mode::AsBuilt).await;
    assert_eq!((status, body), (200, json!([])), "DOC answers [] for a data-API failure");
    env.api.set_script(vec![Step::Ok(two_book_catalog())]);
    let (_, body, _) = env.get(BOOKS, Mode::AsBuilt).await;
    assert_eq!(body, json!([]), "...and remembers it for the TTL (180 s), the defect the fixed mode removes");
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200, "the fixed mode asks again and succeeds");
    assert_eq!(book_names(&body).len(), 2);
}

#[tokio::test]
async fn slow_data_api_times_out_the_attempt_and_the_retry_succeeds() {
    let env = start(vec![Step::Hang, Step::Ok(two_book_catalog())], Knobs::default()).await;
    let started = Instant::now();
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200, "{body}");
    assert!(started.elapsed() >= Duration::from_millis(400), "the first attempt waited for its timeout");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn malformed_graphql_is_retried_then_typed_and_heals() {
    let env = start(vec![Step::Garbage("<html>502 Bad Gateway</html>"), Step::Ok(r#"{"errors":[{"message":"boom"}]}"#.into()), Step::Ok(two_book_catalog())], Knobs::default()).await;
    let (status, _, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200, "two malformed answers are retried away");
    // a permanently malformed answer ends as a typed 503, never as an empty list
    let env = start(vec![Step::Ok(r#"{"data":{"git_repo":"nope"}}"#.into())], Knobs::default()).await;
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 503);
    assert_eq!(body["kind"], "catalog_malformed");
    env.api.set_script(vec![Step::Ok(two_book_catalog())]);
    assert_eq!(env.get(BOOKS, Mode::Fixed).await.0, 200, "the next request heals");
}

#[tokio::test]
async fn an_empty_primary_catalog_is_not_believed_or_cached() {
    let env = start(vec![Step::Ok(catalog_body(&[])), Step::Ok(two_book_catalog())], Knobs::default()).await;
    let (status, body, _) = env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert_eq!(book_names(&body).len(), 2);
}

#[tokio::test]
async fn stale_catalog_is_served_when_a_refresh_fails() {
    let env = start(vec![Step::Ok(two_book_catalog())], Knobs { catalog: Box::new(|c| c.ttl = Duration::from_millis(50)), ..Knobs::default() }).await;
    assert_eq!(env.get(BOOKS, Mode::Fixed).await.0, 200);
    tokio::time::sleep(Duration::from_millis(120)).await;
    env.api.set_script(vec![Step::Status(500)]);
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200, "stale-if-error: the resource stays available");
    assert_eq!(book_names(&body["data"]).len(), 2);
    assert!(findings_codes(&body).contains(&"catalog_served_stale".to_string()), "{body}");
}

#[tokio::test]
async fn concurrent_requests_share_one_data_api_refresh() {
    let env = start(vec![Step::Delay(Duration::from_millis(250), two_book_catalog())], Knobs::default()).await;
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let (c, u) = (env.client.clone(), format!("{}{}", env.base, "/chapters_in_books"));
        let u2 = format!("{}{}", env.base, BOOKS);
        tasks.push(tokio::spawn(async move {
            let _ = c.get(u).send().await;
            c.get(u2).header("x-doc-mode", "fixed").send().await.unwrap().status().as_u16()
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200);
    }
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), 1, "20 concurrent lookups, one upstream call");
}

#[tokio::test]
async fn success_is_cached_for_the_ttl_only() {
    let env = start(vec![Step::Ok(two_book_catalog())], Knobs { catalog: Box::new(|c| c.ttl = Duration::from_millis(300)), ..Knobs::default() }).await;
    env.get(BOOKS, Mode::Fixed).await;
    env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), 1, "second request is a cache hit");
    tokio::time::sleep(Duration::from_millis(400)).await;
    env.get(BOOKS, Mode::Fixed).await;
    assert_eq!(env.api.primary_calls.load(std::sync::atomic::Ordering::SeqCst), 2, "refreshed after the TTL");
}

// ----------------------------------------------------------------------------- git

#[tokio::test]
async fn clone_fails_twice_then_succeeds() {
    let env = start(vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))], Knobs::default()).await;
    env.git.flaky(2);
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert_eq!(book_names(&body["data"]), vec![("mat".into(), "Title-xx_mat_text_reg".into())]);
    assert_eq!(env.git.calls_for("xx_mat"), 3, "two failed attempts and the success");
    assert!(!findings_codes(&body).contains(&"repo_unavailable".to_string()));
}

#[tokio::test]
async fn clone_fails_for_good_is_reported_and_the_other_repository_still_answers() {
    let env = start(vec![Step::Ok(two_book_catalog())], Knobs::default()).await;
    env.git.mode("ok");
    // luk succeeds, mat is "not found": make the fake fail only for mat by switching mode per call
    env.git.mode("notfound");
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200, "a repository that cannot be fetched is a finding, not a failed request");
    let codes = findings_codes(&body);
    assert_eq!(codes.iter().filter(|c| *c == "repo_unavailable").count(), 2, "{body}");
    assert_eq!(env.git.calls_for("xx_mat"), 1, "a repository that does not exist is not retried within the request");
    // the repositories come back: the next request fetches them (nothing was cached)
    env.git.mode("ok");
    let (_, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert!(!findings_codes(&body).contains(&"repo_unavailable".to_string()));
    assert_eq!(book_names(&body["data"]).len(), 2);
}

#[tokio::test]
async fn hung_clone_is_killed_with_its_process_group_and_reported() {
    let env = start(
        vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))],
        Knobs { git: Box::new(|g| { g.clone_timeout = Duration::from_millis(400); g.max_attempts = 2; }), ..Knobs::default() },
    )
    .await;
    env.git.mode("hang");
    let started = Instant::now();
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_secs(5), "bounded by the clone timeout, not by the hang");
    assert!(findings_codes(&body).contains(&"repo_unavailable".to_string()), "{body}");
    assert!(body["findings"][0]["detail"].as_str().unwrap_or("").contains("Timeout"), "{body}");
    let pid = std::fs::read_to_string(env.git.state.join("hang.pid")).unwrap().trim().to_string();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "the clone process is gone");
    let leftover = std::process::Command::new("pgrep").args(["-f", &format!("sleep 31.7{pid}")]).output().unwrap();
    assert!(leftover.stdout.is_empty(), "the grandchild went with the group");
}

#[tokio::test]
async fn partial_clone_is_never_visible_and_leaves_nothing_behind() {
    let env = start(vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))], Knobs::default()).await;
    env.git.mode("partial");
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert!(findings_codes(&body).contains(&"repo_unavailable".to_string()));
    let cache = env.dir.path().join("cache");
    let mut leftovers = Vec::new();
    for key_dir in std::fs::read_dir(&cache).unwrap().flatten() {
        for e in std::fs::read_dir(key_dir.path()).unwrap().flatten() {
            leftovers.push(e.file_name().to_string_lossy().to_string());
        }
    }
    assert!(leftovers.is_empty(), "no tmp dir, no version, no symlink: {leftovers:?}");
    env.git.mode("ok");
    let (_, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(book_names(&body["data"]).len(), 1, "the next request succeeds");
}

#[tokio::test]
async fn concurrent_requests_for_the_same_repository_clone_it_once() {
    let env = start(vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))], Knobs::default()).await;
    env.git.delay("0.4");
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let (c, u) = (env.client.clone(), format!("{}{}", env.base, BOOKS));
        tasks.push(tokio::spawn(async move { c.get(u).header("x-doc-mode", "fixed").send().await.unwrap().text().await.unwrap() }));
    }
    let mut bodies = Vec::new();
    for t in tasks {
        bodies.push(t.await.unwrap());
    }
    assert!(bodies.iter().all(|b| b == &bodies[0]), "every caller got the same answer");
    assert_eq!(env.git.calls_for("xx_mat"), 1, "single-flight: one clone for twelve requests");
}

#[tokio::test]
async fn stale_copy_is_served_when_the_refresh_fails() {
    let env = start(
        vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))],
        Knobs { git: Box::new(|g| g.stale_after = Duration::from_millis(1)), ..Knobs::default() },
    )
    .await;
    assert_eq!(env.get(BOOKS, Mode::Fixed).await.0, 200);
    tokio::time::sleep(Duration::from_millis(30)).await;
    env.git.mode("fail");
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert_eq!(book_names(&body["data"]), vec![("mat".into(), "Title-xx_mat_text_reg".into())], "the old copy answers");
    assert!(findings_codes(&body).contains(&"repo_served_stale".to_string()), "{body}");
}

#[tokio::test]
async fn deadline_returns_but_the_clone_finishes_in_the_background() {
    let env = start(
        vec![Step::Ok(catalog_body(&[row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx")]))],
        Knobs { app: Box::new(|a| a.request_deadline = Duration::from_millis(300)), ..Knobs::default() },
    )
    .await;
    env.git.delay("1");
    let started = Instant::now();
    let (status, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_millis(900), "the request kept its deadline");
    assert!(findings_codes(&body).contains(&"repo_unavailable".to_string()), "{body}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (_, body, _) = env.get(BOOKS_F, Mode::Fixed).await;
    assert_eq!(book_names(&body["data"]).len(), 1, "the background clone filled the cache: the next request is complete");
    assert_eq!(env.git.calls_for("xx_mat"), 1, "and nothing was cloned twice");
}

#[tokio::test]
async fn as_built_reproduces_docs_empty_list_for_an_owner_prefixed_repository_and_fixed_does_not() {
    let env = start(
        vec![Step::Ok(catalog_body(&[
            row("https://content.example.org/o/xx_luk_text_reg", "reg", "xx"),
            row("https://content.example.org/DCS-Mirror/faustin-azaza_xx_mat_text_reg", "reg", "xx"),
        ]))],
        Knobs::default(),
    )
    .await;
    let (_, built, _) = env.get(BOOKS, Mode::AsBuilt).await;
    assert_eq!(built, json!([]), "DOC: KeyError swallowed into an empty list for the whole language");
    let (_, fixed, _) = env.get(BOOKS_F, Mode::Fixed).await;
    let names = book_names(&fixed["data"]);
    assert_eq!(names.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>(), vec!["mat", "luk"]);
    assert!(findings_codes(&fixed).contains(&"owner_prefix_stripped".to_string()));
}
