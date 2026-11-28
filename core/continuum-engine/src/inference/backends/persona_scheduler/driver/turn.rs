//! A turn in flight and where its time went.

use super::*;

/// A turn in flight: what she asked, its token ids, and how far it has come. The frame loop
/// advances it a step at a time; between steps another mind may be bound, and her blocks hold
/// exactly what her turn has computed so far.
pub(super) struct Turn {
    pub(super) req: GenerationRequest,
    pub(super) id: PersonaId,
    pub(super) one_off: bool,
    pub(super) prompt: std::sync::Arc<[i32]>,
    pub(super) basis: Basis,
    pub(super) genes: Genes,
    pub(super) phase: Phase,
    pub(super) submitted: Instant,
    pub(super) ready_since: Instant,
    pub(super) spent: Spent,
    /// Where her view ends in `prompt` (CONTINUUM-ENGINE 4.15): a prefill chunk ends exactly there
    /// and she is checkpointed, so her next act resumes from it. `None` for a turn with no view.
    pub(super) boundaries: Boundaries,
    /// Her answer, as Emit numbers it.
    pub(super) answer: emit::AnswerSeq,
    /// Raised by Emit when her answer has ended on a stop sequence or her caller hung up; read at
    /// each step without a lock.
    pub(super) cancel: Arc<AtomicBool>,
}

/// Where a turn stands.
pub(super) enum Phase {
    /// Not bound yet: what her prefill starts from is decided against her blocks when she is.
    Waiting,
    /// Her prompt from `pos` is still to prefill.
    Prefilling { pos: usize },
    /// `token` is sampled and not yet emitted; it is decoded at `pos` if she goes on.
    Decoding { token: i32, pos: Pos, generated: usize },
}

/// Where a turn's time went, for its one probe row.
#[derive(Default)]
pub(super) struct Spent {
    /// Her once-only transitions: first frame started, first native progress.
    pub(super) milestones: transition::Milestones,
    pub(super) reused: usize,
    pub(super) shared_prefix_tokens: usize,
    /// How her prefill started: append, rewind, or the restart's reason.
    pub(super) start: &'static str,
    pub(super) checkpoint_ms: u64,
    pub(super) first_step: Option<std::time::Duration>,
    pub(super) longest_gap: std::time::Duration,
    pub(super) switch: std::time::Duration,
    pub(super) prefill: std::time::Duration,
    pub(super) decode: std::time::Duration,
    /// Of `decode`: the forward pass to its landed result (decode and sync), and the sampler. What
    /// is left of `decode` is the frame's own work around them (CONTINUUM-ENGINE 4.16: the M5 at
    /// 7f67dc28d0 decoded ~10 ms/token slower than llama-bench on the same GGUF and KV type).
    pub(super) forward: std::time::Duration,
    /// Of `decode`: waiting for the forward pass to land on the GPU.
    pub(super) gpu: std::time::Duration,
    pub(super) sample: std::time::Duration,
    /// Of her decoded tokens, how many reused a built graph instead of rebuilding it on the CPU.
    pub(super) graphs_reused: u64,
    pub(super) switches: u64,
}

/// What a step did to its turn.
pub(super) enum Advanced {
    Continues,
    /// She answered: what her `TokenEvent::Done` will say.
    Done { tokens_generated: usize, end: super::super::TurnEnd },
}
