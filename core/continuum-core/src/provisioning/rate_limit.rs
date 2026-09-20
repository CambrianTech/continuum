//! HONOR THE HOST'S OWN RATE-LIMIT HEADERS — a fetch reads what the server tells it and
//! waits exactly as long as it is told.
//!
//! Joel, 2026-09-17: "They send headers for what's going on actually. Be sure to read
//! them. They tell you how much time remains or you can detect and prevent spamming them
//! and being labeled an attacker. They give these for your own good."
//!
//! HuggingFace (and any well-behaved host) answers a 429 with `Retry-After` (a seconds
//! count or an HTTP-date) and, on normal responses, rate-limit headers
//! (`x-ratelimit-remaining` / `x-ratelimit-reset`). A blind retry loop that ignores them —
//! the [`swe_bench`](crate::cognition::swe_bench) 5s→300s doubling backoff is one, and
//! this primitive is meant to replace it — is what earns a 429, then a ban. This type
//! reads the headers into a `wait until` instant and a remaining-quota gate, so the
//! caller never requests inside a window the server asked it to sit out and never spends
//! the last of its quota. It is the [[the-grid-governor-negotiates-like-wifi-lambda-cost-backoff-and-airc-self-heals]]
//! law applied to HF: the host's own signal drives the backoff, not a guess.
//!
//! Pure: [`RateLimit::from_headers`] takes borrowed header VALUES (not a `reqwest` type)
//! so the whole parse + decision is assertable with no network, no client, no clock but
//! the one the caller passes.

/// A stop-before-you-get-banned budget, parsed from one response's headers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimit {
    /// Unix ms before which the caller MUST NOT request again (from `Retry-After` on a
    /// 429, or a `reset` once `remaining` is 0). `None` = no wait imposed by this response.
    pub wait_until_ms: Option<u64>,
    /// Requests left in the current window, if the host reported it.
    pub remaining: Option<u64>,
}

/// The absolute floor a `Retry-After` in seconds is clamped to (0) and the ceiling we
/// will ever honor from a single header — a hostile or buggy header must not park a
/// fetch for a week. A day is far past any real HF cooldown.
pub const MAX_HONORED_WAIT_MS: u64 = 24 * 3600 * 1000;

impl RateLimit {
    /// Parse the rate-limit signal from one response. `status` is the HTTP status;
    /// `retry_after` is the `Retry-After` header value (seconds, or an HTTP-date);
    /// `remaining` / `reset` are `x-ratelimit-remaining` / `x-ratelimit-reset` (reset is
    /// a unix-seconds epoch on HF). `now_ms` and `http_date_to_ms` let the caller supply
    /// the clock and the (platform) date parser, keeping this pure.
    pub fn from_headers(
        status: u16,
        retry_after: Option<&str>,
        remaining: Option<&str>,
        reset_epoch_secs: Option<&str>,
        now_ms: u64,
        http_date_to_ms: impl Fn(&str) -> Option<u64>,
    ) -> Self {
        let remaining = remaining.and_then(|v| v.trim().parse::<u64>().ok());
        let reset_ms = reset_epoch_secs
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|secs| secs.saturating_mul(1000));

        // A 429 (or any explicit Retry-After) is the hard signal: wait exactly that long.
        let retry_after_ms = if status == 429 || retry_after.is_some() {
            retry_after.and_then(|v| {
                let v = v.trim();
                if let Ok(secs) = v.parse::<u64>() {
                    Some(now_ms.saturating_add(secs.saturating_mul(1000)))
                } else {
                    http_date_to_ms(v) // an HTTP-date form of Retry-After
                }
            })
        } else {
            None
        };

        // The soft signal: quota exhausted → sit out until the window resets.
        let quota_ms = match (remaining, reset_ms) {
            (Some(0), Some(reset)) => Some(reset),
            _ => None,
        };

        // Take the LATER of the two (both are "not before"), then clamp so a bad header
        // can never park a fetch past the ceiling.
        let wait_until_ms = [retry_after_ms, quota_ms]
            .into_iter()
            .flatten()
            .max()
            .map(|w| w.min(now_ms.saturating_add(MAX_HONORED_WAIT_MS)));

        Self { wait_until_ms, remaining }
    }

    /// May the caller make a request at `now_ms`? False while inside an imposed wait.
    pub fn may_request(&self, now_ms: u64) -> bool {
        match self.wait_until_ms {
            Some(until) => now_ms >= until,
            None => true,
        }
    }

    /// Ms the caller must still wait at `now_ms` (0 if clear) — for a probe / a sleep.
    pub fn remaining_wait_ms(&self, now_ms: u64) -> u64 {
        self.wait_until_ms.map(|u| u.saturating_sub(now_ms)).unwrap_or(0)
    }
}

/// THE ONE ADAPTER: a `reqwest` response → [`RateLimit`], reading the de-facto standard
/// headers every well-behaved host shares (`Retry-After`, `x-ratelimit-remaining`,
/// `x-ratelimit-reset`) — HuggingFace AND GitHub both speak them, so one adapter serves
/// both; a host with quirk headers overrides only the mapping, never the honor logic.
/// This is the single place `reqwest` header types touch the pure core (Joel 2026-09-17:
/// "build this kind of logic in one pattern or place ... adapters get it for all").
impl RateLimit {
    pub fn from_response(status: u16, headers: &reqwest::header::HeaderMap, now_ms: u64) -> Self {
        let h = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        Self::from_headers(
            status,
            h("retry-after"),
            h("x-ratelimit-remaining"),
            h("x-ratelimit-reset"),
            now_ms,
            |v| {
                // Retry-After's HTTP-date form is RFC 1123 ("Wed, 21 Oct 2026 07:28:00
                // GMT"), which chrono parses as RFC 2822. A negative epoch clamps to 0.
                chrono::DateTime::parse_from_rfc2822(v)
                    .ok()
                    .map(|d| d.timestamp_millis().max(0) as u64)
            },
        )
    }
}

/// The ONE stateful gate every external client threads through: before each request it
/// asks [`wait_ms`](Self::wait_ms) and sleeps that long (0 = go); after each response it
/// [`observe`](Self::observe)s the headers so the NEXT request honors what the host just
/// said. Replaces the per-caller blind backoffs (the swe_bench 5s→300s doubling loop, the
/// model fetch's none) with one host-driven budget.
#[derive(Debug, Default)]
pub struct RateLimitTracker {
    current: RateLimit,
}

impl RateLimitTracker {
    /// Ms to sleep before the next request (0 = clear to go now).
    pub fn wait_ms(&self, now_ms: u64) -> u64 {
        self.current.remaining_wait_ms(now_ms)
    }

    /// Update the budget from a response's status + headers.
    pub fn observe(&mut self, status: u16, headers: &reqwest::header::HeaderMap, now_ms: u64) {
        self.current = RateLimit::from_response(status, headers, now_ms);
    }

    /// The requests the host says are left in this window, if it reported any.
    pub fn remaining(&self) -> Option<u64> {
        self.current.remaining
    }

    /// Record a wait the caller already parsed (e.g. a `CatalogError::RateLimited { wait_ms }`
    /// carried up from a fetch) without re-reading headers: sit out until `now + wait_ms`.
    pub fn observe_wait_ms(&mut self, wait_ms: u64, now_ms: u64) {
        self.current = RateLimit {
            wait_until_ms: (wait_ms > 0).then(|| now_ms.saturating_add(wait_ms)),
            remaining: self.current.remaining,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000_000_000;
    // A test HTTP-date parser: only the one date the test uses, proving the seam without
    // pulling a date crate into a pure test.
    fn date_parser(v: &str) -> Option<u64> {
        (v == "Wed, 21 Oct 2026 07:28:00 GMT").then_some(NOW + 120_000)
    }

    // what this catches (Joel 2026-09-17, "be sure to read them"): a 429's Retry-After is
    // honored to the second, an HTTP-date Retry-After is parsed through the caller's seam,
    // an exhausted quota waits for its reset, and a normal response imposes no wait.
    #[test]
    fn the_host_headers_drive_the_backoff() {
        // 429 + Retry-After: 30 s → wait 30 s, may_request false until then.
        let r = RateLimit::from_headers(429, Some("30"), None, None, NOW, date_parser);
        assert_eq!(r.wait_until_ms, Some(NOW + 30_000));
        assert!(!r.may_request(NOW));
        assert!(!r.may_request(NOW + 29_999));
        assert!(r.may_request(NOW + 30_000));
        assert_eq!(r.remaining_wait_ms(NOW), 30_000);

        // Retry-After as an HTTP-date goes through the caller's parser seam.
        let r = RateLimit::from_headers(429, Some("Wed, 21 Oct 2026 07:28:00 GMT"), None, None, NOW, date_parser);
        assert_eq!(r.wait_until_ms, Some(NOW + 120_000));

        // Quota exhausted (remaining 0) waits until reset even on a 200.
        let reset = ((NOW / 1000) + 60).to_string();
        let r = RateLimit::from_headers(200, None, Some("0"), Some(&reset), NOW, date_parser);
        assert_eq!(r.wait_until_ms, Some((NOW / 1000 + 60) * 1000));
        assert_eq!(r.remaining, Some(0));

        // Healthy response: quota left, no wait.
        let r = RateLimit::from_headers(200, None, Some("4998"), Some(&reset), NOW, date_parser);
        assert_eq!(r.wait_until_ms, None);
        assert!(r.may_request(NOW));
        assert_eq!(r.remaining, Some(4998));
    }

    // what this catches: a hostile / buggy header can never park a fetch past the ceiling,
    // and the LATER of Retry-After and a quota reset is the one honored.
    // what this catches: the reqwest adapter reads the standard headers HF and gh share,
    // and the tracker turns a 429 response into a wait the next request honors.
    #[test]
    fn the_reqwest_adapter_and_tracker_honor_a_429() {
        use reqwest::header::HeaderMap;
        let mut h = HeaderMap::new();
        h.insert("retry-after", "45".parse().unwrap());
        let mut t = RateLimitTracker::default();
        assert_eq!(t.wait_ms(NOW), 0, "clear before any response");
        t.observe(429, &h, NOW);
        assert_eq!(t.wait_ms(NOW), 45_000, "the host's Retry-After drives the sleep");
        assert_eq!(t.wait_ms(NOW + 45_000), 0, "clear once the window passes");
        // A healthy response with quota clears the wait.
        let mut ok = HeaderMap::new();
        ok.insert("x-ratelimit-remaining", "4999".parse().unwrap());
        t.observe(200, &ok, NOW + 45_000);
        assert_eq!(t.wait_ms(NOW + 45_000), 0);
        assert_eq!(t.remaining(), Some(4999));
    }

    #[test]
    fn a_bad_header_is_clamped_and_the_later_signal_wins() {
        // A week-long Retry-After is clamped to the 24 h ceiling.
        let week = (7 * 24 * 3600).to_string();
        let r = RateLimit::from_headers(429, Some(&week), None, None, NOW, date_parser);
        assert_eq!(r.wait_until_ms, Some(NOW + MAX_HONORED_WAIT_MS));
        // Retry-After 10 s AND remaining 0 with a reset 60 s out → honor the later (60 s).
        let reset = ((NOW / 1000) + 60).to_string();
        let r = RateLimit::from_headers(429, Some("10"), Some("0"), Some(&reset), NOW, date_parser);
        assert_eq!(r.wait_until_ms, Some((NOW / 1000 + 60) * 1000));
        // Nothing rate-limiting at all: clear.
        assert!(RateLimit::from_headers(200, None, None, None, NOW, date_parser).may_request(NOW));
    }
}
