//! The tier's learning in place (IN-PROCESS-LEARNING.md, L2): the bound mind learns a reply into a
//! shadow on her own stream, through the llama crate's [`Learner`]. Between chunks it asks the caller
//! whether to go on; a no drains the reply, which restores her served tail and keeps what committed.
//!
//! Native code trains (and measures) only a shadow attached at a non-zero scale (`opt_untrainable`).
//! Attaching and detaching are the caller's: it holds the shadow until a detach is confirmed, so the
//! context never references a freed adapter (Codex on L2). The shadow is attached alone: it learns
//! against the base weights, never another mind's genes left applied.

use std::num::NonZeroU32;
use std::path::Path;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Execute, Learns, Native, TierRefusal};
use llama::{Finished, Label, Learner, MemoryRow, OptimizerLease, Progress, Reply, ReplyAdmission, Shadow};

use super::{refused, UnifiedTier, TIER};

/// One reply for her stream: the positions she holds before it, its tokens and their labels, and
/// the exact tail she held past `n_cached`, restored afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnReply {
    /// Whose reply it is: learned on the stream she is bound to.
    pub mind: MindId,
    pub n_cached: i32,
    pub tokens: Vec<i32>,
    /// One per token: the target after it.
    pub labels: Vec<Label>,
    pub restore: Vec<i32>,
}

fn reported(mind: MindId, reason: &'static str, refusal: impl std::fmt::Display) -> TierRefusal {
    TierRefusal { tier: TIER, mind, reason, native: Native::Reported(refusal.to_string()) }
}

impl Learns for UnifiedTier<'_, '_> {
    type Shadow = Shadow;
    type Lease = Box<dyn OptimizerLease>;
    type Reply = LearnReply;
    type Admission = Box<dyn ReplyAdmission>;
    type Learned = Finished;
    type Row = MemoryRow;

    fn attach(&mut self, mind: MindId, shadow: &Shadow) -> Result<(), TierRefusal> {
        self.settle()?;
        // Whatever the context held is replaced: her next decode applies her genes again.
        self.applied_genes = None;
        self.ctx.set_loras(&[(shadow.adapter.as_ref(), 1.0)]).map_err(|message| reported(mind, "the engine refused the shadow", message))
    }

    fn detach(&mut self, mind: MindId) -> Result<(), TierRefusal> {
        self.settle()?;
        self.applied_genes = None;
        self.ctx.clear_loras().map_err(|message| reported(mind, "the engine could not take the shadow off", message))
    }

    fn measure(&mut self, mind: MindId, shadow: &Shadow, reply_rows: NonZeroU32) -> Result<(Vec<MemoryRow>, Vec<MemoryRow>), TierRefusal> {
        self.settle()?;
        let mut learner = Learner::new(self.ctx);
        let measured = learner.measure(shadow, reply_rows).map_err(|refusal| reported(mind, "the engine could not measure her shadow learning", refusal))?;
        Ok((measured.optimizer, measured.workspace))
    }

    fn save(&mut self, mind: MindId, shadow: &Shadow, into: &Path, binding: &str) -> Result<u64, TierRefusal> {
        self.settle()?;
        Learner::new(self.ctx).save_generation(&shadow.adapter, into, binding).map_err(|refusal| reported(mind, "the engine could not save the generation", refusal))
    }

    fn retain(&mut self, shadow: &Shadow, lease: Box<dyn OptimizerLease>) -> Result<(), Box<dyn OptimizerLease>> {
        Learner::new(self.ctx).retain_shadow(&shadow.adapter, lease)
    }

    fn release(&mut self, mind: MindId, shadow: &Shadow) -> Result<Box<dyn OptimizerLease>, TierRefusal> {
        Learner::new(self.ctx).release_shadow(&shadow.adapter).map_err(|refusal| reported(mind, "the engine kept her shadow's optimizer", refusal))
    }

    fn learn(&mut self, shadow: &Shadow, reply: &LearnReply, admission: Box<dyn ReplyAdmission>, go_on: &mut dyn FnMut() -> bool) -> Result<Finished, (TierRefusal, Box<dyn ReplyAdmission>)> {
        let mind = reply.mind;
        let Some(stream) = self.stream_of(mind) else {
            return Err((refused(mind, "not bound"), admission));
        };
        if let Err(unsettled) = self.settle() {
            return Err((unsettled, admission));
        }
        let native = Reply { seq: stream.seq(), n_cached: reply.n_cached, tokens: &reply.tokens, labels: &reply.labels, restore: &reply.restore };
        let mut learner = Learner::new(self.ctx);
        let mut run = learner.begin(shadow, admission, &native).map_err(|refused| (reported(mind, "the reply did not begin", refused.refusal), refused.admission))?;
        loop {
            if !go_on() {
                run.drain();
            }
            match run.step() {
                Ok(Progress::More) => {}
                // Done, or a refusal: finish says which, and quarantines an unconfirmed restoration.
                Ok(Progress::Done) | Err(_) => break,
            }
        }
        Ok(run.finish())
    }
}
