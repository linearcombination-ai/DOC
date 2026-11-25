//! Backoff with full jitter (AWS Architecture Blog, "Exponential Backoff And Jitter"): the wait before
//! attempt `n + 1` is uniform in `[0, min(cap, base * 2^n)]`, with a floor so a retry is never a spin.

use std::time::Duration;

use rand::RngExt;

#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    pub base: Duration,
    pub cap: Duration,
    pub floor: Duration,
}

impl Backoff {
    /// The upper bound of the jitter window after `attempt` failed attempts (1-based).
    pub fn ceiling(&self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(20);
        let scaled = self.base.saturating_mul(1u32 << shift);
        scaled.min(self.cap)
    }

    pub fn delay(&self, attempt: u32) -> Duration {
        let hi = self.ceiling(attempt).max(self.floor);
        let lo = self.floor.min(hi);
        if hi <= lo {
            return hi;
        }
        let ms = rand::rng().random_range(lo.as_millis() as u64..=hi.as_millis() as u64);
        Duration::from_millis(ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_doubles_then_caps() {
        let b = Backoff { base: Duration::from_secs(2), cap: Duration::from_secs(60), floor: Duration::from_millis(100) };
        let c: Vec<u64> = (1..=8).map(|a| b.ceiling(a).as_secs()).collect();
        assert_eq!(c, vec![2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn delay_stays_in_window_and_varies() {
        let b = Backoff { base: Duration::from_secs(2), cap: Duration::from_secs(60), floor: Duration::from_millis(100) };
        let ds: Vec<Duration> = (0..200).map(|_| b.delay(3)).collect();
        assert!(ds.iter().all(|d| *d >= b.floor && *d <= Duration::from_secs(8)));
        let distinct: std::collections::BTreeSet<u128> = ds.iter().map(|d| d.as_millis()).collect();
        assert!(distinct.len() > 50, "jitter must spread the waits, got {} distinct", distinct.len());
    }
}
