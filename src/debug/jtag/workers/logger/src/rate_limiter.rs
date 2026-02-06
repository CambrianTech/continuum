/// Rate Limiter Module — Per-category spam control for the logger worker
///
/// Prevents any single category from flooding disk I/O.
/// When a category exceeds its rate limit, messages are dropped
/// and a single summary warning is logged when the burst ends.
///
/// Default: 100 messages/sec per category (configurable per-category).
/// Rate limits reset every second.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per-category rate state
struct CategoryRate {
    /// Messages written in current window
    count: u32,
    /// Messages dropped in current window
    dropped: u32,
    /// Window start time
    window_start: Instant,
    /// Max messages per second for this category (0 = unlimited)
    limit: u32,
}

/// Rate limiter for log categories
pub struct RateLimiter {
    categories: HashMap<String, CategoryRate>,
    default_limit: u32,
    window_duration: Duration,
}

/// Result of checking rate limit
pub enum RateDecision {
    /// Message is allowed
    Allow,
    /// Message is rate-limited (dropped)
    Drop,
    /// Previous burst ended — returns count of dropped messages to log as warning
    BurstEnded(u32),
}

impl RateLimiter {
    /// Create a new rate limiter with the given default limit per second
    pub fn new(default_limit: u32) -> Self {
        Self {
            categories: HashMap::new(),
            default_limit,
            window_duration: Duration::from_secs(1),
        }
    }

    /// Check if a message for the given category should be allowed.
    /// Returns the decision (Allow, Drop, or BurstEnded with dropped count).
    pub fn check(&mut self, category: &str) -> RateDecision {
        let now = Instant::now();
        let default_limit = self.default_limit;
        let window = self.window_duration;

        let state = self.categories.entry(category.to_string()).or_insert_with(|| {
            CategoryRate {
                count: 0,
                dropped: 0,
                window_start: now,
                limit: default_limit,
            }
        });

        // Check if window has elapsed
        if now.duration_since(state.window_start) >= window {
            let prev_dropped = state.dropped;
            state.count = 1; // Count this message
            state.dropped = 0;
            state.window_start = now;

            if prev_dropped > 0 {
                return RateDecision::BurstEnded(prev_dropped);
            }
            return RateDecision::Allow;
        }

        // Unlimited
        if state.limit == 0 {
            state.count += 1;
            return RateDecision::Allow;
        }

        // Within window — check limit
        if state.count < state.limit {
            state.count += 1;
            RateDecision::Allow
        } else {
            state.dropped += 1;
            RateDecision::Drop
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_allows_within_limit() {
        let mut rl = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(matches!(rl.check("test"), RateDecision::Allow));
        }
    }

    #[test]
    fn test_drops_over_limit() {
        let mut rl = RateLimiter::new(3);
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Drop));
        assert!(matches!(rl.check("test"), RateDecision::Drop));
    }

    #[test]
    fn test_window_reset() {
        let mut rl = RateLimiter::new(2);
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Drop));

        // Wait for window to expire
        thread::sleep(Duration::from_millis(1100));

        // Should report burst ended with 1 dropped, then allow
        match rl.check("test") {
            RateDecision::BurstEnded(dropped) => assert_eq!(dropped, 1),
            _ => panic!("Expected BurstEnded"),
        }
    }

    #[test]
    fn test_independent_categories() {
        let mut rl = RateLimiter::new(2);
        assert!(matches!(rl.check("cat_a"), RateDecision::Allow));
        assert!(matches!(rl.check("cat_a"), RateDecision::Allow));
        assert!(matches!(rl.check("cat_a"), RateDecision::Drop));
        // Different category is still allowed
        assert!(matches!(rl.check("cat_b"), RateDecision::Allow));
    }

    #[test]
    fn test_high_limit_category() {
        // With a high limit, many messages pass through
        let mut rl = RateLimiter::new(500);
        for _ in 0..500 {
            assert!(matches!(rl.check("high"), RateDecision::Allow));
        }
        // 501st should be dropped
        assert!(matches!(rl.check("high"), RateDecision::Drop));
    }
}
