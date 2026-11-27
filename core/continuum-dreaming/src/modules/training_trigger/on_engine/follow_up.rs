//! What a seat's answer leaves for the trainer's worker (card d1e5675c, Codex's review). Seating a
//! bucket again reads its journal and decides its competence, and offering a committed generation as
//! a candidate snapshots and evicts its files: work of unbounded length, never on the learner's worker,
//! which only takes each seat's next act. The learner records the transition under the bucket's gate,
//! then hands the follow-up to the trainer, which takes the same gate itself.

use std::sync::Arc;

use crate::inference::learning::LearnPort;

use super::super::{BucketKey, TrainingTriggerState};

/// One bucket's work left by an answer, for the trainer's worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FollowUp {
    /// Seat the bucket again: what it waited for is free, or it was let go while its hold was armed.
    Seat(BucketKey),
    /// Offer the bucket's newly committed generation as a candidate.
    Sweep(BucketKey),
}

impl TrainingTriggerState {
    /// Take `follow_up` on `port`; a failure is probed and the next wake tries again.
    pub(crate) async fn follow_up(self: &Arc<Self>, port: &Arc<dyn LearnPort>, follow_up: FollowUp) {
        match follow_up {
            FollowUp::Seat(key) => {
                if let Err(why) = self.seat_bucket(port, &key).await {
                    continuum_base::probe!(class = "training.on_engine.seat_failed", persona = %key.persona_id, reason = %why,
                        "a bucket whose memory was freed was not seated; its examples stay pending and the next wake tries again");
                }
            }
            FollowUp::Sweep(key) => {
                if let Err(why) = self.sweep_bucket(&key, &key.base_model).await {
                    continuum_base::probe!(class = "training.in_place.candidate_sweep_failed", persona = %key.persona_id, reason = %why,
                        "a committed generation's candidate could not be swept now; the next pass takes it");
                }
            }
        }
    }
}
