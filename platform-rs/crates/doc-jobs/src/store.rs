//! The queue-and-state seam. Two implementations: [`crate::redis_store::RedisStore`] (Redis Streams
//! plus Lua, the production one) and [`crate::mem_store::MemStore`] (in-process, the Redis-less local
//! mode: one process, nothing survives a restart, no cross-process recovery to prove).

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use thiserror::Error;

use crate::model::{JobFailure, JobView};

#[derive(Debug, Error)]
pub enum StoreError {
    /// The backend cannot be reached or answered wrongly. Always transient from the runner's view.
    #[error("store unavailable: {0}")]
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct NewJob {
    pub task_id: String,
    /// Identical in-flight jobs share this (the document request key).
    pub dedup_key: String,
    pub kind: String,
    pub payload: String,
    /// The id of the HTTP request that created the job (`X-Request-Id`): it is on every log line of the job.
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enqueued {
    pub task_id: String,
    /// An identical request was already in flight; its task id is returned and nothing was queued.
    pub coalesced: bool,
}

/// A claimed job. `attempt` is the delivery count including this one; it is also the fencing token:
/// every later call carries the whole lease, and the store refuses calls from a superseded owner.
#[derive(Clone, Debug)]
pub struct Claim {
    pub task_id: String,
    pub kind: String,
    pub payload: String,
    pub attempt: u32,
    pub request_id: String,
    pub lease: Lease,
}

#[derive(Clone, Debug)]
pub struct Lease {
    pub task_id: String,
    pub stream_id: String,
    pub owner: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueHealth {
    pub queued: u64,
    pub running: u64,
    pub delayed: u64,
    pub stalled_recovered_total: u64,
    pub oldest_queued_age_ms: u64,
}

/// What an email claim answered. The email is sent at most once per task: `Granted` is the only answer
/// that allows a send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailClaim {
    /// Nobody has claimed it: this attempt may send.
    Granted,
    /// An earlier attempt confirmed the send; do not send again.
    AlreadySent,
    /// An earlier attempt claimed it and neither confirmed nor released: the message may or may not have
    /// gone out. At most once means: do not send again, and say so.
    InDoubt,
}

pub trait JobStore: Send + Sync + 'static {
    /// Add to named counters (`name|label=value`): the cluster-wide metrics live in the store so any node can
    /// render them. Best effort: a failure is the caller's to ignore.
    fn metric_add(&self, items: &[(String, u64)]) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn metrics(&self) -> impl Future<Output = Result<BTreeMap<String, u64>, StoreError>> + Send;
    /// A process announces itself: `info` is a JSON object; entries older than `ttl` are not alive.
    fn worker_beat(&self, id: &str, info: &str, ttl: Duration) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Registered processes as `(id, age_ms, info)`.
    fn workers(&self) -> impl Future<Output = Result<Vec<(String, u64, String)>, StoreError>> + Send;
    /// Claim the right to send the email of `task_id` (see [`EmailClaim`]).
    fn email_claim(&self, task_id: &str, owner: &str, ttl: Duration) -> impl Future<Output = Result<EmailClaim, StoreError>> + Send;
    /// The send is confirmed: later attempts and redeliveries must not send.
    fn email_sent(&self, task_id: &str, owner: &str, ttl: Duration) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// The send certainly did not happen (refused before the message was accepted): a later attempt may send.
    fn email_release(&self, task_id: &str, owner: &str) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Atomically: coalesce onto an in-flight job with the same dedup key, or create and queue.
    fn enqueue(&self, job: NewJob, in_flight_ttl: Duration) -> impl Future<Output = Result<Enqueued, StoreError>> + Send;
    /// Wait up to `block` for a job and claim it (lease set, attempt incremented, state running).
    fn next(&self, worker: &str, worker_tag: u64, block: Duration, lease_ttl: Duration) -> impl Future<Output = Result<Option<Claim>, StoreError>> + Send;
    /// Extend the lease. `false` means the job is no longer this worker's: stop work, write nothing.
    fn heartbeat(&self, lease: &Lease, lease_ttl: Duration) -> impl Future<Output = Result<bool, StoreError>> + Send;
    fn set_progress(&self, lease: &Lease, progress: &str) -> impl Future<Output = Result<bool, StoreError>> + Send;
    fn complete(&self, lease: &Lease, result: &str, result_ttl: Duration) -> impl Future<Output = Result<bool, StoreError>> + Send;
    fn fail(&self, lease: &Lease, failure: &JobFailure, result_ttl: Duration) -> impl Future<Output = Result<bool, StoreError>> + Send;
    /// Park the job for `delay`, then it is queued again (`promote_due`).
    fn retry_later(&self, lease: &Lease, delay: Duration, last: &JobFailure) -> impl Future<Output = Result<bool, StoreError>> + Send;
    /// Return the job to the queue now without counting the attempt (graceful shutdown).
    fn release(&self, lease: &Lease) -> impl Future<Output = Result<bool, StoreError>> + Send;
    /// Move due delayed jobs to the queue. Returns how many.
    fn promote_due(&self, max: usize) -> impl Future<Output = Result<u32, StoreError>> + Send;
    /// Re-queue jobs whose lease is gone (the worker died). Deliveries held by `worker` itself are
    /// skipped: this worker is alive and still owns them (their lease may only have lapsed during a
    /// store outage). Returns how many were re-queued.
    fn reap_stalled(&self, worker: &str, min_idle: Duration, max: usize) -> impl Future<Output = Result<u32, StoreError>> + Send;
    fn status(&self, task_id: &str) -> impl Future<Output = Result<Option<JobView>, StoreError>> + Send;
    fn health(&self) -> impl Future<Output = Result<QueueHealth, StoreError>> + Send;
}
