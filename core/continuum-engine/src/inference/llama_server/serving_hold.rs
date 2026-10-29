//! Why serving leaves this node's engine alone: live work is bound to it (card a18452d6).
//!
//! On enrico, 2026-10-27, Delia's gene training held the node's only engine for hours.
//! `serving/status` read `ready: false, degraded_reason: null`, and the hourly health
//! verdict read "empty", so a deliberate hold looked like a dead node: two consecutive
//! empty-node receipts were raised against it. The hold was decided from the authoritative
//! occupancy and only probed. Now it is published: what holds the engine, on which
//! incarnation, and since when.
//!
//! A hold is not a degradation, so it is never folded into `degraded_reason`: one field,
//! one meaning.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::inference::engine_residency::Occupancy;

/// Serving is held: no swap, re-home, page-in or retirement of the engine until it is
/// released.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingHold.ts")]
pub struct ServingHold {
    /// When this hold was first observed (epoch ms). Kept across every later observation of
    /// the same hold, so its age is how long the node has been held.
    #[ts(type = "number")]
    pub since_ms: u64,
    pub by: HoldCause,
}

/// What holds the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/serving/HoldCause.ts")]
pub enum HoldCause {
    /// Live work bound to the engine's incarnation.
    Resident { work: Vec<HeldWork> },
    /// The residency record cannot be read, so ownership is unknown and serving holds.
    Unknown { reason: String },
}

/// One piece of live work holding the engine, by the ids the residency record holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/serving/HeldWork.ts")]
pub struct HeldWork {
    /// The job (`JobHandle::local_id`).
    #[ts(type = "string")]
    pub job: Uuid,
    /// The governed reservation's consumer, e.g. `genome-train:<job>`.
    pub consumer: String,
    pub base_model: String,
    /// The engine incarnation the work is bound to.
    pub engine_pid: u32,
    #[ts(type = "number")]
    pub engine_started_s: u64,
    pub port: u16,
}

impl ServingHold {
    /// PURE: the hold `occupancy` puts on serving at `now_ms`. `None` when the engine is free.
    /// The same hold as `previous` (the same work on the same incarnations, or the same unknown
    /// reason) keeps `previous`'s `since_ms`; any other is a new hold from `now_ms`. Identity is
    /// the residency record's ids (job, engine pid and start time), never a label.
    pub fn of(occupancy: &Occupancy, previous: Option<&ServingHold>, now_ms: u64) -> Option<ServingHold> {
        let by = match occupancy {
            Occupancy::Free => return None,
            Occupancy::Resident(work) => {
                let mut work: Vec<HeldWork> = work
                    .iter()
                    .map(|w| HeldWork {
                        job: w.job,
                        consumer: w.consumer.clone(),
                        base_model: w.base_model.clone(),
                        engine_pid: w.engine.pid,
                        engine_started_s: w.engine.started_s,
                        port: w.engine.port,
                    })
                    .collect();
                // The record's order is not the hold's identity.
                work.sort_by(|a, b| (a.job, a.engine_pid, a.engine_started_s).cmp(&(b.job, b.engine_pid, b.engine_started_s)));
                HoldCause::Resident { work }
            }
            Occupancy::Unknown(reason) => HoldCause::Unknown { reason: reason.clone() },
        };
        let since_ms = previous.filter(|held| held.by == by).map_or(now_ms, |held| held.since_ms);
        Some(ServingHold { since_ms, by })
    }

    /// The live work a handoff of this node would end, named by its ids (card 9beab2da: the
    /// windows 5090's deploy killed in-engine job 8b46862c at 41% while this hold named it).
    /// `None` when no work is named: no work, or a hold of unknown cause, which names none and is
    /// not a confirmed absence either (the deploy guard reads the cause itself: `observation_of`).
    pub fn live_work(&self) -> Option<String> {
        match &self.by {
            HoldCause::Resident { work } if !work.is_empty() => Some(
                work.iter()
                    .map(|w| format!("{} on {} (engine pid {}, port {})", w.consumer, w.base_model, w.engine_pid, w.port))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::engine_residency::{EngineIncarnation, ResidentWork};

    fn work(job: u128, pid: u32, started_s: u64) -> ResidentWork {
        ResidentWork {
            job: Uuid::from_u128(job),
            out: format!("{job}.gguf"),
            engine: EngineIncarnation { pid, started_s, port: 58_057 },
            base_model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
            created_ms: 1_793_120_435_262,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 13_130_575_872,
            interrupted: None,
            job_spec: None,
            memory: None,
        }
    }

    // what this catches (card a18452d6): a held node published as merely empty, a hold whose
    // age restarts on every observation, and a hold that outlives its release.
    #[test]
    fn a_hold_keeps_its_start_while_it_lasts_and_is_gone_on_release() {
        let held = Occupancy::Resident(vec![work(1, 33_971, 1_793_118_524)]);
        let first = ServingHold::of(&held, None, 1_000).expect("resident work holds serving");
        assert_eq!(first.since_ms, 1_000);
        match &first.by {
            HoldCause::Resident { work } => {
                assert_eq!((work[0].job, work[0].engine_pid, work[0].port), (Uuid::from_u128(1), 33_971, 58_057));
            }
            other => panic!("resident work is the cause, got {other:?}"),
        }
        let later = ServingHold::of(&held, Some(&first), 61_000).expect("still held");
        assert_eq!(later.since_ms, 1_000, "the same hold keeps its start");
        assert_eq!(ServingHold::of(&Occupancy::Free, Some(&later), 121_000), None, "a released engine is not held");
    }

    // what this catches (Codex on card a18452d6): a hold carried over to a different engine or
    // job because it looked alike. A changed incarnation (same job, same pid, a new start time,
    // as after pid reuse) or a different job is a new hold from now.
    #[test]
    fn a_changed_incarnation_or_job_is_a_new_hold() {
        let first = ServingHold::of(&Occupancy::Resident(vec![work(1, 33_971, 1_793_118_524)]), None, 1_000).expect("held");
        let relaunched = Occupancy::Resident(vec![work(1, 33_971, 1_793_200_000)]);
        assert_eq!(ServingHold::of(&relaunched, Some(&first), 5_000).map(|h| h.since_ms), Some(5_000));
        let other_job = Occupancy::Resident(vec![work(2, 33_971, 1_793_118_524)]);
        assert_eq!(ServingHold::of(&other_job, Some(&first), 6_000).map(|h| h.since_ms), Some(6_000));
        let unknown = ServingHold::of(&Occupancy::Unknown("record unreadable".into()), Some(&first), 7_000).expect("unknown holds");
        assert_eq!((unknown.since_ms, unknown.by), (7_000, HoldCause::Unknown { reason: "record unreadable".into() }));
    }

    // what this catches: two pieces of work recorded in a different order read as a new hold.
    #[test]
    fn the_records_order_is_not_a_new_hold() {
        let a = work(1, 33_971, 1_793_118_524);
        let b = work(2, 33_971, 1_793_118_524);
        let first = ServingHold::of(&Occupancy::Resident(vec![a.clone(), b.clone()]), None, 1_000).expect("held");
        let swapped = ServingHold::of(&Occupancy::Resident(vec![b, a]), Some(&first), 2_000).expect("held");
        assert_eq!(swapped.since_ms, 1_000);
    }

    // what this catches (card 9beab2da): a deploy that reads the node's hold must stop for live
    // work and only for it. Resident work is named by its consumer and engine; an unknown hold
    // names nothing, so it never blocks the deploy that may repair it.
    #[test]
    fn only_resident_work_is_live_work_a_handoff_would_end() {
        let held = ServingHold::of(&Occupancy::Resident(vec![work(0x8b46, 33_971, 1_793_118_524)]), None, 1_000).expect("held");
        let named = held.live_work().expect("resident work is live work");
        assert!(named.contains(&format!("genome-train:{}", 0x8b46)) && named.contains("engine pid 33971"), "{named}");
        let unknown = ServingHold { since_ms: 1_000, by: HoldCause::Unknown { reason: "the residency record is unreadable".into() } };
        assert_eq!(unknown.live_work(), None);
        assert_eq!(ServingHold { since_ms: 1_000, by: HoldCause::Resident { work: Vec::new() } }.live_work(), None, "no work, nothing to end");
    }
}
