//! The doubling backoff, one owner: every retry that waits longer after each failure in a row
//! (a failing store, a panicked loop's restart, a refused seat) computes its wait here, and only
//! its floor and cap, named and justified where it is used, differ.

use std::time::Duration;

/// The wait after the `streak`th failure in a row: `floor`, then twice that, and so on, never
/// past `cap`. A streak of 0 or 1 waits `floor`.
pub fn doubling_wait(streak: u32, floor: Duration, cap: Duration) -> Duration {
    let doublings = streak.saturating_sub(1).min(u32::BITS - 1);
    floor.saturating_mul(1u32 << doublings).min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a backoff that overflows on a long streak or passes its cap.
    #[test]
    fn it_doubles_from_the_floor_and_never_passes_the_cap() {
        let (floor, cap) = (Duration::from_secs(2), Duration::from_secs(600));
        let waits: Vec<u64> = [0, 1, 2, 3, 9, 10, u32::MAX].iter().map(|s| doubling_wait(*s, floor, cap).as_secs()).collect();
        assert_eq!(waits, vec![2, 2, 4, 8, 512, 600, 600]);
    }
}
