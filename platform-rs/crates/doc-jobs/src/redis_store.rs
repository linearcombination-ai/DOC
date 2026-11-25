//! Redis Streams + Lua. One stream (`<ns>:stream`) with one consumer group; the job record is a hash;
//! a lease key with a TTL says a live worker owns the job; a sorted set holds jobs waiting out a backoff.
//! Every state change is one Lua script, so a crash between two steps cannot leave a half-moved job.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use redis::aio::{ConnectionManager, ConnectionManagerConfig, MultiplexedConnection};
use redis::streams::StreamReadReply;
use redis::{Client, Script};
use tokio::sync::Mutex;

use crate::model::{JobFailure, JobView, FAILURE, PENDING, SUCCESS};
use crate::store::{Claim, EmailClaim, Enqueued, JobStore, Lease, NewJob, QueueHealth, StoreError};

const GROUP: &str = "workers";

struct Scripts {
    enqueue: Script,
    claim: Script,
    heartbeat: Script,
    progress: Script,
    finish: Script,
    retry: Script,
    promote: Script,
    reap: Script,
    email: Script,
}

pub struct RedisStore {
    ns: String,
    client: Client,
    conn: ConnectionManager,
    /// The blocking read gets its own connection: a blocked command would stall a shared multiplexed one.
    reader: Mutex<Option<MultiplexedConnection>>,
    scripts: Arc<Scripts>,
}

fn unavailable(e: impl std::fmt::Display) -> StoreError {
    StoreError::Unavailable(e.to_string())
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// How to reach Redis (or Valkey): the URL (`redis://` or `rediss://`), credentials kept out of the URL so they
/// are not logged, and an extra trusted root for a private CA.
#[derive(Clone, Default)]
pub struct RedisOptions {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ca_file: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for RedisOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisOptions").field("url", &redact(&self.url)).field("username", &self.username).field("password", &self.password.as_ref().map(|_| "***")).field("ca_file", &self.ca_file).finish()
    }
}

/// The URL without any `user:password@` part.
pub fn redact(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(i), Some(at)) if at > i => format!("{}***@{}", &url[..i + 3], &url[at + 1..]),
        _ => url.to_string(),
    }
}

impl RedisStore {
    pub async fn connect(url: &str, ns: &str) -> Result<RedisStore, StoreError> {
        RedisStore::connect_with(&RedisOptions { url: url.to_string(), ..RedisOptions::default() }, ns).await
    }

    fn open_client(o: &RedisOptions) -> Result<Client, StoreError> {
        use redis::IntoConnectionInfo;
        let mut info = o.url.as_str().into_connection_info().map_err(unavailable)?;
        let mut settings = info.redis_settings().clone();
        if let Some(u) = &o.username {
            settings = settings.set_username(u);
        }
        if let Some(p) = &o.password {
            settings = settings.set_password(p);
        }
        info = info.set_redis_settings(settings);
        match &o.ca_file {
            Some(path) => {
                let pem = std::fs::read(path).map_err(|e| StoreError::Unavailable(format!("{}: {e}", path.display())))?;
                Client::build_with_tls(info, redis::TlsCertificates { client_tls: None, root_cert: Some(pem) }).map_err(unavailable)
            }
            None => Client::open(info).map_err(unavailable),
        }
    }

    pub async fn connect_with(o: &RedisOptions, ns: &str) -> Result<RedisStore, StoreError> {
        let client = RedisStore::open_client(o)?;
        // Bounded waits: the default manager retries six times with growing delays and holds every command
        // meanwhile (RAN: a POST took 9 s to answer 503 while Redis was down). Two quick retries, then fail.
        let cfg = ConnectionManagerConfig::new()
            .set_number_of_retries(2)
            .set_min_delay(Duration::from_millis(100))
            .set_max_delay(Duration::from_millis(400))
            .set_connection_timeout(Some(Duration::from_secs(1)))
            .set_response_timeout(Some(Duration::from_secs(3)));
        let conn = client.get_connection_manager_with_config(cfg).await.map_err(unavailable)?;
        let scripts = Scripts {
            enqueue: Script::new(include_str!("../lua/enqueue.lua")),
            claim: Script::new(include_str!("../lua/claim.lua")),
            heartbeat: Script::new(include_str!("../lua/heartbeat.lua")),
            progress: Script::new(include_str!("../lua/progress.lua")),
            finish: Script::new(include_str!("../lua/finish.lua")),
            retry: Script::new(include_str!("../lua/retry.lua")),
            promote: Script::new(include_str!("../lua/promote.lua")),
            reap: Script::new(include_str!("../lua/reap.lua")),
            email: Script::new(include_str!("../lua/email.lua")),
        };
        let store = RedisStore { ns: ns.to_string(), client, conn, reader: Mutex::new(None), scripts: Arc::new(scripts) };
        store.ensure_group().await?;
        Ok(store)
    }

    /// A handle for the pieces that share this Redis (the clone lock).
    pub fn connection(&self) -> ConnectionManager {
        self.conn.clone()
    }
    pub fn namespace(&self) -> &str {
        &self.ns
    }

    fn k(&self, part: &str) -> String {
        format!("{}:{part}", self.ns)
    }
    fn job_key(&self, id: &str) -> String {
        format!("{}:job:{id}", self.ns)
    }
    fn lease_key(&self, id: &str) -> String {
        format!("{}:lease:{id}", self.ns)
    }
    fn inflight_key(&self, dedup: &str) -> String {
        format!("{}:inflight:{dedup}", self.ns)
    }

    /// Redis restarted without its data (no persistence) or the stream was lost: make the group again.
    async fn ensure_group(&self) -> Result<(), StoreError> {
        let mut c = self.conn.clone();
        let r: Result<String, redis::RedisError> =
            redis::cmd("XGROUP").arg("CREATE").arg(self.k("stream")).arg(GROUP).arg("$").arg("MKSTREAM").query_async(&mut c).await;
        match r {
            Ok(_) => Ok(()),
            Err(e) if e.code() == Some("BUSYGROUP") => Ok(()),
            Err(e) => Err(unavailable(e)),
        }
    }

    async fn job_dedup(&self, id: &str) -> Result<Option<String>, StoreError> {
        let mut c = self.conn.clone();
        let v: Option<String> = redis::cmd("HGET").arg(self.job_key(id)).arg("dedup").query_async(&mut c).await.map_err(unavailable)?;
        Ok(v)
    }

    async fn finish(&self, lease: &Lease, mode: &str, payload: &str, ttl: Duration) -> Result<bool, StoreError> {
        let Some(dedup) = self.job_dedup(&lease.task_id).await? else { return Ok(false) };
        let mut c = self.conn.clone();
        let n: i64 = self
            .scripts
            .finish
            .key(self.job_key(&lease.task_id))
            .key(self.lease_key(&lease.task_id))
            .key(self.inflight_key(&dedup))
            .key(self.k("stream"))
            .arg(&lease.owner)
            .arg(mode)
            .arg(payload)
            .arg(ttl.as_secs())
            .arg(&lease.stream_id)
            .arg(&lease.task_id)
            .arg(GROUP)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(n == 1)
    }

    async fn park(&self, lease: &Lease, mode: &str, delay: Duration, last: &str) -> Result<bool, StoreError> {
        let Some(dedup) = self.job_dedup(&lease.task_id).await? else { return Ok(false) };
        let mut c = self.conn.clone();
        let n: i64 = self
            .scripts
            .retry
            .key(self.job_key(&lease.task_id))
            .key(self.lease_key(&lease.task_id))
            .key(self.inflight_key(&dedup))
            .key(self.k("stream"))
            .key(self.k("delayed"))
            .arg(&lease.owner)
            .arg(mode)
            .arg(ms(delay))
            .arg(last)
            .arg(&lease.stream_id)
            .arg(&lease.task_id)
            .arg(GROUP)
            .arg(6 * 3600 * 1000u64)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(n == 1)
    }
}

impl JobStore for RedisStore {
    async fn metric_add(&self, items: &[(String, u64)]) -> Result<(), StoreError> {
        if items.is_empty() {
            return Ok(());
        }
        let mut c = self.conn.clone();
        let mut pipe = redis::pipe();
        for (name, n) in items {
            pipe.cmd("HINCRBY").arg(self.k("metrics")).arg(name).arg(*n).ignore();
        }
        pipe.query_async::<()>(&mut c).await.map_err(unavailable)
    }

    async fn metrics(&self) -> Result<BTreeMap<String, u64>, StoreError> {
        let mut c = self.conn.clone();
        let m: BTreeMap<String, u64> = redis::cmd("HGETALL").arg(self.k("metrics")).query_async(&mut c).await.map_err(unavailable)?;
        Ok(m)
    }

    async fn worker_beat(&self, id: &str, info: &str, _ttl: Duration) -> Result<(), StoreError> {
        let mut c = self.conn.clone();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(ms).unwrap_or(0);
        redis::cmd("HSET").arg(self.k("workers")).arg(id).arg(format!("{now}|{info}")).query_async::<i64>(&mut c).await.map_err(unavailable)?;
        Ok(())
    }

    async fn workers(&self) -> Result<Vec<(String, u64, String)>, StoreError> {
        let mut c = self.conn.clone();
        let all: BTreeMap<String, String> = redis::cmd("HGETALL").arg(self.k("workers")).query_async(&mut c).await.map_err(unavailable)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(ms).unwrap_or(0);
        let mut out = Vec::new();
        for (id, v) in all {
            let Some((ts, info)) = v.split_once('|') else { continue };
            let age = now.saturating_sub(ts.parse::<u64>().unwrap_or(0));
            if age > 3_600_000 {
                // a worker that has been gone for an hour is forgotten
                let _ = redis::cmd("HDEL").arg(self.k("workers")).arg(&id).query_async::<i64>(&mut c).await;
                continue;
            }
            out.push((id, age, info.to_string()));
        }
        Ok(out)
    }

    async fn email_claim(&self, task_id: &str, owner: &str, ttl: Duration) -> Result<EmailClaim, StoreError> {
        let mut c = self.conn.clone();
        let r: String = self.scripts.email.key(self.k(&format!("email:{task_id}"))).arg("claim").arg(owner).arg(ttl.as_secs()).invoke_async(&mut c).await.map_err(unavailable)?;
        Ok(match r.as_str() {
            "granted" => EmailClaim::Granted,
            "sent" => EmailClaim::AlreadySent,
            _ => EmailClaim::InDoubt,
        })
    }

    async fn email_sent(&self, task_id: &str, owner: &str, ttl: Duration) -> Result<(), StoreError> {
        let mut c = self.conn.clone();
        let _: String = self.scripts.email.key(self.k(&format!("email:{task_id}"))).arg("sent").arg(owner).arg(ttl.as_secs()).invoke_async(&mut c).await.map_err(unavailable)?;
        Ok(())
    }

    async fn email_release(&self, task_id: &str, owner: &str) -> Result<(), StoreError> {
        let mut c = self.conn.clone();
        let _: String = self.scripts.email.key(self.k(&format!("email:{task_id}"))).arg("release").arg(owner).arg(0).invoke_async(&mut c).await.map_err(unavailable)?;
        Ok(())
    }

    async fn enqueue(&self, job: NewJob, in_flight_ttl: Duration) -> Result<Enqueued, StoreError> {
        let mut c = self.conn.clone();
        let (task_id, coalesced): (String, i64) = self
            .scripts
            .enqueue
            .key(self.inflight_key(&job.dedup_key))
            .key(self.job_key(&job.task_id))
            .key(self.k("stream"))
            .arg(&job.task_id)
            .arg(&job.kind)
            .arg(&job.payload)
            .arg(&job.dedup_key)
            .arg(ms(in_flight_ttl))
            .arg(format!("{}:job:", self.ns))
            .arg(&job.request_id)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(Enqueued { task_id, coalesced: coalesced == 1 })
    }

    async fn next(&self, worker: &str, worker_tag: u64, block: Duration, lease_ttl: Duration) -> Result<Option<Claim>, StoreError> {
        let mut guard = self.reader.lock().await;
        if guard.is_none() {
            // the blocking read may legitimately wait `block`; give it room, but not forever
            let cfg = redis::AsyncConnectionConfig::new().set_connection_timeout(Some(Duration::from_secs(1))).set_response_timeout(Some(block + Duration::from_secs(10)));
            *guard = Some(self.client.get_multiplexed_async_connection_with_config(&cfg).await.map_err(unavailable)?);
        }
        let Some(rc) = guard.as_mut() else { return Ok(None) };
        let reply: Result<Option<StreamReadReply>, redis::RedisError> = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(GROUP)
            .arg(worker)
            .arg("COUNT")
            .arg(1)
            .arg("BLOCK")
            .arg(ms(block))
            .arg("STREAMS")
            .arg(self.k("stream"))
            .arg(">")
            .query_async(rc)
            .await;
        let reply = match reply {
            Ok(r) => r,
            Err(e) => {
                *guard = None;
                if e.code() == Some("NOGROUP") {
                    drop(guard);
                    self.ensure_group().await?;
                }
                return Err(unavailable(e));
            }
        };
        drop(guard);
        let Some(reply) = reply else { return Ok(None) };
        let Some(entry) = reply.keys.into_iter().next().and_then(|k| k.ids.into_iter().next()) else { return Ok(None) };
        let Some(task_id) = entry.get::<String>("id") else {
            // A message without an id cannot be matched to a job: drop it so it cannot loop.
            let mut c = self.conn.clone();
            let _: Result<i64, _> = redis::cmd("XACK").arg(self.k("stream")).arg(GROUP).arg(&entry.id).query_async(&mut c).await;
            return Ok(None);
        };
        let owner = format!("{worker}#{worker_tag}");
        let mut c = self.conn.clone();
        let claimed: Option<(String, String, i64, String)> = self
            .scripts
            .claim
            .key(self.job_key(&task_id))
            .key(self.lease_key(&task_id))
            .key(self.k("stream"))
            .arg(&task_id)
            .arg(&entry.id)
            .arg(&owner)
            .arg(ms(lease_ttl))
            .arg(GROUP)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(claimed.map(|(kind, payload, attempt, request_id)| Claim {
            task_id: task_id.clone(),
            kind,
            payload,
            attempt: u32::try_from(attempt).unwrap_or(u32::MAX),
            request_id,
            lease: Lease { task_id, stream_id: entry.id, owner },
        }))
    }

    async fn heartbeat(&self, lease: &Lease, lease_ttl: Duration) -> Result<bool, StoreError> {
        let mut c = self.conn.clone();
        let n: i64 = self
            .scripts
            .heartbeat
            .key(self.lease_key(&lease.task_id))
            .key(self.job_key(&lease.task_id))
            .arg(&lease.owner)
            .arg(ms(lease_ttl))
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(n == 1)
    }

    async fn set_progress(&self, lease: &Lease, progress: &str) -> Result<bool, StoreError> {
        let mut c = self.conn.clone();
        let n: i64 = self
            .scripts
            .progress
            .key(self.job_key(&lease.task_id))
            .key(self.k(&format!("hist:{}", lease.task_id)))
            .arg(&lease.owner)
            .arg(progress)
            .arg(86_400u64)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(n == 1)
    }

    async fn complete(&self, lease: &Lease, result: &str, result_ttl: Duration) -> Result<bool, StoreError> {
        self.finish(lease, "done", result, result_ttl).await
    }

    async fn fail(&self, lease: &Lease, failure: &JobFailure, result_ttl: Duration) -> Result<bool, StoreError> {
        let json = serde_json::to_string(failure).map_err(unavailable)?;
        self.finish(lease, "failed", &json, result_ttl).await
    }

    async fn retry_later(&self, lease: &Lease, delay: Duration, last: &JobFailure) -> Result<bool, StoreError> {
        let json = serde_json::to_string(last).map_err(unavailable)?;
        self.park(lease, "later", delay, &json).await
    }

    async fn release(&self, lease: &Lease) -> Result<bool, StoreError> {
        self.park(lease, "release", Duration::ZERO, "").await
    }

    async fn promote_due(&self, max: usize) -> Result<u32, StoreError> {
        let mut c = self.conn.clone();
        let n: i64 = self.scripts.promote.key(self.k("delayed")).key(self.k("stream")).arg(max).arg(format!("{}:job:", self.ns)).invoke_async(&mut c).await.map_err(unavailable)?;
        Ok(u32::try_from(n).unwrap_or(0))
    }

    async fn reap_stalled(&self, worker: &str, min_idle: Duration, max: usize) -> Result<u32, StoreError> {
        let mut c = self.conn.clone();
        let n: i64 = self
            .scripts
            .reap
            .key(self.k("stream"))
            .key(self.k("stats"))
            .arg(GROUP)
            .arg(format!("{}:job:", self.ns))
            .arg(format!("{}:lease:", self.ns))
            .arg(ms(min_idle))
            .arg(max)
            .arg(worker)
            .invoke_async(&mut c)
            .await
            .map_err(unavailable)?;
        Ok(u32::try_from(n).unwrap_or(0))
    }

    async fn status(&self, task_id: &str) -> Result<Option<JobView>, StoreError> {
        let mut c = self.conn.clone();
        let f: Vec<Option<String>> = redis::cmd("HMGET")
            .arg(self.job_key(task_id))
            .arg(&["st", "progress", "result", "failure", "attempt", "retrying"])
            .query_async(&mut c)
            .await
            .map_err(unavailable)?;
        let get = |i: usize| f.get(i).cloned().flatten();
        let Some(st) = get(0) else { return Ok(None) };
        let progress = get(1).unwrap_or_default();
        let state = match st.as_str() {
            "done" => SUCCESS.to_string(),
            "failed" => FAILURE.to_string(),
            _ if progress.is_empty() => PENDING.to_string(),
            _ => progress,
        };
        Ok(Some(JobView {
            task_id: task_id.to_string(),
            state,
            result: get(2),
            failure: get(3).and_then(|j| serde_json::from_str(&j).ok()),
            attempt: get(4).and_then(|a| a.parse().ok()).unwrap_or(0),
            retrying: get(5).as_deref() == Some("1"),
        }))
    }

    async fn health(&self) -> Result<QueueHealth, StoreError> {
        let mut c = self.conn.clone();
        let len: u64 = redis::cmd("XLEN").arg(self.k("stream")).query_async(&mut c).await.map_err(unavailable)?;
        let pending: (u64, Option<String>, Option<String>, Option<Vec<(String, String)>>) =
            redis::cmd("XPENDING").arg(self.k("stream")).arg(GROUP).query_async(&mut c).await.map_err(unavailable)?;
        let delayed: u64 = redis::cmd("ZCARD").arg(self.k("delayed")).query_async(&mut c).await.map_err(unavailable)?;
        let stalled: Option<u64> = redis::cmd("HGET").arg(self.k("stats")).arg("stalled_recovered").query_async(&mut c).await.map_err(unavailable)?;
        let oldest: Vec<(String, Vec<String>)> = redis::cmd("XRANGE").arg(self.k("stream")).arg("-").arg("+").arg("COUNT").arg(1).query_async(&mut c).await.map_err(unavailable)?;
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| ms(d)).unwrap_or(0);
        let oldest_age = oldest.first().and_then(|(id, _)| id.split('-').next().and_then(|m| m.parse::<u64>().ok())).map(|t| now_ms.saturating_sub(t)).unwrap_or(0);
        Ok(QueueHealth { queued: len.saturating_sub(pending.0), running: pending.0, delayed, stalled_recovered_total: stalled.unwrap_or(0), oldest_queued_age_ms: oldest_age })
    }
}
