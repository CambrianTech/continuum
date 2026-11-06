//! The dyno (ONE-ACT, "Proof: the dyno", slice 1): a seconds-scale validator built from the REAL
//! policy code, driven by what the M5 RECORDED on 2026-11-04, with no engine, no residents, any
//! machine. Its first job is to reproduce each failure of that day as a failing assertion of the
//! design's invariant; each fixing slice turns its reproduction green.
//!
//! - [`recorded`]: the ONE fixture module: the typed `Recorded` over the day's data file
//!   (`dyno/2026-11-04.json`, every measured value beside its receipt) and the engine's refusal wording.
//! - [`expected`]: the outcomes the design names that the code does not have yet (the typed
//!   restore outcome of ONE-ACT §5b.1, the seams a reproduction still needs). Test-side
//!   expectations; the fixing slice replaces each with the production type, by rename.
//! - [`conservation`]: physical conservation (weights + resident KV + staging <= device), from
//!   the footprint numbers the planner sizes with. Always on, usable by every scenario.
//!
//! Known-red convention (no repo convention existed): a reproduction of an unfixed failure is
//! `#[ignore = "ONE-ACT slice N: <invariant>"]`, so CI stays green and
//! `cargo test -- --ignored` runs the red set. The slice that fixes it deletes the attribute.
//!
//! Where each reproduction drives its real code (each in its owner's existing test module):
//!
//! | # | Failure | Real code | Test |
//! |---|---|---|---|
//! | 1 | launch sized from a reading with the old engine still wired | `serving_daemon::launch::launch_pool` | `continuum-serving` `serving_daemon/tests/dyno.rs` |
//! | 2 | one censored bound takes the pool | `window_grant::publish_fair_grants` + reservations + handoff receipts, on [`service`]'s virtual clock | `dyno/tests.rs` `grants` |
//! | 3 | a turn completes and restarts between grant ticks | as 2 | `dyno/tests.rs` `grants` |
//! | §4 | feasible service opportunities; late/stale receipts, cancellation per phase, same-URL replacement, all-pinned waits, revocation during quote, seeded schedules | as 2 | `dyno/service/tests.rs` |
//! | §4b/5d | the engine's batch scheduler as a pluggable decision on the M5's recorded hour (2026-11-06) | the fork's `update_slots`, reproduced from source, calibrated | `dyno/batch/tests.rs` |
//! | §5c/5d | the cold-prefill knee and bounded warm decode under mixed load | `resource_admission::LaneAdmission` + the measured service curves | `dyno/knee/tests.rs` |
//! | 4 | a capacity refusal loses the page | `turn_admission::admit_turn` + `kv_page_action` + the persisted ledger, on a fake engine | `inference/turn_admission/tests/restore_refusal.rs` |
//! | 5 | a page larger than the whole cache | as 4; the launch half lacks [`expected::MissingSeam::LaunchKnowsResidentPages`] | as 4, and the serving `dyno.rs` |
//! | §5b.1 | the restore-outcome proof list (restart, reclamation, oversize, incompatible, corrupt, concurrent, late receipt, torn ledger) | as 4 | as 4 |
//! | 6 | an admission hold with no releaser | `llama_server::pool_growth` + `resource_admission` quiesce | `inference/llama_server/pool_growth.rs` tests |
//! | 7 | an overflowing view | not drivable here yet: [`expected::MissingSeam::ViewTurnoverPolicy`] | `dyno/tests.rs` `view` |

pub mod conservation;
pub mod expected;
pub mod recorded;

mod batch;
mod knee;
mod service;
#[cfg(test)]
mod tests;
