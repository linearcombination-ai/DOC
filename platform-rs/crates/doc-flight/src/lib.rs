//! Single-flight without locks on the file system (the captain: the file-lock approach did not
//! work well). Concurrent callers with the same key share one execution. The execution is a
//! spawned task: if every waiter times out or disconnects it still finishes, so its result (a
//! clone on disk, a cache entry) is there for the next request (self-heal).

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use futures::future::{BoxFuture, FutureExt, Shared};

/// Whether this caller started the work or joined work in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Leader,
    Follower,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlightError {
    /// The task panicked or was cancelled (runtime shutdown).
    Aborted(String),
}

type Flight<V> = Shared<BoxFuture<'static, Result<V, FlightError>>>;

pub struct SingleFlight<K, V> {
    inflight: Arc<Mutex<HashMap<K, Flight<V>>>>,
}

impl<K, V> Default for SingleFlight<K, V> {
    fn default() -> Self {
        SingleFlight { inflight: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl<K, V> SingleFlight<K, V>
where
    K: Hash + Eq + Clone + Send + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `work` unless an execution for `key` is in flight, and wait for its result. The returned
    /// future can be dropped (a caller's deadline) without stopping the execution.
    pub fn run<F>(&self, key: K, work: F) -> impl Future<Output = (Result<V, FlightError>, Role)> + Send + 'static
    where
        F: Future<Output = V> + Send + 'static,
    {
        let (flight, role) = {
            let mut map = match self.inflight.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            match map.get(&key) {
                Some(existing) => (existing.clone(), Role::Follower),
                None => {
                    let inflight = Arc::clone(&self.inflight);
                    let key_for_task = key.clone();
                    let handle = tokio::spawn(async move {
                        let value = work.await;
                        let mut map = match inflight.lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        map.remove(&key_for_task);
                        value
                    });
                    let shared: Flight<V> = async move {
                        handle.await.map_err(|e| FlightError::Aborted(e.to_string()))
                    }
                    .boxed()
                    .shared();
                    map.insert(key, shared.clone());
                    (shared, Role::Leader)
                }
            }
        };
        async move { (flight.await, role) }
    }

    pub fn in_flight(&self) -> usize {
        match self.inflight.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn concurrent_callers_share_one_execution() {
        let flight: Arc<SingleFlight<&'static str, u32>> = Arc::new(SingleFlight::new());
        let runs = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (f, r) = (flight.clone(), runs.clone());
            tasks.push(tokio::spawn(async move {
                f.run("k", async move {
                    r.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    7
                })
                .await
            }));
        }
        let mut leaders = 0;
        for t in tasks {
            let (res, role) = t.await.unwrap();
            assert_eq!(res, Ok(7));
            if role == Role::Leader {
                leaders += 1;
            }
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(leaders, 1);
        assert_eq!(flight.in_flight(), 0);
    }

    #[tokio::test]
    async fn work_finishes_when_the_only_waiter_gives_up() {
        let flight: SingleFlight<u8, u8> = SingleFlight::new();
        let done = Arc::new(AtomicUsize::new(0));
        let d = done.clone();
        let waiting = flight.run(1, async move {
            tokio::time::sleep(Duration::from_millis(60)).await;
            d.fetch_add(1, Ordering::SeqCst);
            1
        });
        assert!(tokio::time::timeout(Duration::from_millis(10), waiting).await.is_err());
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(done.load(Ordering::SeqCst), 1);
        assert_eq!(flight.in_flight(), 0);
    }
}
