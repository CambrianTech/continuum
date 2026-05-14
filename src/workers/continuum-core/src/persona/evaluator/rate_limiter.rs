//! Per-persona rate limiter with per-room tracking.
//!
//! Mirrors the TypeScript `RateLimiter`. Tracks per-room response cadence
//! so a persona can be told "you replied recently" — used as a SIGNAL into
//! `full_evaluate`'s social-signals payload, not a hard gate on local
//! models (cloud rate limits belong at the provider layer).
//!
//! Extracted from `evaluator.rs` (continuum#1208) — independent of the
//! gate pipeline, reusable wherever per-room turn cadence matters.

use std::collections::HashMap;
use uuid::Uuid;

/// Per-room rate limiting state.
#[derive(Debug, Clone)]
pub struct RoomRateState {
    pub last_response_time_ms: u64,
    pub response_count: u32,
}

/// Per-persona rate limiter with per-room tracking.
#[derive(Debug, Clone)]
pub struct RateLimiterState {
    pub rooms: HashMap<Uuid, RoomRateState>,
    pub min_seconds_between_responses: f64,
    pub max_responses_per_session: u32,
}

impl Default for RateLimiterState {
    fn default() -> Self {
        Self {
            rooms: HashMap::new(),
            min_seconds_between_responses: 10.0,
            max_responses_per_session: 50,
        }
    }
}

impl RateLimiterState {
    pub fn new(min_seconds: f64, max_responses: u32) -> Self {
        Self {
            rooms: HashMap::new(),
            min_seconds_between_responses: min_seconds,
            max_responses_per_session: max_responses,
        }
    }

    /// Check if response cap reached for a room.
    pub fn has_reached_response_cap(&self, room_id: Uuid) -> bool {
        self.rooms
            .get(&room_id)
            .map(|r| r.response_count >= self.max_responses_per_session)
            .unwrap_or(false)
    }

    /// Check if rate limited for a room (time-based).
    pub fn is_rate_limited(&self, room_id: Uuid, now_ms: u64) -> bool {
        self.rooms
            .get(&room_id)
            .map(|r| {
                let elapsed_seconds = (now_ms - r.last_response_time_ms) as f64 / 1000.0;
                elapsed_seconds < self.min_seconds_between_responses
            })
            .unwrap_or(false)
    }

    /// Get seconds until rate limit expires. None if not limited.
    pub fn rate_limit_wait_seconds(&self, room_id: Uuid, now_ms: u64) -> Option<f64> {
        self.rooms.get(&room_id).and_then(|r| {
            let elapsed = (now_ms - r.last_response_time_ms) as f64 / 1000.0;
            if elapsed < self.min_seconds_between_responses {
                Some(self.min_seconds_between_responses - elapsed)
            } else {
                None
            }
        })
    }

    /// Track a response in a room.
    pub fn track_response(&mut self, room_id: Uuid, now_ms: u64) {
        let entry = self.rooms.entry(room_id).or_insert(RoomRateState {
            last_response_time_ms: 0,
            response_count: 0,
        });
        entry.last_response_time_ms = now_ms;
        entry.response_count += 1;
    }

    /// Get response count for a room.
    pub fn response_count(&self, room_id: Uuid) -> u32 {
        self.rooms
            .get(&room_id)
            .map(|r| r.response_count)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: regression where `track_response` stops
    /// incrementing the per-room counter (e.g. assigns to a fresh
    /// entry on every call instead of incrementing the existing one).
    #[test]
    fn track_response_increments_per_room_count() {
        let mut limiter = RateLimiterState::default();
        let room_id = Uuid::new_v4();

        limiter.track_response(room_id, 1000);
        limiter.track_response(room_id, 2000);
        limiter.track_response(room_id, 3000);

        assert_eq!(limiter.response_count(room_id), 3);
    }

    /// What this catches: regression where the rate limit window is
    /// computed in the wrong unit (seconds vs ms) or where elapsed-time
    /// comparison flips its inequality direction. After the configured
    /// window has passed, `is_rate_limited` MUST return false.
    #[test]
    fn rate_limit_expires_after_min_seconds() {
        let mut limiter = RateLimiterState::new(10.0, 50);
        let room_id = Uuid::new_v4();
        limiter.track_response(room_id, 1000);

        // 5 seconds later — still limited.
        assert!(limiter.is_rate_limited(room_id, 6_000));
        // 11 seconds later — limit cleared.
        assert!(!limiter.is_rate_limited(room_id, 12_000));
    }
}
