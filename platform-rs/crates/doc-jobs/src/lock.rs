//! The clone lock shared by every worker process: `SET`-style lock with a lease, renewal and a fencing token
//! (an `INCR` counter per key). `RedisLock` is the production one, `MemLock` the single-process one.
//! Both implement [`doc_ports::RepoLock`], so the acquirer does not know which it has.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use doc_ports::{LockFuture, LockGuard, RepoLock};
use redis::aio::ConnectionManager;
use redis::Script;

pub struct RedisLock {
    ns: String,
    conn: ConnectionManager,
    script: Script,
    lease: Duration,
    owner: String,
}

impl RedisLock {
    /// `owner` should be unique per process (worker id and pid); the token adds a random part per grant.
    pub fn new(conn: ConnectionManager, ns: &str, owner: &str, lease: Duration) -> RedisLock {
        RedisLock { ns: ns.to_string(), conn, script: Script::new(include_str!("../lua/lock.lua")), lease, owner: owner.to_string() }
    }

    fn lock_key(&self, key: &str) -> String {
        format!("{}:lock:{key}", self.ns)
    }
    fn fence_key(&self, key: &str) -> String {
        format!("{}:fence:{key}", self.ns)
    }

    async fn call(&self, mode: &str, key: &str, token: &str) -> Result<(i64, i64), String> {
        let mut c = self.conn.clone();
        let ttl = u64::try_from(self.lease.as_millis()).unwrap_or(u64::MAX);
        let r: Vec<i64> = self
            .script
            .key(self.lock_key(key))
            .key(self.fence_key(key))
            .arg(mode)
            .arg(token)
            .arg(ttl)
            .invoke_async::<redis::Value>(&mut c)
            .await
            .map_err(|e| e.to_string())
            .and_then(|v| redis::from_redis_value::<Vec<i64>>(v.clone()).or_else(|_| redis::from_redis_value::<i64>(v).map(|n| vec![n, 0])).map_err(|e| e.to_string()))?;
        Ok((r.first().copied().unwrap_or(0), r.get(1).copied().unwrap_or(0)))
    }
}

fn token_for(owner: &str) -> String {
    let n: u64 = rand::random();
    format!("{owner}~{n:016x}")
}

impl RepoLock for RedisLock {
    fn acquire<'a>(&'a self, key: &'a str, wait: Duration) -> LockFuture<'a, Result<Option<LockGuard>, String>> {
        Box::pin(async move {
            let token = token_for(&self.owner);
            let deadline = Instant::now() + wait;
            let mut n = 0u32;
            loop {
                let (granted, fence) = self.call("acquire", key, &token).await?;
                if granted == 1 {
                    return Ok(Some(LockGuard { key: key.to_string(), token, fence: u64::try_from(fence).unwrap_or(0) }));
                }
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                n += 1;
                let pause = Duration::from_millis(100 * u64::from(n.min(10)));
                tokio::time::sleep(pause.min(deadline.saturating_duration_since(Instant::now()))).await;
            }
        })
    }

    fn renew<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool> {
        Box::pin(async move { matches!(self.call("renew", &guard.key, &guard.token).await, Ok((1, _))) })
    }

    fn release<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, ()> {
        Box::pin(async move {
            let _ = self.call("release", &guard.key, &guard.token).await;
        })
    }

    fn holds<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool> {
        // a lock backend that cannot answer is not evidence of loss: the swap is atomic anyway
        Box::pin(async move { !matches!(self.call("holds", &guard.key, &guard.token).await, Ok((0, _))) })
    }
}

/// The same contract inside one process (tests, the Redis-less mode).
#[derive(Default)]
pub struct MemLock {
    held: Mutex<HashMap<String, (String, u64, Instant)>>,
    fence: Mutex<HashMap<String, u64>>,
    lease: Duration,
}

impl MemLock {
    pub fn new(lease: Duration) -> Arc<MemLock> {
        Arc::new(MemLock { held: Mutex::default(), fence: Mutex::default(), lease })
    }
}

impl RepoLock for MemLock {
    fn acquire<'a>(&'a self, key: &'a str, wait: Duration) -> LockFuture<'a, Result<Option<LockGuard>, String>> {
        Box::pin(async move {
            let token = token_for("mem");
            let deadline = Instant::now() + wait;
            loop {
                {
                    let mut held = self.held.lock().map_err(|e| e.to_string())?;
                    let free = held.get(key).is_none_or(|(_, _, until)| Instant::now() >= *until);
                    if free {
                        let mut f = self.fence.lock().map_err(|e| e.to_string())?;
                        let n = f.entry(key.to_string()).or_insert(0);
                        *n += 1;
                        held.insert(key.to_string(), (token.clone(), *n, Instant::now() + self.lease));
                        return Ok(Some(LockGuard { key: key.to_string(), token, fence: *n }));
                    }
                }
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
    }
    fn renew<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool> {
        Box::pin(async move {
            let Ok(mut held) = self.held.lock() else { return false };
            match held.get_mut(&guard.key) {
                Some((t, _, until)) if *t == guard.token => {
                    *until = Instant::now() + self.lease;
                    true
                }
                _ => false,
            }
        })
    }
    fn release<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, ()> {
        Box::pin(async move {
            if let Ok(mut held) = self.held.lock() {
                if held.get(&guard.key).is_some_and(|(t, _, _)| *t == guard.token) {
                    held.remove(&guard.key);
                }
            }
        })
    }
    fn holds<'a>(&'a self, guard: &'a LockGuard) -> LockFuture<'a, bool> {
        Box::pin(async move { self.held.lock().map(|h| h.get(&guard.key).is_some_and(|(t, _, until)| *t == guard.token && Instant::now() < *until)).unwrap_or(true) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mem_lock_excludes_fences_and_ignores_a_stale_release() {
        let l = MemLock::new(Duration::from_millis(150));
        let a = l.acquire("k", Duration::from_millis(10)).await.ok().flatten();
        let Some(a) = a else { panic!("first acquire must be granted") };
        assert!(l.acquire("k", Duration::from_millis(30)).await.ok().flatten().is_none(), "held: second must wait out");
        tokio::time::sleep(Duration::from_millis(200)).await; // lease expires without renewal
        let Some(b) = l.acquire("k", Duration::from_millis(10)).await.ok().flatten() else { panic!("expired lease must be re-grantable") };
        assert!(b.fence > a.fence, "fence grows");
        assert!(!l.holds(&a).await, "the old holder has lost it");
        l.release(&a).await; // a stale release must not remove the new holder's lock
        assert!(l.holds(&b).await, "stale release left the new holder alone");
    }
}
