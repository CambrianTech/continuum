//! Pool discovery: which `/props` verdicts may latch the directory (moved with the discovery from
//! the OpenAI adapter).

use crate::inference::slots::discover::props_status_proves_endpoint_absent;

// what this catches: the boot-race latch that killed slot affinity on effectively
// EVERY boot (2026-08-21). Personas start deliberating while llama-server still
// answers 503_loading (measured ×93 in the same ledger); the old code latched
// Unsupported on ANY non-success status, so one probe racing the load window
// disabled pinning for the process's life and prefix-similarity slot theft took
// over (`cached: 0` mid-conversation). Only "no such endpoint" may be permanent.
#[test]
fn a_loading_lane_must_not_latch_slot_affinity_off() {
    use reqwest::StatusCode;
    for transient in [
        StatusCode::SERVICE_UNAVAILABLE, // llama-server mid-load — THE incident
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        assert!(
            !props_status_proves_endpoint_absent(transient),
            "{transient} is a statement about NOW, not about what the server IS — \
             latching Unsupported on it re-opens the boot race"
        );
    }
    for absent in [StatusCode::NOT_FOUND, StatusCode::NOT_IMPLEMENTED] {
        assert!(
            props_status_proves_endpoint_absent(absent),
            "{absent} genuinely proves the surface is missing — without the latch \
             every cloud provider would be re-probed per persona request forever"
        );
    }
}
