//! Marks committed before an effect reaches the world (stop-drain S2, card 68811445).
//!
//! A turn the stop cuts may be re-admitted at the next start (S3). That is safe only if nothing
//! it did had already reached the world, so every effect is marked durably BEFORE it happens, at
//! the two places effects start: the tool batch's dispatch (`apply_act`, foreground and
//! background alike) and her one reply post (`publish_own_speech`). The live typing preview is
//! visible but is not a durable post, so it is not a boundary.
//!
//! The mark rides the turn: `Burst::effects` carries it in, `ActChain` holds it for the turn's
//! acts, and both are dropped when the turn settles, so a mark can never be aimed at another
//! turn's obligation. A mark that cannot be confirmed refuses the effect (fail closed): an
//! unconfirmed mark is never permission to dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

/// What is about to reach the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A tool batch, as (call id, tool name) pairs, so recovery can find each call's receipt.
    ToolBatch { calls: Vec<(String, String)> },
    /// Her reply, posted to the room.
    Speech,
}

/// Records an effect durably before it happens. `Ok` only once the record is confirmed.
#[async_trait]
pub trait EffectMark: Send + Sync {
    /// The turn this mark records for, so a mark aimed at another turn is detectable.
    fn turn(&self) -> Uuid;
    async fn before(&self, effect: &Effect) -> Result<(), String>;
}

/// A turn's mark, carried by value through its burst and act chain.
#[derive(Clone)]
pub struct EffectGate(pub Arc<dyn EffectMark>);

impl std::fmt::Debug for EffectGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EffectGate(turn {})", self.0.turn())
    }
}

impl EffectGate {
    /// Record `effect` before it happens. A turn without a gate (a self-tick, an eval fork) has
    /// no obligation to keep, so it passes: nothing could ever re-admit it.
    pub async fn before(gate: Option<&EffectGate>, effect: &Effect) -> Result<(), String> {
        match gate {
            Some(gate) => gate.0.before(effect).await,
            None => Ok(()),
        }
    }
}
