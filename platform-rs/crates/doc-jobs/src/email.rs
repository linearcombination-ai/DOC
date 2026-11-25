//! The email step: a small SMTP client written for one property, **the email of a task is sent at most once**,
//! and a store record (`none -> sending -> sent`, claimed before the send, confirmed after) that makes it
//! hold across retries, redeliveries and worker crashes.
//!
//! Why hand-written: the question that matters is *did the server accept the message*, and a library hides
//! the phase in which an error happened. Here the phases are explicit. An error before the server's answer to
//! the end of `DATA` means the message was **not** accepted (`Refused`: the claim is released, a retry may
//! send). An error after the message body was sent and before the server's final answer is `Ambiguous`: the
//! message may have been delivered, so the claim stays and nothing sends it again (reported as a finding).
//! SMTP itself cannot do better: no protocol makes that last step exactly-once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::{info, warn};

use crate::store::{EmailClaim, JobStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmtpTls {
    /// No TLS (a local relay, the test server). Credentials are not sent without TLS.
    None,
    /// `STARTTLS` after the greeting (DOC's own behaviour, port 587).
    StartTls,
    /// TLS from the first byte (port 465).
    Implicit,
}

#[derive(Clone, Debug)]
pub struct EmailConfig {
    /// DOC's `SEND_EMAIL`.
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tls: SmtpTls,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
    pub subject: String,
    /// A PEM file of extra trusted roots (a private relay); the system roots are always trusted.
    pub ca_file: Option<PathBuf>,
    pub timeout: Duration,
    pub body_template: Option<PathBuf>,
    /// How long the sent/claimed record is kept.
    pub record_ttl: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    /// The server did not accept the message (or it was never offered): safe to retry if `transient`.
    Refused { transient: bool, message: String },
    /// The message may have been delivered.
    Ambiguous { message: String },
}

/// What the step did, for the job's findings and the metrics.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    AlreadySent,
    InDoubt,
    Skipped,
    /// Not sent and not going to be (permanent refusal, or attempts used up): the document is still delivered.
    Failed(String),
    /// Not sent yet: the job should retry the step.
    RetryLater(String),
}

struct Reply {
    code: u16,
    text: String,
}

async fn read_reply<S: AsyncRead + AsyncWrite + Unpin>(c: &mut BufReader<S>, limit: Duration) -> Result<Reply, String> {
    let mut text = String::new();
    loop {
        let mut line = String::new();
        let n = tokio::time::timeout(limit, c.read_line(&mut line)).await.map_err(|_| "timed out waiting for the server".to_string())?.map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the server closed the connection".into());
        }
        let l = line.trim_end();
        let code: u16 = l.get(..3).and_then(|c| c.parse().ok()).ok_or_else(|| format!("not an SMTP reply: {l:?}"))?;
        text.push_str(l);
        text.push('\n');
        if l.as_bytes().get(3) != Some(&b'-') {
            return Ok(Reply { code, text });
        }
    }
}

async fn command<S: AsyncRead + AsyncWrite + Unpin>(c: &mut BufReader<S>, cmd: &str, limit: Duration) -> Result<Reply, String> {
    let w = c.get_mut();
    tokio::time::timeout(limit, async {
        w.write_all(cmd.as_bytes()).await?;
        w.write_all(b"\r\n").await?;
        w.flush().await
    })
    .await
    .map_err(|_| "timed out writing to the server".to_string())?
    .map_err(|e| e.to_string())?;
    read_reply(c, limit).await
}

fn tls_connector(cfg: &EmailConfig) -> Result<tokio_rustls::TlsConnector, String> {
    let mut roots = rustls::RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for c in native.certs {
        let _ = roots.add(c);
    }
    if let Some(path) = &cfg.ca_file {
        let pem = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for c in CertificateDer::pem_slice_iter(&pem) {
            roots.add(c.map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| e.to_string())?;
        }
    }
    let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

fn dot_stuff(message: &str) -> String {
    let mut out = String::with_capacity(message.len() + 16);
    for line in message.split("\r\n") {
        if line.starts_with('.') {
            out.push('.');
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out
}

fn wrap76(b64: &str) -> String {
    let mut out = String::with_capacity(b64.len() + b64.len() / 76 * 2 + 2);
    for chunk in b64.as_bytes().chunks(76) {
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push_str("\r\n");
    }
    out
}

/// A header value must not carry a line break (header injection through a name or subject).
fn header_safe(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

pub struct Mail<'a> {
    pub to: &'a str,
    pub message_id: String,
    pub body: String,
    pub attachment: Option<(&'a Path, &'a str)>,
}

fn build_message(cfg: &EmailConfig, m: &Mail<'_>) -> Result<String, String> {
    let boundary = format!("doc-{}", m.message_id.chars().filter(char::is_ascii_alphanumeric).collect::<String>());
    let mut s = String::new();
    s.push_str(&format!("From: {}\r\nTo: {}\r\nSubject: {}\r\nMessage-ID: <{}@doc>\r\nMIME-Version: 1.0\r\n", header_safe(&cfg.from), header_safe(m.to), header_safe(&cfg.subject), header_safe(&m.message_id)));
    s.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"));
    if let Some((path, mime)) = m.attachment {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map(|n| header_safe(&n.to_string_lossy())).unwrap_or_default();
        s.push_str(&format!("--{boundary}\r\nContent-Type: {mime}\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"{name}\"\r\n\r\n"));
        s.push_str(&wrap76(&B64.encode(data)));
    }
    s.push_str(&format!("--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"));
    s.push_str(&wrap76(&B64.encode(m.body.as_bytes())));
    s.push_str(&format!("--{boundary}--\r\n"));
    Ok(s)
}

macro_rules! expect {
    ($r:expr, $ok:expr, $what:expr) => {
        match $r {
            Ok(r) if $ok.contains(&r.code) => r,
            Ok(r) => return SendOutcome::Refused { transient: (400..500).contains(&r.code), message: format!("{} answered {}", $what, r.text.trim()) },
            Err(e) => return SendOutcome::Refused { transient: true, message: format!("{}: {e}", $what) },
        }
    };
}

async fn greet<S: AsyncRead + AsyncWrite + Unpin>(c: &mut BufReader<S>, lim: Duration, with_greeting: bool) -> SendOutcome {
    if with_greeting {
        expect!(read_reply(c, lim).await, [220], "greeting");
    }
    expect!(command(c, "EHLO doc.local", lim).await, [250], "EHLO");
    SendOutcome::Sent
}

/// AUTH, the envelope, DATA and the body. A failure before the server's answer to the end of `DATA` is a
/// refusal; after the body was written it is ambiguous.
async fn transact<S: AsyncRead + AsyncWrite + Unpin>(c: &mut BufReader<S>, cfg: &EmailConfig, m: &Mail<'_>, message: &str, secure: bool) -> SendOutcome {
    let lim = cfg.timeout;
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        if !secure {
            return SendOutcome::Refused { transient: false, message: "credentials are configured but TLS is off: refusing to send them in clear".into() };
        }
        let blob = B64.encode(format!("\0{u}\0{p}"));
        expect!(command(c, &format!("AUTH PLAIN {blob}"), lim).await, [235], "AUTH");
    }
    expect!(command(c, &format!("MAIL FROM:<{}>", header_safe(&cfg.from)), lim).await, [250], "MAIL FROM");
    expect!(command(c, &format!("RCPT TO:<{}>", header_safe(m.to)), lim).await, [250, 251], "RCPT TO");
    expect!(command(c, "DATA", lim).await, [354], "DATA");
    let body = dot_stuff(message);
    let sent: Result<(), String> = tokio::time::timeout(lim, async {
        let w = c.get_mut();
        w.write_all(body.as_bytes()).await?;
        w.write_all(b".\r\n").await?;
        w.flush().await
    })
    .await
    .map_err(|_| "timed out sending the message".to_string())
    .and_then(|r| r.map_err(|e| e.to_string()));
    if let Err(e) = sent {
        return SendOutcome::Ambiguous { message: e };
    }
    match read_reply(c, lim).await {
        Ok(r) if r.code == 250 => {
            let _ = command(c, "QUIT", Duration::from_secs(2)).await;
            SendOutcome::Sent
        }
        Ok(r) => SendOutcome::Refused { transient: (400..500).contains(&r.code), message: format!("the server did not accept the message: {}", r.text.trim()) },
        Err(e) => SendOutcome::Ambiguous { message: format!("no final answer after the message was sent: {e}") },
    }
}

/// One SMTP transaction. The outcome says whether the message was accepted, refused, or may have been.
pub async fn smtp_send(cfg: &EmailConfig, m: &Mail<'_>) -> SendOutcome {
    let refused = |transient: bool, message: String| SendOutcome::Refused { transient, message };
    let message = match build_message(cfg, m) {
        Ok(s) => s,
        Err(e) => return refused(true, format!("cannot build the message: {e}")),
    };
    let lim = cfg.timeout;
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let tcp = match tokio::time::timeout(lim, TcpStream::connect(&addr)).await {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return refused(true, format!("cannot connect to {addr}: {e}")),
        Err(_) => return refused(true, format!("connecting to {addr} timed out")),
    };
    let server_name = match ServerName::try_from(cfg.host.clone()) {
        Ok(n) => n,
        Err(e) => return refused(false, format!("bad SMTP host name: {e}")),
    };
    match cfg.tls {
        SmtpTls::None => {
            let mut c = BufReader::new(tcp);
            if let SendOutcome::Refused { transient, message } = greet(&mut c, lim, true).await {
                return refused(transient, message);
            }
            transact(&mut c, cfg, m, &message, false).await
        }
        SmtpTls::Implicit => {
            let conn = match tls_connector(cfg) {
                Ok(c) => c,
                Err(e) => return refused(true, format!("TLS setup: {e}")),
            };
            let tls = match tokio::time::timeout(lim, conn.connect(server_name, tcp)).await {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => return refused(true, format!("TLS handshake: {e}")),
                Err(_) => return refused(true, "TLS handshake timed out".into()),
            };
            let mut c = BufReader::new(tls);
            if let SendOutcome::Refused { transient, message } = greet(&mut c, lim, true).await {
                return refused(transient, message);
            }
            transact(&mut c, cfg, m, &message, true).await
        }
        SmtpTls::StartTls => {
            let conn = match tls_connector(cfg) {
                Ok(c) => c,
                Err(e) => return refused(true, format!("TLS setup: {e}")),
            };
            let mut plain = BufReader::new(tcp);
            if let SendOutcome::Refused { transient, message } = greet(&mut plain, lim, true).await {
                return refused(transient, message);
            }
            expect!(command(&mut plain, "STARTTLS", lim).await, [220], "STARTTLS");
            let tls = match tokio::time::timeout(lim, conn.connect(server_name, plain.into_inner())).await {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => return refused(true, format!("TLS handshake: {e}")),
                Err(_) => return refused(true, "TLS handshake timed out".into()),
            };
            let mut c = BufReader::new(tls);
            if let SendOutcome::Refused { transient, message } = greet(&mut c, lim, false).await {
                return refused(transient, message);
            }
            transact(&mut c, cfg, m, &message, true).await
        }
    }
}

pub fn email_body(template_file: Option<&Path>, task_key: &str) -> String {
    let t = template_file.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| "Your document is attached. Reference: {{ data }}".to_string());
    t.replace("{{ data }}", task_key).replace("{{data}}", task_key)
}

/// Test hook, never set in production: `DOC_FAULT_POINT` names a place in the email step where the process
/// dies (SIGABRT, no cleanup) the first time it passes (`DOC_FAULT_STATE` is a file that records that it did).
fn fault_point(name: &str) {
    if std::env::var("DOC_FAULT_POINT").as_deref() != Ok(name) {
        return;
    }
    if let Ok(state) = std::env::var("DOC_FAULT_STATE") {
        if std::path::Path::new(&state).exists() {
            return;
        }
        let _ = std::fs::write(&state, b"fired");
    }
    std::process::abort();
}

/// The idempotent step. `owner` identifies this attempt (worker and attempt); `task_id` is the idempotency key.
pub async fn deliver<S: JobStore>(store: &S, cfg: &EmailConfig, task_id: &str, owner: &str, mail: &Mail<'_>, final_attempt: bool) -> Delivery {
    if !cfg.enabled {
        return Delivery::Skipped;
    }
    match store.email_claim(task_id, owner, cfg.record_ttl).await {
        Err(e) => return Delivery::RetryLater(format!("the store is unavailable: {e}")),
        Ok(EmailClaim::AlreadySent) => {
            info!(task_id, "email already sent for this task; not sending again");
            return Delivery::AlreadySent;
        }
        Ok(EmailClaim::InDoubt) => {
            warn!(task_id, "an earlier attempt claimed the email and never confirmed it: it may have been sent; not sending again");
            return Delivery::InDoubt;
        }
        Ok(EmailClaim::Granted) => {}
    }
    fault_point("after_email_claim");
    match smtp_send(cfg, mail).await {
        SendOutcome::Sent => {
            fault_point("after_email_accepted");
            // the record is the only protection against a repeat: keep trying to write it
            for n in 1..=20u64 {
                if store.email_sent(task_id, owner, cfg.record_ttl).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200 * n)).await;
            }
            fault_point("after_email_recorded");
            info!(task_id, to = mail.to, "email sent");
            Delivery::Sent
        }
        SendOutcome::Refused { transient, message } => {
            let _ = store.email_release(task_id, owner).await;
            warn!(task_id, transient, error = %message, "email not sent");
            if transient && !final_attempt {
                Delivery::RetryLater(message)
            } else {
                Delivery::Failed(message)
            }
        }
        SendOutcome::Ambiguous { message } => {
            warn!(task_id, error = %message, "the email may or may not have been delivered; it will not be sent again");
            Delivery::InDoubt
        }
    }
}
