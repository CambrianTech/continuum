//! The dyno (ONE-ACT, "Proof: the dyno", slice 1): a seconds-scale validator built from the REAL
//! policy code, driven by what the M5 RECORDED on 2026-11-04, with no engine, no residents, any
//! machine. Its first job is to reproduce each failure of that day as a failing assertion of the
//! design's invariant; each fixing slice turns its reproduction green.
//!
//! - [`recorded`]: the ONE fixture module: the typed `Recorded` over the day's data file
//!   (`fixtures/dyno/2026-11-04.json`, every measured value beside its receipt) and the engine's refusal wording.
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
//! | 6 | an admission hold with no releaser | removed with its cause: the pool-growth relaunch and its quiesce are deleted (it froze the M5 for 2.5 h on 2026-11-10) | none: no hold exists |
//! | 7 | an overflowing view | not drivable here yet: [`expected::MissingSeam::ViewTurnoverPolicy`] | `dyno/tests.rs` `view` |
//! | 8 | idle pool, one mind, censored demand: her grant follows a demand her collapses measured | `LaneRequirement` -> `window_grant::Demand::of` -> `DemandFollowing`, her wish ring as the working set keeps it | `dyno/tests.rs` `censored_demand` |
//! | 9 | boot during memory settle: a boot that remembered no resident launched at a reading taken while the stopped engine's memory came back (-c 2045, the M5 2026-11-13), and the grown plan was never applied | the boot's remembered residents -> `serving_daemon::launch::launch_pool`, `settle::launch_gate`, `floor_sized::floor_sized` | `continuum-serving` `serving_daemon/tests/dyno.rs` `boot_during_memory_settle` |
//! | 10 | a leased-in mind sized from a prompt size, not her requirement | not drivable yet: [`expected::MissingSeam::LeasedInRequirement`] (the grid seam) | as 9 |
//! | 11 | a restart re-prefills every mind (the M5 2026-11-13 23:45Z: the ledger restored nothing, every first act a full prefill), and the page store gave paged-out minds' pages to a volume other classes filled (29 in two hours) | the page store's `ColdPages::under` at the recorded pressure; save and restore costs from the engine's measured rates | `dyno/restart.rs` |
| 11b | a page switch holds the engine's one task loop for its disk IO: every slot's decode stalls for it and switches issued together wait for each other (the M5 2026-11-14 00:18:13Z: Benchy's restore waited 1,024 ms behind Aris's) | the engine's measured save/restore rates and the M5's host copy rate, the switch on the loop vs streamed (copy on the loop, IO on a worker) | `dyno/restart/stall.rs` |

pub mod conservation;
pub mod expected;
pub mod recorded;

mod batch;
mod knee;
mod restart;
mod service;
#[cfg(test)]
mod tests;
