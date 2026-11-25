//! The runner: claim loop, lease heartbeats, retry with backoff and jitter, the reaper, the delayed-job
//! promoter, graceful drain. Written against [`JobStore`], so the same code runs on Redis or in memory.

use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::metrics;
use crate::model::{JobError, JobFailure, Progress};
use crate::retry::Backoff;
use crate::store::{Claim, JobStore, Lease};

#[derive(Clone, Debug)]
pub struct RunnerConfig {
    pub worker_id: String,
    pub concurrency: usize,
    /// How long a dead worker's job stays invisible before another worker takes it (stall recovery latency).
    pub lease_ttl: Duration,
    pub claim_block: Duration,
    pub promote_every: Duration,
    pub reap_every: Duration,
    pub max_attempts: u32,
    pub backoff: Backoff,
    /// One attempt's ceiling, over all stages. A generous backstop; stages have their own deadlines.
    pub job_ceiling: Duration,
    /// On SIGTERM: how long running jobs may finish before they are handed back to the queue.
    pub drain_deadline: Duration,
    pub result_ttl: Duration,
    pub in_flight_ttl: Duration,
    /// Waits between attempts to talk to a store that is down.
    pub store_backoff: Backoff,
}

impl RunnerConfig {
    pub fn production(worker_id: &str) -> RunnerConfig {
        RunnerConfig {
            worker_id: worker_id.to_string(),
            concurrency: 2,
            lease_ttl: Duration::from_secs(60),
            claim_block: Duration::from_secs(2),
            promote_every: Duration::from_secs(1),
            reap_every: Duration::from_secs(5),
            max_attempts: 8,
            backoff: Backoff { base: Duration::from_secs(5), cap: Duration::from_secs(300), floor: Duration::from_millis(500) },
            job_ceiling: Duration::from_secs(1800),
            drain_deadline: Duration::from_secs(120),
            result_ttl: Duration::from_secs(24 * 3600),
            in_flight_ttl: Duration::from_secs(6 * 3600),
            store_backoff: Backoff { base: Duration::from_millis(200), cap: Duration::from_secs(5), floor: Duration::from_millis(100) },
        }
    }
}

/// What a handler sees of its job.
#[derive(Clone)]
pub struct JobCtx {
    pub task_id: String,
    pub attempt: u32,
    pub kind: String,
    pub payload: String,
    /// The `X-Request-Id` of the HTTP request that created the job.
    pub request_id: String,
    /// Attempts allowed in all; the last one must end the job one way or the other.
    pub max_attempts: u32,
    sink: Arc<dyn Fn(String) -> BoxFuture<'static, ()> + Send + Sync>,
    cancel: watch::Receiver<bool>,
}

impl JobCtx {
    /// Publish a progress string (the legacy status). Awaits the store, so the order is the call order.
    pub async fn progress(&self, p: Progress) {
        (self.sink)(p.as_str().to_string()).await;
    }
    /// Resolves when the job must stop: its lease was lost, or the drain deadline passed.
    pub async fn cancelled(&self) {
        let mut rx = self.cancel.clone();
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
    pub fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }
}

pub trait Handler: Send + Sync + 'static {
    fn run(&self, ctx: JobCtx) -> impl Future<Output = Result<String, JobError>> + Send;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ended {
    Finished,
    LeaseLost,
    Drained,
}

pub struct Runner<S: JobStore, H: Handler> {
    store: Arc<S>,
    handler: Arc<H>,
    cfg: RunnerConfig,
    tag: AtomicU64,
    running: Arc<AtomicUsize>,
}

impl<S: JobStore, H: Handler> Runner<S, H> {
    pub fn new(store: Arc<S>, handler: Arc<H>, cfg: RunnerConfig) -> Arc<Runner<S, H>> {
        Arc::new(Runner { store, handler, cfg, tag: AtomicU64::new(0), running: Arc::new(AtomicUsize::new(0)) })
    }

    pub fn running_jobs(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    /// Run until `shutdown` turns true, then drain.
    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut background = JoinSet::new();
        {
            let me = self.clone();
            let mut sd = shutdown.clone();
            background.spawn(async move { me.periodic(&mut sd, true).await });
            let me = self.clone();
            let mut sd = shutdown.clone();
            background.spawn(async move { me.periodic(&mut sd, false).await });
        }
        let sem = Arc::new(Semaphore::new(self.cfg.concurrency));
        let mut jobs: JoinSet<()> = JoinSet::new();
        let mut store_failures: u32 = 0;
        info!(worker = %self.cfg.worker_id, concurrency = self.cfg.concurrency, "runner started");
        'claim: loop {
            if *shutdown.borrow() {
                break;
            }
            let permit = tokio::select! {
                p = sem.clone().acquire_owned() => match p { Ok(p) => p, Err(_) => break },
                _ = shutdown.changed() => continue 'claim,
            };
            while jobs.try_join_next().is_some() {}
            let tag = self.tag.fetch_add(1, Ordering::SeqCst);
            let next = tokio::select! {
                n = self.store.next(&self.cfg.worker_id, tag, self.cfg.claim_block, self.cfg.lease_ttl) => n,
                _ = shutdown.changed() => { drop(permit); continue 'claim },
            };
            match next {
                Ok(Some(claim)) => {
                    store_failures = 0;
                    let me = self.clone();
                    let cancel = cancel_rx.clone();
                    self.running.fetch_add(1, Ordering::SeqCst);
                    jobs.spawn(async move {
                        me.run_job(claim, cancel).await;
                        me.running.fetch_sub(1, Ordering::SeqCst);
                        drop(permit);
                    });
                }
                Ok(None) => {
                    store_failures = 0;
                    drop(permit);
                }
                Err(e) => {
                    drop(permit);
                    store_failures = store_failures.saturating_add(1);
                    let wait = self.cfg.store_backoff.delay(store_failures);
                    warn!(error = %e, wait_ms = wait.as_millis() as u64, "store unavailable while claiming; will retry");
                    tokio::select! { _ = tokio::time::sleep(wait) => {}, _ = shutdown.changed() => {} }
                }
            }
        }
        // Drain: running jobs finish within the deadline, or they are cancelled and handed back.
        info!(worker = %self.cfg.worker_id, in_flight = self.running_jobs(), deadline_secs = self.cfg.drain_deadline.as_secs(), "draining");
        let drained = tokio::time::timeout(self.cfg.drain_deadline, async { while jobs.join_next().await.is_some() {} }).await;
        if drained.is_err() {
            warn!(in_flight = self.running_jobs(), "drain deadline passed; cancelling and releasing the remaining jobs");
            let _ = cancel_tx.send(true);
            let _ = tokio::time::timeout(Duration::from_secs(30), async { while jobs.join_next().await.is_some() {} }).await;
        }
        background.abort_all();
        info!(worker = %self.cfg.worker_id, "runner stopped");
    }

    /// The promoter (`promote == true`) and the reaper loops.
    async fn periodic(&self, shutdown: &mut watch::Receiver<bool>, promote: bool) {
        let every = if promote { self.cfg.promote_every } else { self.cfg.reap_every };
        loop {
            tokio::select! { _ = tokio::time::sleep(every) => {}, _ = shutdown.changed() => {} }
            if *shutdown.borrow() {
                return;
            }
            if promote {
                match self.store.promote_due(100).await {
                    Ok(n) if n > 0 => info!(promoted = n, "retries due, queued again"),
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "promote_due failed"),
                }
            } else {
                match self.store.reap_stalled(&self.cfg.worker_id, self.cfg.lease_ttl, 100).await {
                    Ok(n) if n > 0 => warn!(recovered = n, "stalled jobs (worker lost) queued again"),
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "reap_stalled failed"),
                }
            }
        }
    }

    async fn run_job(self: &Arc<Self>, claim: Claim, drain_cancel: watch::Receiver<bool>) {
        let lease = claim.lease.clone();
        let task_id = claim.task_id.clone();
        info!(request_id = %claim.request_id, task_id = %task_id, attempt = claim.attempt, max = self.cfg.max_attempts, kind = %claim.kind, worker = %self.cfg.worker_id, "attempt started");
        metrics::incr(&*self.store, "doc_job_attempts_total", &[]).await;
        if claim.attempt > self.cfg.max_attempts.saturating_add(2) {
            // Delivered over and over without ever finishing: it takes workers down. Stop it.
            let f = JobFailure { code: "worker_lost_repeatedly".into(), message: format!("the job was started {} times and never finished", claim.attempt), retryable: false };
            error!(request_id = %claim.request_id, task_id = %task_id, attempt = claim.attempt, "poison job; failing it");
            metrics::incr(&*self.store, "doc_jobs_total", &[("outcome", "failed")]).await;
            metrics::incr(&*self.store, "doc_job_failures_total", &[("code", "worker_lost_repeatedly"), ("kind", "permanent")]).await;
            self.store_retrying(|| async { self.store.fail(&lease, &f, self.cfg.result_ttl).await }).await;
            return;
        }
        // One cancel signal for this job: its own lease loss, or the drain.
        let (job_cancel_tx, job_cancel_rx) = watch::channel(false);
        let lost = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hb = {
            let store = self.store.clone();
            let lease = lease.clone();
            let ttl = self.cfg.lease_ttl;
            let lost = lost.clone();
            let tx = job_cancel_tx.clone();
            tokio::spawn(async move {
                let every = (ttl / 3).max(Duration::from_millis(50));
                loop {
                    tokio::time::sleep(every).await;
                    match store.heartbeat(&lease, ttl).await {
                        Ok(true) => {}
                        Ok(false) => {
                            lost.store(true, Ordering::SeqCst);
                            let _ = tx.send(true);
                            return;
                        }
                        Err(e) => warn!(error = %e, "heartbeat failed (store down?); the lease may lapse and be re-taken when it returns"),
                    }
                }
            })
        };
        let fwd = {
            let mut dc = drain_cancel.clone();
            let tx = job_cancel_tx.clone();
            tokio::spawn(async move {
                while !*dc.borrow() {
                    if dc.changed().await.is_err() {
                        return;
                    }
                }
                let _ = tx.send(true);
            })
        };
        let sink: Arc<dyn Fn(String) -> BoxFuture<'static, ()> + Send + Sync> = {
            let store = self.store.clone();
            let lease = lease.clone();
            let backoff = self.cfg.store_backoff;
            Arc::new(move |p: String| {
                let store = store.clone();
                let lease = lease.clone();
                Box::pin(async move {
                    for n in 1..=12u32 {
                        match store.set_progress(&lease, &p).await {
                            Ok(_) => return,
                            Err(e) => {
                                warn!(error = %e, progress = %p, "could not publish progress; retrying");
                                tokio::time::sleep(backoff.delay(n)).await;
                            }
                        }
                    }
                })
            })
        };
        let ctx = JobCtx { task_id: task_id.clone(), attempt: claim.attempt, kind: claim.kind.clone(), payload: claim.payload.clone(), request_id: claim.request_id.clone(), max_attempts: self.cfg.max_attempts, sink, cancel: job_cancel_rx.clone() };
        let handler = self.handler.clone();
        let started = std::time::Instant::now();
        let mut work = tokio::spawn(async move { handler.run(ctx).await });
        let outcome: (Ended, Option<Result<String, JobError>>) = tokio::select! {
            r = &mut work => {
                match r {
                    Ok(res) => (Ended::Finished, Some(res)),
                    Err(join) if join.is_panic() => (Ended::Finished, Some(Err(JobError::transient("internal_panic", "the job handler panicked")))),
                    Err(_) => (Ended::Finished, Some(Err(JobError::transient("internal_cancelled", "the job handler was cancelled")))),
                }
            }
            _ = tokio::time::sleep(self.cfg.job_ceiling) => {
                work.abort();
                let _ = job_cancel_tx.send(true);
                (Ended::Finished, Some(Err(JobError::transient("job_ceiling", format!("the attempt exceeded {} s", self.cfg.job_ceiling.as_secs())))))
            }
            _ = wait_true(job_cancel_rx.clone()) => {
                // Give the handler a moment to unwind (kill its child), then abort what remains.
                if tokio::time::timeout(Duration::from_secs(10), &mut work).await.is_err() { work.abort(); }
                if lost.load(Ordering::SeqCst) { (Ended::LeaseLost, None) } else { (Ended::Drained, None) }
            }
        };
        hb.abort();
        fwd.abort();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match outcome {
            (Ended::LeaseLost, _) => {
                warn!(request_id = %claim.request_id, task_id = %task_id, attempt = claim.attempt, elapsed_ms, "lease lost: another worker owns this job now; abandoning without writing");
            }
            (Ended::Drained, _) => {
                warn!(request_id = %claim.request_id, task_id = %task_id, elapsed_ms, "drain deadline: handing the job back to the queue");
                self.store_retrying(|| async { self.store.release(&lease).await }).await;
            }
            (Ended::Finished, Some(Ok(result))) => {
                let ok = self.store_retrying(|| async { self.store.complete(&lease, &result, self.cfg.result_ttl).await }).await;
                info!(request_id = %claim.request_id, task_id = %task_id, attempt = claim.attempt, elapsed_ms, recorded = ok, "attempt succeeded; job done");
                if ok {
                    metrics::incr(&*self.store, "doc_jobs_total", &[("outcome", "succeeded")]).await;
                }
            }
            (Ended::Finished, Some(Err(err))) => self.settle_error(&claim, &lease, err, elapsed_ms).await,
            (Ended::Finished, None) => {}
        }
    }

    async fn settle_error(&self, claim: &Claim, lease: &Lease, err: JobError, elapsed_ms: u64) {
        let failure = err.failure();
        let exhausted = claim.attempt >= self.cfg.max_attempts;
        metrics::incr(&*self.store, "doc_job_failures_total", &[("code", &failure.code), ("kind", if failure.retryable { "transient" } else { "permanent" })]).await;
        if err.retryable() && !exhausted {
            let delay = self.cfg.backoff.delay(claim.attempt);
            warn!(request_id = %claim.request_id, task_id = %claim.task_id, attempt = claim.attempt, max = self.cfg.max_attempts, code = %failure.code, error = %failure.message, retry_in_ms = delay.as_millis() as u64, elapsed_ms, "attempt failed (transient); will retry");
            self.store_retrying(|| async { self.store.retry_later(lease, delay, &failure).await }).await;
        } else {
            error!(request_id = %claim.request_id, task_id = %claim.task_id, attempt = claim.attempt, code = %failure.code, error = %failure.message, retryable = failure.retryable, exhausted, elapsed_ms, "attempt failed; job failed");
            metrics::incr(&*self.store, "doc_jobs_total", &[("outcome", "failed")]).await;
            self.store_retrying(|| async { self.store.fail(lease, &failure, self.cfg.result_ttl).await }).await;
        }
    }

    /// A state change must not be lost to a store outage: keep trying, with backoff, for up to ~10 minutes.
    async fn store_retrying<F, Fut>(&self, mut f: F) -> bool
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<bool, crate::store::StoreError>>,
    {
        for n in 1..=200u32 {
            match f().await {
                Ok(b) => return b,
                Err(e) => {
                    warn!(error = %e, attempt = n, "store unavailable while recording a job outcome; retrying");
                    tokio::time::sleep(self.cfg.store_backoff.delay(n)).await;
                }
            }
        }
        error!("gave up recording a job outcome after 200 attempts; the lease will expire and the job will be re-run");
        false
    }
}

async fn wait_true(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}
