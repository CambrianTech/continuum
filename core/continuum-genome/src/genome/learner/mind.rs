//! One mind learning in place on one engine: her seated shadow, her learning memory, and her grant
//! (card 4114a2c6).
//!
//! Start is seat (measure, lease, mirror, load), then her grant, uncontended-only until a measurement
//! sets a share. Each step prices the example she lived (preflight, lease, mirror her optimizer and
//! the engine's workspace, replacing a workspace that no longer covers), then learns it on her own
//! slot, admitted by that grant inside those residencies. Stop is the reverse:
//! revoke the grant, unload the shadow (the engine frees her optimizer and, for the last mind, its
//! workspace), release her residencies, and the leases drop. No separate trainer, window or reserve.

use super::grant::{self, Grant};
use super::memory::{self, EngineWorkspace, MindLearning};
use super::seat::{self, Authority, Seat, SeatRefusal};
use crate::inference::llama_server::{CheckpointAnswer, RestoreCheckpoint, RestoreOutcome, FreshShadow, GrantShare, LearnExample, LearnOutcome, LearningEngine, LlamaServerError};

/// A mind learning in place.
pub struct MindLearner {
    seat: Seat,
    consumer: String,
    /// Whether her shadow is still loaded in the engine (stop unloads it once).
    loaded: bool,
    /// Her optimizer residency, leased by her first example's preflight.
    memory: Option<MindLearning>,
    /// Her grant, until stop revokes it.
    grant: Option<Grant>,
}

/// Why start did not finish. A mind that was seated comes back so the caller stops it: its leases
/// stay held until the engine confirms each release, never dropped with a half-started value.
pub enum StartFailure {
    /// Nothing is held (or what the engine may hold is retained inside the refusal).
    NotSeated(SeatRefusal),
    /// Seated but not granted: stop `seated` to release what it holds.
    Ungranted { why: SeatRefusal, seated: MindLearner },
}

impl std::fmt::Display for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSeated(why) => write!(f, "not seated: {why}"),
            Self::Ungranted { why, .. } => write!(f, "seated but not granted (stop it): {why}"),
        }
    }
}

/// Why a learning step did not reach the engine, or what the engine answered.
#[derive(Debug)]
pub enum LearnFailure {
    /// The example's memory could not be priced, leased or mirrored.
    Uncovered(SeatRefusal),
    /// The engine did not answer the step.
    Engine(LlamaServerError),
    /// A stop already revoked her grant; she learns nothing more until she is started again.
    Stopping,
}

impl std::fmt::Display for LearnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uncovered(why) => write!(f, "the example's memory is not covered: {why}"),
            Self::Engine(error) => write!(f, "the engine did not answer the step: {error}"),
            Self::Stopping => write!(f, "she is stopping: her grant is revoked"),
        }
    }
}

impl MindLearner {
    /// Seat her shadow and issue her uncontended-only grant expiring after `grant_expiry` unless
    /// revoked; her learning memory is priced by her first example.
    pub async fn start(
        authority: &Authority,
        engine: LearningEngine,
        consumer: &str,
        shadow: FreshShadow,
        grant_expiry: std::time::Duration,
    ) -> Result<Self, StartFailure> {
        let seat = seat::seat(authority, engine, consumer, shadow).await.map_err(StartFailure::NotSeated)?;
        let mut mind = Self { seat, consumer: consumer.to_owned(), loaded: true, memory: None, grant: None };
        match grant::issue(&mind.seat, GrantShare::UncontendedOnly, grant_expiry).await {
            Ok(grant) => {
                mind.grant = Some(grant);
                Ok(mind)
            }
            Err(why) => Err(StartFailure::Ungranted { why, seated: mind }),
        }
    }

    /// Learn one lived example (the core's wire framing of the served conversation, its last message
    /// the reply) on her `slot` at `learning_rate`, after pricing it and covering its memory (the
    /// engine's `workspace`, shared by every mind learning on it, is replaced when it does not cover).
    pub async fn learn(
        &mut self,
        authority: &Authority,
        workspace: &mut Option<EngineWorkspace>,
        messages: Vec<serde_json::Value>,
        tools: Option<serde_json::Value>,
        slot: u32,
        learning_rate: f32,
    ) -> Result<LearnOutcome, LearnFailure> {
        let example = self.example(authority, workspace, messages, tools, slot, learning_rate).await?;
        let outcome = self.seat.engine.learn(&example).await.map_err(LearnFailure::Engine);
        settle(workspace).await;
        outcome
    }

    /// Restore her dormant fresh shadow from `checkpoint` (its weights and optimizer state), the
    /// allocation priced and covered by `messages` as a learn would be; nothing is learned.
    pub async fn restore(
        &mut self,
        authority: &Authority,
        workspace: &mut Option<EngineWorkspace>,
        messages: Vec<serde_json::Value>,
        tools: Option<serde_json::Value>,
        slot: u32,
        learning_rate: f32,
        checkpoint: &RestoreCheckpoint,
    ) -> Result<RestoreOutcome, LearnFailure> {
        let example = self.example(authority, workspace, messages, tools, slot, learning_rate).await?;
        let outcome = self.seat.engine.restore(&self.seat.ledger, &example, checkpoint).await.map_err(LearnFailure::Engine);
        settle(workspace).await;
        outcome
    }

    /// The step's example, once its memory is covered.
    async fn example(
        &mut self,
        authority: &Authority,
        workspace: &mut Option<EngineWorkspace>,
        messages: Vec<serde_json::Value>,
        tools: Option<serde_json::Value>,
        slot: u32,
        learning_rate: f32,
    ) -> Result<LearnExample, LearnFailure> {
        let Some(grant) = self.grant.as_ref().map(|grant| grant.id) else {
            return Err(LearnFailure::Stopping);
        };
        let covered = memory::cover(authority, &self.seat, &self.consumer, &messages, tools.as_ref(), &mut self.memory, workspace)
            .await
            .map_err(LearnFailure::Uncovered)?;
        Ok(LearnExample {
            messages,
            tools,
            slot,
            lora_id: self.seat.shadow.id,
            grant,
            optimizer_residency: covered.optimizer,
            workspace_residency: covered.workspace,
            learning_rate,
        })
    }

    /// Publish her shadow's weights and optimizer state as checkpoint generation `<name>.checkpoint`,
    /// bound to `binding`, `provenance` in its manifest. Refused by the engine while she is learning.
    pub async fn checkpoint(&self, name: &str, binding: &str, provenance: serde_json::Value) -> Result<CheckpointAnswer, LlamaServerError> {
        self.seat.engine.save_checkpoint(&self.seat.ledger, self.seat.shadow.id, name, binding, provenance).await
    }

    /// Replace her grant with one of the same share expiring after `expiry` (the new one registered
    /// before the old is revoked), keeping her shadow and its progress: the renewal at a quiescent
    /// boundary that an expired grant asks for (the engine refuses it `unknown_grant`).
    pub async fn renew_grant(&mut self, expiry: std::time::Duration) -> Result<(), SeatRefusal> {
        let Some(old) = self.grant.take() else {
            return Err(SeatRefusal::Unleased("she is stopping: her grant is revoked".into()));
        };
        match grant::replace(&self.seat, &old, old.share, expiry).await {
            Ok(new) => {
                self.grant = Some(new);
                Ok(())
            }
            Err(why) => {
                self.grant = Some(old);
                Err(why)
            }
        }
    }

    /// Her shadow's adapter id in the engine.
    pub fn shadow(&self) -> i64 {
        self.seat.shadow.id
    }

    /// Stop, in the reverse of start: revoke the grant, unload the shadow (the engine frees her
    /// optimizer and, for the last mind, its workspace), release her optimizer residency, then the
    /// weight residency; each part's leases drop only as the engine confirms its release. A step that
    /// fails hands her back with what is still held, so the caller stops her again later.
    pub async fn stop(mut self) -> Result<(), (Self, String)> {
        if let Some(grant) = self.grant.take() {
            if let Err(why) = grant::revoke(&self.seat, &grant).await {
                self.grant = Some(grant);
                return Err((self, why.to_string()));
            }
        }
        if self.loaded {
            if let Err(why) = self.seat.unload().await {
                return Err((self, why));
            }
            self.loaded = false;
        }
        if let Some(memory) = self.memory.take() {
            if let Err((memory, why)) = memory.release().await {
                self.memory = Some(memory);
                return Err((self, why));
            }
        }
        let Self { seat, consumer, loaded, memory, grant } = self;
        seat.release().await.map_err(|(seat, why)| (Self { seat, consumer, loaded, memory, grant }, why))
    }
}

/// The step claimed the covering workspace, so the ones it replaced may be released now.
async fn settle(workspace: &mut Option<EngineWorkspace>) {
    if let Some(held) = workspace.as_mut() {
        held.settle().await;
    }
}
