// Nonce replay cache — prevents replay attacks
use crate::fault::SoapFault;
use std::collections::HashSet;
use std::time::Instant;

/// Two-bucket rotating nonce cache for WS-Security replay detection.
///
/// Buckets rotate every `half_window` seconds (default: 150 s → 300 s total window).
/// A nonce present in **either** bucket is considered a replay and rejected.
///
/// # Thread-safety
///
/// `check_and_insert` takes `&mut self`, so `RotatingNonceCache` itself is **not**
/// thread-safe. Wrap it in a `tokio::sync::Mutex` (or `std::sync::Mutex` for sync
/// contexts) before sharing across async tasks:
///
/// ```rust,ignore
/// use tokio::sync::Mutex;
/// use soap_server::RotatingNonceCache;
///
/// let cache = Arc::new(Mutex::new(RotatingNonceCache::new(150)));
/// // Inside an async handler:
/// let mut cache = cache.lock().await;
/// cache.check_and_insert(&nonce)?;
/// ```
///
/// The soap-server [`SoapService`] handles this internally — consumers using
/// [`validate_username_token`] via the server builder do not need to manage the
/// cache directly. Consider interior-mutability refactoring in v0.2.
pub struct RotatingNonceCache {
    current: HashSet<String>,
    previous: HashSet<String>,
    bucket_start: Instant,
    half_window_secs: u64,
}

impl RotatingNonceCache {
    pub fn new(half_window_secs: u64) -> Self {
        Self {
            current: HashSet::new(),
            previous: HashSet::new(),
            bucket_start: Instant::now(),
            half_window_secs,
        }
    }

    /// Check nonce for replay and insert if not seen. Returns Err on replay.
    pub fn check_and_insert(&mut self, nonce: &str) -> Result<(), SoapFault> {
        self.rotate_if_needed();
        if self.current.contains(nonce) || self.previous.contains(nonce) {
            return Err(SoapFault::sender("WS-Security nonce replay detected"));
        }
        self.current.insert(nonce.to_string());
        Ok(())
    }

    fn rotate_if_needed(&mut self) {
        // Use a while loop so that 2+ elapsed windows fully evict stale nonces.
        // Without this loop, a server idle for 2× half_window would retain the
        // previous bucket, causing legitimate clients to get spurious replay faults.
        while self.bucket_start.elapsed().as_secs() >= self.half_window_secs {
            self.previous = std::mem::take(&mut self.current);
            self.bucket_start += std::time::Duration::from_secs(self.half_window_secs);
        }
    }

    /// For testing: force a bucket rotation by resetting the bucket_start to the past.
    #[cfg(test)]
    pub fn force_rotate(&mut self) {
        self.previous = std::mem::take(&mut self.current);
        self.bucket_start = Instant::now();
    }

    /// For testing: rewind bucket_start by `secs` seconds to simulate elapsed time.
    #[cfg(test)]
    pub fn rewind_bucket_start(&mut self, secs: u64) {
        self.bucket_start -= std::time::Duration::from_secs(secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_cache_accepts_new_nonce() {
        let mut cache = RotatingNonceCache::new(150);
        assert!(cache.check_and_insert("abc").is_ok());
    }

    #[test]
    fn same_nonce_rejected_on_second_call() {
        let mut cache = RotatingNonceCache::new(150);
        cache.check_and_insert("abc").unwrap();
        let result = cache.check_and_insert("abc");
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(fault.reason.contains("replay"), "got: {}", fault.reason);
    }

    #[test]
    fn different_nonce_accepted() {
        let mut cache = RotatingNonceCache::new(150);
        cache.check_and_insert("abc").unwrap();
        assert!(cache.check_and_insert("xyz").is_ok());
    }

    #[test]
    fn nonce_in_previous_bucket_still_detected_as_replay() {
        let mut cache = RotatingNonceCache::new(150);
        // Insert nonce into current bucket
        cache.check_and_insert("abc").unwrap();
        // Force rotation — "abc" moves to previous bucket
        cache.force_rotate();
        // Should still be rejected because it's in previous
        let result = cache.check_and_insert("abc");
        assert!(result.is_err(), "Expected replay error after rotation");
    }

    #[test]
    fn nonce_dropped_after_two_rotations() {
        let mut cache = RotatingNonceCache::new(150);
        // Insert nonce
        cache.check_and_insert("abc").unwrap();
        // First rotation — "abc" moves to previous
        cache.force_rotate();
        // Second rotation — "abc" is dropped (previous is replaced)
        cache.force_rotate();
        // Now "abc" should be accepted again
        assert!(
            cache.check_and_insert("abc").is_ok(),
            "Expected nonce to be accepted after two rotations"
        );
    }

    /// Regression test for BLOCK-SS-C02: idle-gap bug.
    /// If the server is idle for 2+ half_window periods, rotate_if_needed() must
    /// rotate enough times so that stale nonces are fully evicted.
    /// Without the while-loop fix, this test would fail with a spurious replay fault.
    #[test]
    fn idle_gap_regression_nonce_accepted_after_two_window_gap() {
        let half_window = 5u64;
        let mut cache = RotatingNonceCache::new(half_window);
        // Insert nonce into current bucket.
        cache.check_and_insert("gap_nonce").unwrap();
        // Simulate server being idle for 2× half_window + 1 second.
        cache.rewind_bucket_start(half_window * 2 + 1);
        // The nonce was inserted >2 windows ago — it must be fully evicted.
        // Without the while-loop fix, only one rotation would happen, leaving
        // "gap_nonce" in `previous` and causing a spurious replay rejection.
        assert!(
            cache.check_and_insert("gap_nonce").is_ok(),
            "Nonce must be accepted after a 2× half_window idle gap (BLOCK-SS-C02 regression)"
        );
    }
}
