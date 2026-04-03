// Nonce replay cache — prevents replay attacks
use std::collections::HashSet;
use std::time::Instant;
use crate::fault::SoapFault;

/// Two-bucket rotating nonce cache.
/// Buckets rotate every `half_window` seconds (default: 150s for a 300s total window).
/// A nonce present in EITHER bucket is considered replayed.
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
        if self.bucket_start.elapsed().as_secs() >= self.half_window_secs {
            self.previous = std::mem::take(&mut self.current);
            self.bucket_start = Instant::now();
        }
    }

    /// For testing: force a bucket rotation by resetting the bucket_start to the past.
    #[cfg(test)]
    pub fn force_rotate(&mut self) {
        self.previous = std::mem::take(&mut self.current);
        self.bucket_start = Instant::now();
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
        assert!(cache.check_and_insert("abc").is_ok(), "Expected nonce to be accepted after two rotations");
    }
}
