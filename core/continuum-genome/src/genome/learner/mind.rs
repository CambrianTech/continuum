//! One mind learning in place on one engine: her seated shadow, her learning memory, and her grant
//! (card 4114a2c6).
//!
//! Start is seat (measure, lease, mirror, load), then her learning memory (preflight, lease, mirror),
//! then her grant, uncontended-only until a measurement sets a share. Each step learns one example
//! she lived, on her own slot, admitted by that grant inside those residencies. Stop is the reverse:
//! revoke the grant, unload the shadow (the engine frees her optimizer and, for the last mind, its
//! workspace), release her residencies, and the leases drop. No separate trainer, window or reserve.

use super::grant::{self, Grant};
use super::memory::{self, EngineWorkspace, MindLearning};
use super::seat::{self, Authority, Seat, SeatRefusal};
use crate::inference::llama_server::{FreshShadow, GrantShare, LearnExample, LearnOutcome, LearningEngine, LlamaServerError};

/// A mind learning in place.
pub struct MindLearner {
    seat: Seat,
    memory: MindLearning,
    grant: Grant,
}

impl MindLearner {
    /// Seat her shadow, lease her learning memory (and the engine's workspace when it holds none that
    /// covers her), and issue her uncontended-only grant expiring after `grant_expiry` unless revoked.
    pub async fn start(
        authority: &Authority,
        engine: LearningEngine,
        consumer: &str,
        shadow: FreshShadow,
        workspace: &mut Option<EngineWorkspace>,
        grant_expiry: std::time::Duration,
    ) -> Result<Self, SeatRefusal> {
        let seat = seat::seat(authority, engine, consumer, shadow).await?;
        let memory = memory::prepare(authority, &seat, consumer, workspace).await?;
        let grant = grant::issue(&seat, GrantShare::UncontendedOnly, grant_expiry).await?;
        Ok(Self { seat, memory, grant })
    }

    /// Learn one lived example (the core's wire framing of the served conversation, its last message
    /// the reply) on her `slot` at `learning_rate`.
    pub async fn learn(
        &self,
        workspace: &EngineWorkspace,
        messages: Vec<serde_json::Value>,
        tools: Option<serde_json::Value>,
        slot: u32,
        learning_rate: f32,
    ) -> Result<LearnOutcome, LlamaServerError> {
        let example = LearnExample {
            messages,
            tools,
            slot,
            lora_id: self.seat.shadow.id,
            grant: self.grant.id,
            optimizer_residency: self.memory.optimizer,
            workspace_residency: workspace.residency,
            learning_rate,
        };
        self.seat.engine.learn(&example).await
    }

    /// Her shadow's adapter id in the engine.
    pub fn shadow(&self) -> i64 {
        self.seat.shadow.id
    }

    /// Stop, in the reverse of start: revoke the grant, unload the shadow (the engine frees her
    /// optimizer and, for the last mind, its workspace), release her optimizer residency, then the
    /// weight residency; each part's leases drop as it is released.
    pub async fn stop(self) -> Result<(), String> {
        grant::revoke(&self.seat, &self.grant).await.map_err(|why| why.to_string())?;
        self.seat.unload().await?;
        self.memory.release(&self.seat).await.map_err(|(_, why)| why)?;
        self.seat.release().await.map_err(|(_, why)| why)
    }
}
