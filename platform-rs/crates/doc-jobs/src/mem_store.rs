//! The Redis-less local mode: the same contract in one process behind a mutex. It proves the runner
//! logic (statuses, retries, coalescing, fencing against a lost lease) but NOT anything that needs a
//! second process: survival of a worker SIGKILL, a Redis restart, or two workers racing.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use crate::model::{JobFailure, JobView, FAILURE, PENDING, SUCCESS};
use crate::store::{Claim, EmailClaim, Enqueued, JobStore, Lease, NewJob, QueueHealth, StoreError};

#[derive(Clone, Copy, PartialEq, Eq)]
enum St {
    Queued,
    Running,
    Retrying,
    Done,
    Failed,
}

struct Rec {
    st: St,
    kind: String,
    payload: String,
    request_id: String,
    dedup: String,
    progress: String,
    attempt: u32,
    owner: String,
    result: Option<String>,
    failure: Option<JobFailure>,
}

#[derive(Default)]
struct State {
    jobs: HashMap<String, Rec>,
    inflight: HashMap<String, String>,
    queue: VecDeque<String>,
    leases: HashMap<String, (String, Instant)>,
    delayed: BTreeMap<(Instant, String), ()>,
    stalled: u64,
    metrics: BTreeMap<String, u64>,
    workers: HashMap<String, (Instant, String)>,
    emails: HashMap<String, String>,
}

#[derive(Default)]
pub struct MemStore {
    state: Mutex<State>,
    wake: Notify,
}

impl MemStore {
    pub fn new() -> MemStore {
        MemStore::default()
    }
    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> Result<T, StoreError> {
        let mut g = self.state.lock().map_err(|e| StoreError::Unavailable(format!("poisoned: {e}")))?;
        Ok(f(&mut g))
    }
}

fn owns(s: &State, lease: &Lease) -> bool {
    s.jobs.get(&lease.task_id).is_some_and(|r| r.owner == lease.owner && r.st == St::Running)
}

impl JobStore for MemStore {
    async fn metric_add(&self, items: &[(String, u64)]) -> Result<(), StoreError> {
        self.with(|s| {
            for (k, n) in items {
                *s.metrics.entry(k.clone()).or_insert(0) += n;
            }
        })
    }
    async fn metrics(&self) -> Result<BTreeMap<String, u64>, StoreError> {
        self.with(|s| s.metrics.clone())
    }
    async fn worker_beat(&self, id: &str, info: &str, _ttl: Duration) -> Result<(), StoreError> {
        self.with(|s| {
            s.workers.insert(id.to_string(), (Instant::now(), info.to_string()));
        })
    }
    async fn workers(&self) -> Result<Vec<(String, u64, String)>, StoreError> {
        self.with(|s| s.workers.iter().map(|(id, (t, info))| (id.clone(), u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX), info.clone())).collect())
    }
    async fn email_claim(&self, task_id: &str, owner: &str, _ttl: Duration) -> Result<EmailClaim, StoreError> {
        self.with(|s| {
            let mine = format!("sending:{owner}");
            match s.emails.get(task_id).map(String::as_str) {
                None => {
                    s.emails.insert(task_id.to_string(), mine);
                    EmailClaim::Granted
                }
                Some("sent") => EmailClaim::AlreadySent,
                Some(v) if v == mine => EmailClaim::Granted,
                Some(_) => EmailClaim::InDoubt,
            }
        })
    }
    async fn email_sent(&self, task_id: &str, owner: &str, _ttl: Duration) -> Result<(), StoreError> {
        self.with(|s| {
            let mine = format!("sending:{owner}");
            if s.emails.get(task_id).is_none_or(|v| *v == mine) {
                s.emails.insert(task_id.to_string(), "sent".to_string());
            }
        })
    }
    async fn email_release(&self, task_id: &str, owner: &str) -> Result<(), StoreError> {
        self.with(|s| {
            if s.emails.get(task_id).is_some_and(|v| *v == format!("sending:{owner}")) {
                s.emails.remove(task_id);
            }
        })
    }

    async fn enqueue(&self, job: NewJob, _ttl: Duration) -> Result<Enqueued, StoreError> {
        let out = self.with(|s| {
            if let Some(existing) = s.inflight.get(&job.dedup_key) {
                if s.jobs.get(existing).is_some_and(|r| !matches!(r.st, St::Done | St::Failed)) {
                    return Enqueued { task_id: existing.clone(), coalesced: true };
                }
            }
            s.jobs.insert(
                job.task_id.clone(),
                Rec { st: St::Queued, kind: job.kind, payload: job.payload, request_id: job.request_id, dedup: job.dedup_key.clone(), progress: String::new(), attempt: 0, owner: String::new(), result: None, failure: None },
            );
            s.inflight.insert(job.dedup_key, job.task_id.clone());
            s.queue.push_back(job.task_id.clone());
            Enqueued { task_id: job.task_id, coalesced: false }
        })?;
        self.wake.notify_one();
        Ok(out)
    }

    async fn next(&self, worker: &str, worker_tag: u64, block: Duration, lease_ttl: Duration) -> Result<Option<Claim>, StoreError> {
        let deadline = Instant::now() + block;
        loop {
            let notified = self.wake.notified();
            let got = self.with(|s| {
                while let Some(id) = s.queue.pop_front() {
                    let Some(rec) = s.jobs.get_mut(&id) else { continue };
                    if matches!(rec.st, St::Done | St::Failed) {
                        continue;
                    }
                    let owner = format!("{worker}#{worker_tag}");
                    rec.attempt += 1;
                    rec.st = St::Running;
                    rec.owner = owner.clone();
                    let claim = Claim { task_id: id.clone(), kind: rec.kind.clone(), payload: rec.payload.clone(), attempt: rec.attempt, request_id: rec.request_id.clone(), lease: Lease { task_id: id.clone(), stream_id: String::new(), owner: owner.clone() } };
                    s.leases.insert(id, (owner, Instant::now() + lease_ttl));
                    return Some(claim);
                }
                None
            })?;
            if got.is_some() {
                return Ok(got);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let _ = tokio::time::timeout(left, notified).await;
        }
    }

    async fn heartbeat(&self, lease: &Lease, ttl: Duration) -> Result<bool, StoreError> {
        self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            s.leases.insert(lease.task_id.clone(), (lease.owner.clone(), Instant::now() + ttl));
            true
        })
    }

    async fn set_progress(&self, lease: &Lease, progress: &str) -> Result<bool, StoreError> {
        self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            if let Some(r) = s.jobs.get_mut(&lease.task_id) {
                r.progress = progress.to_string();
            }
            true
        })
    }

    async fn complete(&self, lease: &Lease, result: &str, _ttl: Duration) -> Result<bool, StoreError> {
        self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            if let Some(r) = s.jobs.get_mut(&lease.task_id) {
                r.st = St::Done;
                r.result = Some(result.to_string());
                r.owner.clear();
                let dedup = r.dedup.clone();
                if s.inflight.get(&dedup) == Some(&lease.task_id) {
                    s.inflight.remove(&dedup);
                }
            }
            s.leases.remove(&lease.task_id);
            true
        })
    }

    async fn fail(&self, lease: &Lease, failure: &JobFailure, _ttl: Duration) -> Result<bool, StoreError> {
        self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            if let Some(r) = s.jobs.get_mut(&lease.task_id) {
                r.st = St::Failed;
                r.failure = Some(failure.clone());
                r.owner.clear();
                let dedup = r.dedup.clone();
                if s.inflight.get(&dedup) == Some(&lease.task_id) {
                    s.inflight.remove(&dedup);
                }
            }
            s.leases.remove(&lease.task_id);
            true
        })
    }

    async fn retry_later(&self, lease: &Lease, delay: Duration, last: &JobFailure) -> Result<bool, StoreError> {
        self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            if let Some(r) = s.jobs.get_mut(&lease.task_id) {
                r.st = St::Retrying;
                r.failure = Some(last.clone());
                r.owner.clear();
            }
            s.leases.remove(&lease.task_id);
            s.delayed.insert((Instant::now() + delay, lease.task_id.clone()), ());
            true
        })
    }

    async fn release(&self, lease: &Lease) -> Result<bool, StoreError> {
        let ok = self.with(|s| {
            if !owns(s, lease) {
                return false;
            }
            if let Some(r) = s.jobs.get_mut(&lease.task_id) {
                r.st = St::Queued;
                r.attempt = r.attempt.saturating_sub(1);
                r.owner.clear();
            }
            s.leases.remove(&lease.task_id);
            s.queue.push_back(lease.task_id.clone());
            true
        })?;
        self.wake.notify_one();
        Ok(ok)
    }

    async fn promote_due(&self, max: usize) -> Result<u32, StoreError> {
        let n = self.with(|s| {
            let now = Instant::now();
            let due: Vec<(Instant, String)> = s.delayed.keys().filter(|(t, _)| *t <= now).take(max).cloned().collect();
            for k in &due {
                s.delayed.remove(k);
                if let Some(r) = s.jobs.get_mut(&k.1) {
                    r.st = St::Queued;
                }
                s.queue.push_back(k.1.clone());
            }
            u32::try_from(due.len()).unwrap_or(0)
        })?;
        if n > 0 {
            self.wake.notify_waiters();
            self.wake.notify_one();
        }
        Ok(n)
    }

    async fn reap_stalled(&self, _worker: &str, _min_idle: Duration, _max: usize) -> Result<u32, StoreError> {
        let n = self.with(|s| {
            let now = Instant::now();
            let dead: Vec<String> = s.leases.iter().filter(|(_, (_, exp))| *exp <= now).map(|(id, _)| id.clone()).collect();
            let mut n = 0;
            for id in dead {
                s.leases.remove(&id);
                if let Some(r) = s.jobs.get_mut(&id) {
                    if r.st == St::Running {
                        r.st = St::Queued;
                        r.owner.clear();
                        s.queue.push_back(id);
                        s.stalled += 1;
                        n += 1;
                    }
                }
            }
            n
        })?;
        if n > 0 {
            self.wake.notify_one();
        }
        Ok(n)
    }

    async fn status(&self, task_id: &str) -> Result<Option<JobView>, StoreError> {
        self.with(|s| {
            s.jobs.get(task_id).map(|r| JobView {
                task_id: task_id.to_string(),
                state: match r.st {
                    St::Done => SUCCESS.to_string(),
                    St::Failed => FAILURE.to_string(),
                    _ if r.progress.is_empty() => PENDING.to_string(),
                    _ => r.progress.clone(),
                },
                result: r.result.clone(),
                failure: r.failure.clone(),
                attempt: r.attempt,
                retrying: r.st == St::Retrying,
            })
        })
    }

    async fn health(&self) -> Result<QueueHealth, StoreError> {
        self.with(|s| QueueHealth {
            queued: s.queue.len() as u64,
            running: s.jobs.values().filter(|r| r.st == St::Running).count() as u64,
            delayed: s.delayed.len() as u64,
            stalled_recovered_total: s.stalled,
            oldest_queued_age_ms: 0,
        })
    }
}
