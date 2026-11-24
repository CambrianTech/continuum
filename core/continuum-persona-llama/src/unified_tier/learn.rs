//! The tier's learning in place (IN-PROCESS-LEARNING.md, L2): the bound mind learns a reply into a
//! shadow on her own stream, through the llama crate's [`Learner`]. Between chunks it asks the caller
//! whether to go on; a no drains the reply, which restores her served tail and keeps what committed.

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Execute, Learns, Native, TierRefusal};
use llama::{Finished, Label, Learner, OptimizerLease, Progress, Reply, ReplyAdmission, Shadow};

use super::{refused, UnifiedTier, TIER};

/// One reply for her stream: the positions she holds before it, its tokens and their labels, and
/// the exact tail she held past `n_cached`, restored afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnReply {
    pub n_cached: i32,
    pub tokens: Vec<i32>,
    /// One per token: the target after it.
    pub labels: Vec<Label>,
    pub restore: Vec<i32>,
}

impl Learns for UnifiedTier<'_, '_> {
    type Shadow = Shadow;
    type Lease = Box<dyn OptimizerLease>;
    type Reply = LearnReply;
    type Admission = Box<dyn ReplyAdmission>;
    type Learned = Finished;

    fn retain(&mut self, shadow: &Shadow, lease: Box<dyn OptimizerLease>) -> Result<(), Box<dyn OptimizerLease>> {
        Learner::new(self.ctx).retain_shadow(std::sync::Arc::clone(&shadow.adapter), lease)
    }

    fn release(&mut self, mind: MindId, shadow: &Shadow) -> Result<Box<dyn OptimizerLease>, TierRefusal> {
        Learner::new(self.ctx)
            .release_shadow(&shadow.adapter)
            .map_err(|refusal| TierRefusal { tier: TIER, mind, reason: "the engine kept her shadow's optimizer", native: Native::Reported(refusal.to_string()) })
    }

    fn learn(
        &mut self,
        mind: MindId,
        shadow: &Shadow,
        reply: &LearnReply,
        admission: Box<dyn ReplyAdmission>,
        go_on: &mut dyn FnMut() -> bool,
    ) -> Result<Finished, (TierRefusal, Box<dyn ReplyAdmission>)> {
        let Some(stream) = self.stream_of(mind) else {
            return Err((refused(mind, "not bound"), admission));
        };
        if let Err(unsettled) = self.settle() {
            return Err((unsettled, admission));
        }
        // The optimizer selects her shadow on the context: the next decode applies her genes again.
        self.applied_genes = None;
        let native = Reply { seq: stream.seq(), n_cached: reply.n_cached, tokens: &reply.tokens, labels: &reply.labels, restore: &reply.restore };
        let mut learner = Learner::new(self.ctx);
        let mut run = learner.begin(shadow, admission, &native).map_err(|refused| {
            (TierRefusal { tier: TIER, mind, reason: "the reply did not begin", native: Native::Reported(refused.refusal.to_string()) }, refused.admission)
        })?;
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
