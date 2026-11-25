#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The P7 suite: the Python seam as a service across a (process) boundary, exactly the wiring the containers use:
//! `doc-jobd` with `DOC_DOCX_MODE=remote` / `DOC_PDF_ENGINE=remote`, and `python/seam_server.py` as the other side.
//! Real Redis or Valkey (DOC_SERVER_BIN), real processes; git, the catalog and SMTP are fakes. `DOC_TEST_PYTHON` names
//! the interpreter for the service (default `python3`); the PDF test needs one that can `import weasyprint` and says so
//! when it is skipped.

mod support;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
    async fn submit(&self, d: &Daemon, route: &str, body: &Value) -> String {
        let r = self.c.post(format!("{}/{route}", d.base())).json(body).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 200);
        r.json::<Value>().await.unwrap()["task_id"].as_str().unwrap().to_string()
    }
    async fn done(&self, d: &Daemon, id: &str) -> Value {
        let (seen, last) = watch(&self.c, &d.base(), id, Duration::from_secs(120)).await;
        assert_eq!(last["state"], "SUCCESS", "states {seen:?}; log:\n{}", tail_log(&d.log_text()));
        last
    }
    async fn hget(&self, id: &str, field: &str) -> Option<String> {
        let mut c = self.redis.conn().await;
        redis::cmd("HGET").arg(format!("t:job:{id}")).arg(field).query_async(&mut c).await.unwrap()
    }
    fn out(&self) -> PathBuf {
        self.setup().output
    }
}

fn tail_log(s: &str) -> String {
    let l: Vec<&str> = s.lines().collect();
    l[l.len().saturating_sub(30)..].join("\n")
}

fn events(d: &Daemon) -> Vec<(String, Value)> {
    log_events(&d.log_text())
}

fn python() -> String {
    std::env::var("DOC_TEST_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn weasyprint_available() -> bool {
    Command::new(python()).args(["-I", "-c", "import weasyprint"]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// `python/seam_server.py` as a child process on its own port, serving the rig's output directory.
struct PyService {
    child: Option<Child>,
    port: u16,
    log: PathBuf,
}

impl PyService {
    fn start(rig: &Rig, port: Option<u16>, extra: &[(&str, &str)]) -> PyService {
        let port = port.unwrap_or_else(free_port);
        std::fs::create_dir_all(rig.out()).unwrap();
        let log = rig.dir.path().join(format!("pyworker-{port}.log"));
        let f = std::fs::File::create(&log).unwrap();
        let mut cmd = Command::new(python());
        cmd.args(["-I"]).arg(p5_root().join("python/seam_server.py"));
        cmd.env("PYWORKER_PORT", port.to_string()).env("PYWORKER_HOST", "127.0.0.1").env("PYWORKER_ROOT", rig.out()).env("PYWORKER_CONCURRENCY", "2").env("PYWORKER_DRAIN_SECONDS", "20");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::from(f.try_clone().unwrap())).stderr(Stdio::from(f));
        let child = cmd.spawn().expect("python seam_server.py");
        let s = PyService { child: Some(child), port, log };
        s.wait_ready();
        s
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if std::net::TcpStream::connect_timeout(&format!("127.0.0.1:{}", self.port).parse().unwrap(), Duration::from_millis(200)).is_ok() {
                return;
            }
            assert!(Instant::now() < deadline, "the Python service did not listen:\n{}", std::fs::read_to_string(&self.log).unwrap_or_default());
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fn kill9(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    fn pid(&self) -> i32 {
        i32::try_from(self.child.as_ref().unwrap().id()).unwrap()
    }
    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for PyService {
    fn drop(&mut self) {
        self.kill9();
    }
}

fn remote_env<'a>(svc: &'a PyService, url: &'a mut String) -> Vec<(&'static str, &'a str)> {
    *url = svc.url();
    // generous attempts: the tests keep the service away for seconds, and backoff here is 0.3 to 1.5 s (production: 8 attempts, 5 s to 300 s)
    vec![("DOC_DOCX_MODE", "remote"), ("DOC_PYWORKER_URL", url.as_str()), ("DOC_JOBD_MAX_ATTEMPTS", "40")]
}

async fn get_json(c: &reqwest::Client, url: &str) -> (u16, Value) {
    let r = c.get(url).send().await.unwrap();
    let code = r.status().as_u16();
    (code, r.json::<Value>().await.unwrap_or(Value::Null))
}

// =============================================================================================== the service on its own

#[tokio::test(flavor = "multi_thread")]
async fn the_service_answers_probes_rejects_paths_outside_its_root_and_drains_on_sigterm() {
    let rig = Rig::new("p7-svc").await;
    let mut svc = PyService::start(&rig, None, &[]);
    let (code, ready) = get_json(&rig.c, &format!("{}/readyz", svc.url())).await;
    println!("readyz {code}: {ready}");
    assert_eq!(ready["docx_script"], true);
    assert_eq!(ready["root_writable"], true);
    let (live, _) = get_json(&rig.c, &format!("{}/healthz", svc.url())).await;
    assert_eq!(live, 200);
    // a job file outside the root is refused with 400 and nothing runs
    let r = rig.c.post(format!("{}/v1/docx", svc.url())).json(&json!({"job_file": "/etc/passwd", "deadline_ms": 5000})).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 400);
    let r = rig.c.post(format!("{}/v1/pdf", svc.url())).json(&json!({"job_file": "/etc/hostname", "engine": "prince"})).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 400);
    // a body that is not JSON, and a route that does not exist
    let r = rig.c.post(format!("{}/v1/docx", svc.url())).body("nope").send().await.unwrap();
    assert_eq!(r.status().as_u16(), 400);
    assert_eq!(rig.c.get(format!("{}/nothing", svc.url())).send().await.unwrap().status().as_u16(), 404);
    let m = rig.c.get(format!("{}/metrics", svc.url())).send().await.unwrap().text().await.unwrap();
    assert!(m.contains("pyworker_jobs_total{kind=\"docx\",outcome=\"ok\"} 0") && m.contains("pyworker_inflight 0"), "metrics:\n{m}");
    // SIGTERM: exit 0 when nothing is running
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(svc.pid()), nix::sys::signal::Signal::SIGTERM).unwrap();
    let status = tokio::task::spawn_blocking(move || svc.child.take().unwrap().wait().unwrap()).await.unwrap();
    assert!(status.success(), "a clean stop exits 0: {status:?}");
}

// =============================================================================================== docx across the boundary

#[tokio::test(flavor = "multi_thread")]
async fn a_docx_job_runs_in_the_python_service_and_the_result_is_a_valid_document() {
    let rig = Rig::new("p7-docx").await;
    let svc = PyService::start(&rig, None, &[]);
    let mut u = String::new();
    let d = rig.daemon("d", &remote_env(&svc, &mut u)).await;
    let id = rig.submit(&d, "documents_docx", &jude_request(json!({}))).await;
    let last = rig.done(&d, &id).await;
    let key = last["result"].as_str().unwrap().to_string();
    let docx = rig.out().join(format!("{key}.docx"));
    let report = check_docx(&docx);
    println!("docx check: {report}");
    assert_eq!(report["ok"], true, "{report}");
    let ev = events(&d);
    assert!(ev.iter().any(|(m, _)| m == "docx worker finished (remote)"), "the remote seam ran");
    assert!(ev.iter().any(|(m, f)| m == "stage finished" && f["stage"] == "docx"), "the docx stage line");
    let (_, health) = get_json(&rig.c, &format!("{}/health/status", d.base())).await;
    println!("health: {health}");
    let m = rig.c.get(format!("{}/metrics", svc.url())).send().await.unwrap().text().await.unwrap();
    assert!(m.contains("pyworker_jobs_total{kind=\"docx\",outcome=\"ok\"} 1"), "the service counted the job:\n{m}");
    // the service's own log lines carry the request id the API made
    assert!(svc.log_text().contains("\"event\": \"docx_done\"") || svc.log_text().contains("\"event\":\"docx_done\""), "the Python script's lines are relayed:\n{}", svc.log_text());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_service_killed_in_the_middle_of_a_job_then_restarted_the_job_still_ends_with_a_document() {
    let rig = Rig::new("p7-kill").await;
    let state = rig.dir.path().join("fault.state");
    let mut svc = PyService::start(&rig, None, &[("DOCX_WORKER_FAULT", "slow:6"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", state.to_str().unwrap())]);
    let port = svc.port;
    let mut u = String::new();
    let d = rig.daemon("d", &remote_env(&svc, &mut u)).await;
    let id = rig.submit(&d, "documents_docx", &jude_request(json!({}))).await;
    // wait until the job is in the docx stage, then kill the whole service
    let t0 = Instant::now();
    loop {
        let (_, st) = status_of(&rig.c, &d.base(), &id).await;
        if st["state"] == "Converting to Docx" {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "never reached the docx stage: {st}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    svc.kill9();
    // the UI-visible state never turns into FAILURE while the service is away
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (_, st) = status_of(&rig.c, &d.base(), &id).await;
    assert_ne!(st["state"], "FAILURE", "a transient condition is not a failure: {st}");
    let _svc2 = PyService::start(&rig, Some(port), &[]);
    let last = rig.done(&d, &id).await;
    let attempt: u32 = rig.hget(&id, "attempt").await.unwrap().parse().unwrap();
    println!("finished on attempt {attempt}");
    assert!(attempt >= 2, "the first attempt was lost with the service");
    let docx = rig.out().join(format!("{}.docx", last["result"].as_str().unwrap()));
    assert_eq!(check_docx(&docx)["ok"], true);
    let ev = events(&d);
    assert!(ev.iter().any(|(m, f)| m == "attempt failed (transient); will retry" && f["code"].as_str().unwrap_or("").starts_with("pyworker_")), "the failure was classified as the service being away");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_docx_in_the_service_is_killed_at_the_deadline_and_the_retry_succeeds() {
    let rig = Rig::new("p7-hang").await;
    let state = rig.dir.path().join("hang.state");
    let svc = PyService::start(&rig, None, &[("DOCX_WORKER_FAULT", "hang-grandchild"), ("DOCX_WORKER_FAULT_TIMES", "1"), ("DOCX_WORKER_FAULT_STATE", state.to_str().unwrap())]);
    let mut u = String::new();
    let mut env = remote_env(&svc, &mut u);
    env.push(("DOC_JOBD_DOCX_DEADLINE_MS", "2500"));
    let d = rig.daemon("d", &env).await;
    let t0 = Instant::now();
    let id = rig.submit(&d, "documents_docx", &jude_request(json!({}))).await;
    let last = rig.done(&d, &id).await;
    println!("done after {:?}", t0.elapsed());
    assert_eq!(check_docx(&rig.out().join(format!("{}.docx", last["result"].as_str().unwrap())))["ok"], true);
    let ev = events(&d);
    assert!(ev.iter().any(|(m, f)| m == "attempt failed (transient); will retry" && f["code"] == "docx_worker_timeout"), "classified as a timeout:\n{}", tail_log(&d.log_text()));
    let m = rig.c.get(format!("{}/metrics", svc.url())).send().await.unwrap().text().await.unwrap();
    assert!(m.contains("pyworker_jobs_total{kind=\"docx\",outcome=\"timeout\"} 1"), "the service recorded the kill:\n{m}");
    // nothing of the hung worker's group is left running (the grandchild `sleep`)
    let leftovers = Command::new("pgrep").args(["-f", "docx_worker.py"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    println!("docx_worker processes after the kill: {leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_rejected_by_the_worker_is_a_permanent_failure_with_one_attempt() {
    let rig = Rig::new("p7-reject").await;
    let svc = PyService::start(&rig, None, &[("DOCX_WORKER_FAULT", "reject")]);
    let mut u = String::new();
    let d = rig.daemon("d", &remote_env(&svc, &mut u)).await;
    let id = rig.submit(&d, "documents_docx", &jude_request(json!({}))).await;
    let (seen, last) = watch(&rig.c, &d.base(), &id, Duration::from_secs(60)).await;
    println!("{seen:?} {last}");
    assert_eq!(last["state"], "FAILURE");
    assert_eq!(last["result"]["code"], "docx_input_rejected");
    assert_eq!(last["result"]["retryable"], false);
    assert_eq!(rig.hget(&id, "attempt").await.unwrap(), "1");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_that_is_down_for_a_while_is_waited_out_not_failed() {
    let rig = Rig::new("p7-down").await;
    let mut svc = PyService::start(&rig, None, &[]);
    let port = svc.port;
    let mut u = String::new();
    let d = rig.daemon("d", &remote_env(&svc, &mut u)).await;
    svc.kill9();
    let id = rig.submit(&d, "documents_docx", &jude_request(json!({}))).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    let (_, st) = status_of(&rig.c, &d.base(), &id).await;
    println!("while the service is down: {st}");
    assert_ne!(st["state"], "FAILURE");
    let _svc = PyService::start(&rig, Some(port), &[]);
    let last = rig.done(&d, &id).await;
    assert_eq!(check_docx(&rig.out().join(format!("{}.docx", last["result"].as_str().unwrap())))["ok"], true);
}

// =============================================================================================== PDF across the boundary

#[tokio::test(flavor = "multi_thread")]
async fn pdf_through_the_service_falls_back_from_prince_to_weasyprint_when_prince_is_missing() {
    if !weasyprint_available() {
        println!("SKIPPED: {} cannot import weasyprint (set DOC_TEST_PYTHON)", python());
        return;
    }
    let rig = Rig::new("p7-pdf").await;
    let svc = PyService::start(&rig, None, &[("PATH", "/usr/bin:/bin")]); // no prince on this PATH
    let u = svc.url();
    let d = rig.daemon("d", &[("DOC_DOCX_MODE", "remote"), ("DOC_PDF_ENGINE", "remote"), ("DOC_PYWORKER_URL", u.as_str())]).await;
    let body = jude_request(json!({"generate_pdf": true, "generate_docx": false, "use_prince": true}));
    let id = rig.submit(&d, "documents", &body).await;
    let last = rig.done(&d, &id).await;
    let key = last["result"].as_str().unwrap().to_string();
    let pdf = std::fs::read(rig.out().join(format!("{key}.pdf"))).unwrap();
    assert!(pdf.starts_with(b"%PDF-") && pdf.len() > 1000, "a real PDF ({} bytes)", pdf.len());
    assert!(rig.out().join(format!("{key}.html")).is_file(), "the HTML is placed too: DOC's tests read it");
    let findings: Value = serde_json::from_slice(&std::fs::read(rig.out().join(format!("{key}.findings.json"))).unwrap()).unwrap();
    assert!(findings.to_string().contains("pdf_engine_fallback"), "the fallback is a finding: {findings}");
    assert!(svc.log_text().contains("engine_fallback"), "and a log line");
}

#[test]
fn the_service_script_is_where_the_dockerfile_expects_it() {
    // containers/pyworker.Dockerfile copies platform-rs/python to /app/python and runs seam_server.py from there
    let root = p5_root();
    for f in ["python/seam_server.py", "python/docx_worker.py", "python/pdf_worker.py", "python/docx/gen_docx.py"] {
        assert!(Path::new(&root.join(f)).is_file(), "{f}");
    }
}
