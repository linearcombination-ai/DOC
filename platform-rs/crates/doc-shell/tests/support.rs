#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Test fixtures: a scriptable mock data API, a fake `git` executable, and the shell served on an
//! ephemeral port with the production adapters pointed at them.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use doc_acquire::{GitAcquirer, GitConfig};
use doc_app::{App, AppConfig};
use doc_catalog::{CatalogConfig, HttpCatalog};
use doc_contract::Mode;
use doc_kitchen::KitchenCanon;
use doc_shell::Shared;

// ------------------------------------------------------------------ mock data API

#[derive(Clone, Debug)]
pub enum Step {
    Ok(String),
    Status(u16),
    Garbage(&'static str),
    /// Sleep, then answer with the body.
    Delay(Duration, String),
    /// Never answer (until the client gives up).
    Hang,
}

pub struct MockApi {
    pub addr: SocketAddr,
    script: Arc<Mutex<Vec<Step>>>,
    pub primary_calls: Arc<AtomicUsize>,
    pub non_primary_calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct ApiState {
    script: Arc<Mutex<Vec<Step>>>,
    primary: Arc<AtomicUsize>,
    non_primary: Arc<AtomicUsize>,
    non_primary_body: Arc<String>,
}

async fn handle(State(st): State<ApiState>, body: String) -> Response {
    if body.contains("_eq: \\\"bg\\\"") || body.contains("_eq: \"bg\"") {
        st.non_primary.fetch_add(1, Ordering::SeqCst);
        return (StatusCode::OK, [("content-type", "application/json")], (*st.non_primary_body).clone()).into_response();
    }
    st.primary.fetch_add(1, Ordering::SeqCst);
    let step = {
        let mut s = st.script.lock().unwrap();
        if s.len() > 1 { s.remove(0) } else { s[0].clone() }
    };
    match step {
        Step::Ok(b) => (StatusCode::OK, [("content-type", "application/json")], b).into_response(),
        Step::Status(c) => StatusCode::from_u16(c).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR).into_response(),
        Step::Garbage(g) => (StatusCode::OK, g).into_response(),
        Step::Delay(d, b) => {
            tokio::time::sleep(d).await;
            (StatusCode::OK, [("content-type", "application/json")], b).into_response()
        }
        Step::Hang => {
            tokio::time::sleep(Duration::from_secs(120)).await;
            StatusCode::OK.into_response()
        }
    }
}

impl MockApi {
    pub async fn start(script: Vec<Step>) -> MockApi {
        let st = ApiState {
            script: Arc::new(Mutex::new(script)),
            primary: Arc::new(AtomicUsize::new(0)),
            non_primary: Arc::new(AtomicUsize::new(0)),
            non_primary_body: Arc::new(non_primary_body()),
        };
        let router = Router::new().route("/v1/graphql", post(handle)).with_state(st.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        MockApi { addr, script: st.script, primary_calls: st.primary, non_primary_calls: st.non_primary }
    }
    pub fn url(&self) -> String {
        format!("http://{}/v1/graphql", self.addr)
    }
    pub fn set_script(&self, script: Vec<Step>) {
        *self.script.lock().unwrap() = script;
    }
}

pub fn row(url: &str, resource_type: &str, lang: &str) -> String {
    format!(
        r#"{{"repo_url":"{url}","content":{{"resource_type":"{resource_type}","language":{{"english_name":"Testish","ietf_code":"{lang}","national_name":"Testish","direction":"ltr"}}}}}}"#
    )
}

pub fn catalog_body(rows: &[String]) -> String {
    format!(r#"{{"data":{{"git_repo":[{}]}}}}"#, rows.join(","))
}

fn non_primary_body() -> String {
    catalog_body(&[row("https://content.example.org/WA-Catalog/bg_bpb", "bpb", "bg")])
}

/// Two per-book Writer repositories of language `xx`: Matthew and Luke.
pub fn two_book_catalog() -> String {
    catalog_body(&[
        row("https://content.example.org/o/xx_luk_text_reg", "reg", "xx"),
        row("https://content.example.org/o/xx_mat_text_reg", "reg", "xx"),
    ])
}

// ------------------------------------------------------------------ fake git

pub struct FakeGit {
    pub bin: PathBuf,
    pub state: PathBuf,
}

impl FakeGit {
    pub fn install(dir: &Path) -> FakeGit {
        let state = dir.join("fakegit-state");
        std::fs::create_dir_all(&state).unwrap();
        let bin = dir.join("fake-git.sh");
        let script = format!(
            r#"#!/usr/bin/env bash
state="{state}"
url="${{@: -2:1}}"; dest="${{@: -1}}"
echo "$(date +%s.%N) $url" >> "$state/calls.log"
mode=$(cat "$state/mode" 2>/dev/null || echo ok)
base=$(basename "$url")
[ -f "$state/delay" ] && sleep "$(cat "$state/delay")"
case "$mode" in
  fail) echo "fatal: unable to access '$url': Could not resolve host" >&2; exit 128;;
  notfound) echo "remote: Repository not found." >&2; exit 128;;
  hang) echo $$ > "$state/hang.pid"; exec sleep "31.7$$";;
  partial) mkdir -p "$dest/.git"; echo partial > "$dest/.git/config"; echo "fatal: early EOF" >&2; exit 128;;
  flaky) n=$(cat "$state/flaky" 2>/dev/null || echo 0)
         if [ "$n" -gt 0 ]; then echo $((n-1)) > "$state/flaky"; echo "fatal: transient failure" >&2; exit 128; fi;;
esac
mkdir -p "$dest/.git/objects" "$dest/front"
echo "[core]" > "$dest/.git/config"; echo "ref: refs/heads/master" > "$dest/.git/HEAD"
printf 'Title-%s\n' "$base" > "$dest/front/title.txt"
exit 0
"#,
            state = state.display()
        );
        std::fs::write(&bin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        FakeGit { bin, state }
    }
    pub fn mode(&self, m: &str) {
        std::fs::write(self.state.join("mode"), m).unwrap();
    }
    pub fn flaky(&self, failures: u32) {
        std::fs::write(self.state.join("flaky"), failures.to_string()).unwrap();
        self.mode("flaky");
    }
    pub fn delay(&self, secs: &str) {
        std::fs::write(self.state.join("delay"), secs).unwrap();
    }
    /// URLs of every clone invocation so far.
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.state.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(' ').map(|(_, u)| u.to_string()))
            .collect()
    }
    pub fn calls_for(&self, needle: &str) -> usize {
        self.calls().iter().filter(|u| u.contains(needle)).count()
    }
}

// ------------------------------------------------------------------ the shell under test

pub struct Env {
    pub base: String,
    pub api: MockApi,
    pub git: FakeGit,
    pub dir: tempfile_dir::TempDir,
    pub client: reqwest::Client,
}

pub struct Knobs {
    pub catalog: Box<dyn Fn(&mut CatalogConfig)>,
    pub git: Box<dyn Fn(&mut GitConfig)>,
    pub app: Box<dyn Fn(&mut AppConfig)>,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs { catalog: Box::new(|_| {}), git: Box::new(|_| {}), app: Box::new(|_| {}) }
    }
}

pub async fn start(script: Vec<Step>, knobs: Knobs) -> Env {
    let dir = tempfile_dir::TempDir::new();
    let api = MockApi::start(script).await;
    let git = FakeGit::install(dir.path());
    let mut cc = CatalogConfig::new(&api.url());
    cc.connect_timeout = Duration::from_millis(500);
    cc.attempt_timeout = Duration::from_millis(400);
    cc.backoff_base = Duration::from_millis(10);
    cc.backoff_cap = Duration::from_millis(60);
    cc.max_attempts = 4;
    cc.refresh_ceiling = Duration::from_secs(5);
    (knobs.catalog)(&mut cc);
    let mut gc = GitConfig::new(dir.path().join("cache"));
    gc.git_bin = git.bin.clone();
    gc.backoff_base = Duration::from_millis(10);
    gc.backoff_cap = Duration::from_millis(60);
    gc.max_attempts = 3;
    gc.clone_timeout = Duration::from_secs(5);
    gc.gc_grace = Duration::from_secs(3600);
    (knobs.git)(&mut gc);
    let mut ac = AppConfig { request_deadline: Duration::from_secs(20), as_built_failure_memory: Duration::from_secs(180), ..AppConfig::default() };
    (knobs.app)(&mut ac);
    let default_mode = ac.default_mode;
    let shared = Arc::new(Shared {
        app: Arc::new(App::new(HttpCatalog::new(cc).unwrap(), GitAcquirer::new(gc), Arc::new(KitchenCanon), ac)),
        default_mode,
    });
    let (router, _api) = doc_shell::build(shared, &[]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Env { base: format!("http://{addr}"), api, git, dir, client: reqwest::Client::new() }
}

impl Env {
    pub async fn get(&self, path: &str, mode: Mode) -> (u16, serde_json::Value, reqwest::header::HeaderMap) {
        let resp = self.client.get(format!("{}{}", self.base, path)).header("x-doc-mode", mode.as_str()).send().await.unwrap();
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let text = resp.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)), headers)
    }
}

/// A minimal temp dir (no extra dependency): removed on drop.
pub mod tempfile_dir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir(PathBuf);
    impl TempDir {
        pub fn new() -> TempDir {
            let base = std::env::var("DOC_P4_TEST_TMP").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
            let p = base.join(format!("doc-p4-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
