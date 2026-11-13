//! Whether a core-only handoff leaves the engine lanes it keeps running (card 51d6b4a6).
//!
//! The handoff stops the core alone and keeps the live lane (`stop_with`'s keep set), and the next
//! core adopts that engine and re-attaches to the in-engine work bound to it. Whether the engine
//! actually outlives the old core is the platform's: what its supervisor does to the processes a
//! stopped core leaves behind.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod os;

/// True only where a core-only handoff is known to leave the kept engine lanes running.
pub fn keeps_engine_lanes() -> bool {
    os::KEEPS_ENGINE_LANES
}
