#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]
//! Fixtures for the P5 suites: a real Redis server (built from source on this host), a scriptable
//! catalog, a fake `git` that copies fixture content, and `doc-jobd` as a real subprocess.

use std::net::TcpListener as StdListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};

pub fn p5_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

pub fn free_port() -> u16 {
    StdListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A scratch directory under the worktree (never /tmp), removed on drop.
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = p5_root().join("scratch/tests").join(format!("{tag}-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var("DOC_P5_KEEP").is_err() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

// ------------------------------------------------------------------ Redis

pub struct Redis {
    pub port: u16,
    pub dir: PathBuf,
    pub child: Option<Child>,
}

impl Redis {
    pub fn bin() -> PathBuf {
        std::env::var("DOC_SERVER_BIN").map(PathBuf::from).unwrap_or_else(|_| p5_root().join(".p6env/redis/bin/redis-server"))
    }
    pub async fn start(dir: &Path, persistent: bool) -> Redis {
        let port = free_port();
        let mut r = Redis { port, dir: dir.to_path_buf(), child: None };
        std::fs::create_dir_all(dir).unwrap();
        r.spawn(persistent).await;
        r
    }
    async fn spawn(&mut self, persistent: bool) {
        let mut c = Command::new(Self::bin());
        c.args(["--port", &self.port.to_string(), "--bind", "127.0.0.1", "--dir"]).arg(&self.dir).args(["--save", "", "--daemonize", "no"]);
        if persistent {
            c.args(["--appendonly", "yes", "--appendfsync", "everysec"]);
        }
        c.stdout(Stdio::from(std::fs::File::create(self.dir.join("redis.log")).unwrap())).stderr(Stdio::null());
        self.child = Some(c.spawn().expect("redis-server"));
        self.wait_ready().await;
    }
    pub fn url(&self) -> String {
        format!("redis://127.0.0.1:{}/0", self.port)
    }
    pub async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(client) = redis::Client::open(self.url()) {
                if let Ok(mut c) = client.get_multiplexed_async_connection().await {
                    let r: Result<String, _> = redis::cmd("PING").query_async(&mut c).await;
                    if r.is_ok() {
                        return;
                    }
                }
            }
            assert!(Instant::now() < deadline, "redis did not come up");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub fn pid(&self) -> u32 {
        self.child.as_ref().map(Child::id).unwrap_or(0)
    }
    /// SIGKILL: no shutdown handler, the AOF as the last fsync left it.
    pub fn kill9(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    /// SHUTDOWN (flushes the AOF), then wait for the process.
    pub async fn shutdown(&mut self) {
        if let Ok(client) = redis::Client::open(self.url()) {
            if let Ok(mut c) = client.get_multiplexed_async_connection().await {
                let _: Result<(), _> = redis::cmd("SHUTDOWN").arg("NOSAVE").query_async(&mut c).await;
            }
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.wait();
        }
    }
    pub async fn restart(&mut self, persistent: bool) {
        self.spawn(persistent).await;
    }
    pub async fn conn(&self) -> redis::aio::MultiplexedConnection {
        redis::Client::open(self.url()).unwrap().get_multiplexed_async_connection().await.unwrap()
    }
}
impl Drop for Redis {
    fn drop(&mut self) {
        self.kill9();
    }
}

// ------------------------------------------------------------------ catalog

pub fn catalog_row(url: &str, resource_type: &str, lang: &str, english: &str) -> String {
    format!(
        r#"{{"repo_url":"{url}","content":{{"resource_type":"{resource_type}","language":{{"english_name":"{english}","ietf_code":"{lang}","national_name":"{english}","direction":"ltr"}}}}}}"#
    )
}

pub fn english_catalog() -> String {
    let rows = [
        catalog_row("https://content.example.org/WycliffeAssociates/en_ulb", "ulb", "en", "English"),
        catalog_row("https://content.example.org/WycliffeAssociates/en_tn", "tn", "en", "English"),
    ];
    format!(r#"{{"data":{{"git_repo":[{}]}}}}"#, rows.join(","))
}

pub struct Catalog {
    pub port: u16,
    pub state: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

/// A catalog whose answers are scripted: each call pops one entry; the last one repeats.
/// Entries: a JSON body, `status:503`, `hang`.
impl Catalog {
    pub async fn start(script: Vec<String>) -> Catalog {
        let state = std::sync::Arc::new(std::sync::Mutex::new(script));
        let st = state.clone();
        let handler = move |_body: String| {
            let st = st.clone();
            async move {
                let step = {
                    let mut s = st.lock().unwrap();
                    if s.len() > 1 { s.remove(0) } else { s[0].clone() }
                };
                match step.as_str() {
                    "hang" => {
                        tokio::time::sleep(Duration::from_secs(300)).await;
                        (axum::http::StatusCode::OK, String::new())
                    }
                    s if s.starts_with("status:") => (axum::http::StatusCode::from_u16(s[7..].parse().unwrap()).unwrap(), String::new()),
                    body => (axum::http::StatusCode::OK, body.to_string()),
                }
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = Router::new().route("/v1/graphql", post(handler));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Catalog { port, state }
    }
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/v1/graphql", self.port)
    }
    pub fn set(&self, script: Vec<String>) {
        *self.state.lock().unwrap() = script;
    }
}

// ------------------------------------------------------------------ fake git

pub struct FakeGit {
    pub bin: PathBuf,
    pub state: PathBuf,
}

impl FakeGit {
    /// The fake copies `fixtures/content/<last URL segment>` into the destination and marks it a clone.
    pub fn install(dir: &Path) -> FakeGit {
        let state = dir.join("fakegit-state");
        std::fs::create_dir_all(&state).unwrap();
        let bin = dir.join("fake-git.sh");
        let script = format!(
            r#"#!/usr/bin/env bash
state="{state}"; content="{content}"
url="${{@: -2:1}}"; dest="${{@: -1}}"
echo "$(date +%s.%N) $url" >> "$state/calls.log"
base=$(basename "$url")
[ -f "$state/delay" ] && sleep "$(cat "$state/delay")"
mode=$(cat "$state/mode" 2>/dev/null || echo ok)
case "$mode" in
  fail) echo "fatal: unable to access '$url': Could not resolve host" >&2; exit 128;;
  hang) echo $$ > "$state/hang.pid"; exec sleep 3600;;
  flaky) n=$(cat "$state/flaky" 2>/dev/null || echo 0)
         if [ "$n" -gt 0 ]; then echo $((n-1)) > "$state/flaky"; echo "fatal: transient failure" >&2; exit 128; fi;;
esac
mkdir -p "$dest/.git/objects"; cp -r "$content/$base/." "$dest/"
echo "[core]" > "$dest/.git/config"; echo "ref: refs/heads/master" > "$dest/.git/HEAD"
exit 0
"#,
            state = state.display(),
            content = p5_root().join("fixtures/content").display()
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
    pub fn calls(&self) -> usize {
        std::fs::read_to_string(self.state.join("calls.log")).map(|s| s.lines().count()).unwrap_or(0)
    }
}

// ------------------------------------------------------------------ the daemon

pub struct Daemon {
    pub child: Child,
    pub port: u16,
    pub log: PathBuf,
}

pub struct Setup {
    pub dir: PathBuf,
    pub redis_url: String,
    pub catalog_url: String,
    pub git_bin: PathBuf,
    pub output: PathBuf,
    pub cache: PathBuf,
}

impl Setup {
    pub fn env(&self, port: u16, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = vec![
            ("DATA_API_URL".into(), self.catalog_url.clone()),
            ("DOC_SHELL_ADDR".into(), format!("127.0.0.1:{port}")),
            ("DOC_SHELL_GIT_BIN".into(), self.git_bin.display().to_string()),
            ("DOC_SHELL_CACHE_DIR".into(), self.cache.display().to_string()),
            ("DOC_SHELL_MODE".into(), "fixed".into()),
            ("DOC_SHELL_CLONE_ATTEMPTS".into(), "2".into()),
            ("DOC_SHELL_CLONE_TIMEOUT_SECS".into(), "10".into()),
            ("DOC_SHELL_CATALOG_ATTEMPTS".into(), "2".into()),
            ("DOC_SHELL_CATALOG_TIMEOUT_SECS".into(), "3".into()),
            ("DOC_JOBD_STORE".into(), "redis".into()),
            ("DOC_JOBD_REDIS_URL".into(), self.redis_url.clone()),
            ("DOC_JOBD_OUTPUT_DIR".into(), self.output.display().to_string()),
            ("DOC_JOBD_DOCX_WORKER".into(), p5_root().join("python/docx_worker.py").display().to_string()),
            ("DOC_JOBD_LEASE_TTL_MS".into(), "2000".into()),
            ("DOC_JOBD_REAP_EVERY_MS".into(), "300".into()),
            ("DOC_JOBD_PROMOTE_EVERY_MS".into(), "200".into()),
            ("DOC_JOBD_CLAIM_BLOCK_MS".into(), "300".into()),
            ("DOC_JOBD_BACKOFF_BASE_MS".into(), "300".into()),
            ("DOC_JOBD_BACKOFF_CAP_MS".into(), "1500".into()),
            ("DOC_JOBD_BACKOFF_FLOOR_MS".into(), "100".into()),
            ("DOC_JOBD_MAX_ATTEMPTS".into(), "5".into()),
            ("DOC_JOBD_DRAIN_DEADLINE_MS".into(), "20000".into()),
            ("DOC_JOBD_NAMESPACE".into(), "t".into()),
            ("RUST_LOG".into(), "info".into()),
            ("DOC_SHELL_LOG_FORMAT".into(), "json".into()),
        ];
        for (k, val) in extra {
            v.retain(|(kk, _)| kk != k);
            v.push(((*k).to_string(), (*val).to_string()));
        }
        v
    }
}

impl Daemon {
    pub async fn spawn(setup: &Setup, name: &str, extra: &[(&str, &str)]) -> Daemon {
        let port = free_port();
        let log = setup.dir.join(format!("{name}.log"));
        let f = std::fs::File::create(&log).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_doc-jobd"));
        cmd.env_clear().envs(setup.env(port, extra)).env("PATH", std::env::var("PATH").unwrap_or_default());
        cmd.stdout(Stdio::from(f.try_clone().unwrap())).stderr(Stdio::from(f));
        let child = cmd.spawn().expect("doc-jobd");
        let d = Daemon { child, port, log };
        d.wait_listening().await;
        d
    }
    pub async fn wait_listening(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let c = reqwest::Client::new();
        loop {
            if c.get(format!("http://127.0.0.1:{}/health/status", self.port)).send().await.is_ok() {
                return;
            }
            assert!(Instant::now() < deadline, "doc-jobd did not listen; log:\n{}", std::fs::read_to_string(&self.log).unwrap_or_default());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn pid(&self) -> i32 {
        i32::try_from(self.child.id()).unwrap()
    }
    pub fn sigkill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn sigterm(&self) {
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(self.pid()), nix::sys::signal::Signal::SIGTERM);
    }
    pub async fn wait_exit(&mut self, within: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + within;
        loop {
            if let Ok(Some(s)) = self.child.try_wait() {
                return Some(s);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ------------------------------------------------------------------ requests and polling

/// The e2e request: English ULB with TN for Jude, interleaved by book, as the UI sends it for a docx.
pub fn jude_request(extra: Value) -> Value {
    let mut r = json!({
        "assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": true, "generate_pdf": false,
        "resource_requests": [
            {"lang_code": "en", "resource_type": "ulb", "book_code": "jud"},
            {"lang_code": "en", "resource_type": "tn", "book_code": "jud"}
        ]
    });
    if let (Some(o), Some(e)) = (r.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    r
}

pub async fn post_docx(c: &reqwest::Client, base: &str, body: &Value) -> (u16, Value, reqwest::header::HeaderMap) {
    let resp = c.post(format!("{base}/documents_docx")).json(body).send().await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let text = resp.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::String(text)), headers)
}

pub async fn status_of(c: &reqwest::Client, base: &str, id: &str) -> (u16, Value) {
    let resp = c.get(format!("{base}/task_status/{id}")).send().await.unwrap();
    let st = resp.status().as_u16();
    (st, resp.json().await.unwrap_or(Value::Null))
}

/// Poll quickly until SUCCESS or FAILURE, returning every distinct state seen, in order, and the last body.
pub async fn watch(c: &reqwest::Client, base: &str, id: &str, within: Duration) -> (Vec<String>, Value) {
    let deadline = Instant::now() + within;
    let mut seen: Vec<String> = Vec::new();
    loop {
        let (st, body) = status_of(c, base, id).await;
        if st == 200 {
            if let Some(s) = body["state"].as_str() {
                if seen.last().map(String::as_str) != Some(s) {
                    seen.push(s.to_string());
                }
                if s == "SUCCESS" || s == "FAILURE" {
                    return (seen, body);
                }
            }
        }
        assert!(Instant::now() < deadline, "job {id} did not finish in {within:?}; states so far {seen:?}");
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

pub fn check_docx(path: &Path) -> Value {
    let out = Command::new("python3").arg("-I").arg(p5_root().join("scripts/check_docx.py")).arg(path).output().expect("python3");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| json!({"ok": false, "error": String::from_utf8_lossy(&out.stdout).to_string()}))
}

/// The daemon's JSON log lines: (message, fields).
pub fn log_events(log: &str) -> Vec<(String, Value)> {
    log.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|v| {
            let f = v.get("fields").cloned().unwrap_or_else(|| v.clone());
            (f["message"].as_str().unwrap_or("").to_string(), f)
        })
        .collect()
}

pub fn pid_alive(pid: i32) -> bool {
    // /proc/<pid>/stat state Z (zombie) counts as gone for the process, but our seam reaps, so a
    // missing /proc entry is the strict check; report both.
    Path::new(&format!("/proc/{pid}")).exists()
}

pub fn setup(dir: &Path, redis: &Redis, catalog: &Catalog, git: &FakeGit) -> Setup {
    Setup { dir: dir.to_path_buf(), redis_url: redis.url(), catalog_url: catalog.url(), git_bin: git.bin.clone(), output: dir.join("out"), cache: dir.join("cache") }
}

// ------------------------------------------------------------------ P6 additions

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

impl Redis {
    /// A server with extra command-line arguments (requirepass, TLS) and its own URL.
    pub async fn start_with(dir: &Path, port: u16, extra: &[&str]) -> Redis {
        std::fs::create_dir_all(dir).unwrap();
        let mut r = Redis { port, dir: dir.to_path_buf(), child: None };
        let mut c = Command::new(Self::bin());
        c.args(extra).arg("--dir").arg(dir).args(["--save", "", "--daemonize", "no", "--bind", "127.0.0.1"]);
        c.stdout(Stdio::from(std::fs::File::create(dir.join("server.log")).unwrap())).stderr(Stdio::null());
        r.child = Some(c.spawn().expect("server"));
        // wait for the TCP port (TLS or not) to accept
        let deadline = Instant::now() + Duration::from_secs(20);
        while tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            assert!(Instant::now() < deadline, "server did not come up");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        r
    }
}

/// A throwaway CA and a server certificate for localhost (openssl CLI).
pub fn make_certs(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let run = |args: &[&str]| {
        let o = Command::new("openssl").args(args).current_dir(dir).output().expect("openssl");
        assert!(o.status.success(), "openssl {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    run(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.crt", "-days", "2", "-subj", "/CN=p6-test-ca"]);
    run(&["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "srv.key", "-out", "srv.csr", "-subj", "/CN=localhost"]);
    std::fs::write(dir.join("ext.cnf"), "subjectAltName=DNS:localhost,IP:127.0.0.1\n").unwrap();
    run(&["x509", "-req", "-in", "srv.csr", "-CA", "ca.crt", "-CAkey", "ca.key", "-CAcreateserial", "-out", "srv.crt", "-days", "2", "-extfile", "ext.cnf"]);
    // a second, unrelated CA: "the wrong CA"
    run(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "other.key", "-out", "other.crt", "-days", "2", "-subj", "/CN=someone-else"]);
}

#[derive(Default, Clone)]
pub struct SmtpKnobs {
    /// Answer this many RCPT commands with 450 (a transient refusal), then accept.
    pub rcpt_refusals: u32,
    /// Read the whole message, store it, and close the connection without the final 250.
    pub drop_after_data: bool,
    pub connections: u32,
}

/// A fake SMTP server: records every message it was handed (accepted or not acknowledged), speaks just enough
/// SMTP, and can be told to misbehave.
pub struct FakeSmtp {
    pub port: u16,
    pub received: Arc<Mutex<Vec<String>>>,
    pub knobs: Arc<Mutex<SmtpKnobs>>,
}

impl FakeSmtp {
    pub async fn start() -> FakeSmtp {
        let received = Arc::new(Mutex::new(Vec::new()));
        let knobs = Arc::new(Mutex::new(SmtpKnobs::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (rc, kn) = (received.clone(), knobs.clone());
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                let (rc, kn) = (rc.clone(), kn.clone());
                tokio::spawn(async move {
                    kn.lock().unwrap().connections += 1;
                    let (r, mut w) = sock.into_split();
                    let mut r = BufReader::new(r);
                    let _ = w.write_all(b"220 fake ESMTP\r\n").await;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if r.read_line(&mut line).await.unwrap_or(0) == 0 {
                            return;
                        }
                        let up = line.trim_end().to_ascii_uppercase();
                        if up.starts_with("EHLO") {
                            let _ = w.write_all(b"250-fake\r\n250 8BITMIME\r\n").await;
                        } else if up.starts_with("MAIL FROM") {
                            let _ = w.write_all(b"250 ok\r\n").await;
                        } else if up.starts_with("RCPT TO") {
                            let refuse = {
                                let mut k = kn.lock().unwrap();
                                if k.rcpt_refusals > 0 {
                                    k.rcpt_refusals -= 1;
                                    true
                                } else {
                                    false
                                }
                            };
                            let _ = w.write_all(if refuse { b"450 mailbox busy, try later\r\n" as &[u8] } else { b"250 ok\r\n" }).await;
                        } else if up == "DATA" {
                            let _ = w.write_all(b"354 go ahead\r\n").await;
                            let mut data = String::new();
                            loop {
                                let mut l = String::new();
                                if r.read_line(&mut l).await.unwrap_or(0) == 0 {
                                    return;
                                }
                                if l == ".\r\n" {
                                    break;
                                }
                                data.push_str(&l);
                            }
                            rc.lock().unwrap().push(data);
                            if kn.lock().unwrap().drop_after_data {
                                return; // no 250: the client cannot know
                            }
                            let _ = w.write_all(b"250 queued\r\n").await;
                        } else if up == "QUIT" {
                            let _ = w.write_all(b"221 bye\r\n").await;
                            return;
                        } else if up == "RSET" || up == "NOOP" {
                            let _ = w.write_all(b"250 ok\r\n").await;
                        } else {
                            let _ = w.write_all(b"502 not implemented\r\n").await;
                        }
                    }
                });
            }
        });
        FakeSmtp { port, received, knobs }
    }
    pub fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }
}

pub async fn get_text(c: &reqwest::Client, url: &str) -> (u16, String) {
    let r = c.get(url).send().await.unwrap();
    (r.status().as_u16(), r.text().await.unwrap_or_default())
}

pub async fn read_all(mut s: tokio::net::TcpStream) -> String {
    let mut b = Vec::new();
    let _ = s.read_to_end(&mut b).await;
    String::from_utf8_lossy(&b).to_string()
}

/// English and Spanish repositories for the request-shape table (trimmed fixtures under `fixtures/content`).
pub fn full_catalog() -> String {
    let base = "https://content.example.org/WycliffeAssociates";
    let rows = [
        catalog_row(&format!("{base}/en_ulb"), "ulb", "en", "English"),
        catalog_row(&format!("{base}/en_tn"), "tn", "en", "English"),
        catalog_row(&format!("{base}/en_tn_condensed"), "tn-condensed", "en", "English"),
        catalog_row(&format!("{base}/en_tw"), "tw", "en", "English"),
        catalog_row(&format!("{base}/en_bc"), "bc", "en", "English"),
        catalog_row(&format!("{base}/es-419_ulb"), "ulb", "es-419", "Spanish"),
        catalog_row(&format!("{base}/es-419_tn"), "tn", "es-419", "Spanish"),
        catalog_row(&format!("{base}/es-419_tq"), "tq", "es-419", "Spanish"),
    ];
    format!(r#"{{"data":{{"git_repo":[{}]}}}}"#, rows.join(","))
}
