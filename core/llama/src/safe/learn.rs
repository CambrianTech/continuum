//! Learning on a live sequence of a serving context (IN-PROCESS-LEARNING.md, L1): the fork's
//! resumable `llama_opt_learn_*`, with no policy. One reply trains on one sequence. Each step runs
//! at most one chunk (a gradient chunk or the stable restoration), so other sequences may decode
//! between steps. Afterwards the reply is restored under her stable gene set, leaving the sequence
//! as serving left it.
//!
//! Ownership, so native code never calls into something gone:
//! - every native callback reaches ONE holder the context owns (native code keeps those pointers
//!   past a reply, so nothing shorter-lived may hold them). A reply places its admission there and
//!   takes it back at a confirmed restoration ([`LearnRun::finish`]) or a refusal; an empty holder
//!   refuses every allocation, so no callback ever reaches freed memory;
//! - the run owns the reply's buffers and the native result for as long as the reply is active;
//! - finishing is explicit and fallible, and a failure is never read as learned. A run whose
//!   restoration was not confirmed blocks its [`Learner`] (keeping a dropped run's admission, its
//!   bytes retained) until [`Learner::fence`] clears the sequence.
//!
//! The shadow adapter must already be attached to the context at a non-zero scale (the caller
//! binds her gene set), with F32 A/B tensors; otherwise selection is refused by name.

use std::ffi::{c_char, c_void, CStr};
use std::num::NonZeroU32;
use std::ptr::NonNull;

use super::*;

/// One allocator a buffer comes from, as the backend names it at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferType(NonNull<sys::ggml_backend_buffer_type>);

impl BufferType {
    /// The backend's own name for this buffer type ("CUDA0", "Metal", "CPU", ...).
    pub fn name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(sys::ggml_backend_buft_name(self.0.as_ptr())) }.to_str()
    }
}

/// What an optimizer-owned buffer holds, as the optimizer names it.
#[derive(Debug, Clone, Copy)]
pub struct OptimizerBuffer<'a>(&'a CStr);

impl OptimizerBuffer<'_> {
    pub fn kind(&self) -> Result<&str, std::str::Utf8Error> {
        self.0.to_str()
    }
}

/// The memory owner's answer for one reply. Native code asks it before it allocates; it must not
/// allocate itself.
pub trait ReplyAdmission: Send {
    /// Before each optimizer-owned buffer is allocated, with its exact aligned size: an increment.
    /// false refuses that allocation, and the step fails by name.
    fn optimizer_buffer(&self, buffer: BufferType, kind: OptimizerBuffer<'_>, bytes: u64) -> bool;
    /// Before the shared workspace grows: checks a lease already held (`held`) against the plan
    /// (`required`). It never takes or consumes a lease, so a repeated measurement is never charged
    /// twice.
    fn workspace(&self, buffer: BufferType, required: u64, held: u64) -> bool;
}

/// The optimizer and its constant parameters for the reply.
#[derive(Debug, Clone, Copy)]
pub enum Optimizer {
    AdamW { learning_rate: f32, beta1: f32, beta2: f32, eps: f32, weight_decay: f32 },
    Sgd { learning_rate: f32, weight_decay: f32 },
}

/// How the backward pass walks the reply's chunks.
#[derive(Debug, Clone, Copy, Default)]
pub struct Walk {
    /// Carry each chunk's loss backward through the cached K/V of the chunks before it (one exact
    /// step per window). false stops the gradient at each chunk boundary.
    pub exact: bool,
    /// How many cached positions before a chunk receive its gradient; `None` is the whole window.
    pub horizon: Option<NonZeroU32>,
}

/// The shadow adapter she learns into, and how.
pub struct Shadow<'a> {
    pub adapter: &'a LoraAdapter,
    pub optimizer: Optimizer,
    /// Recompute each layer's intermediates in the backward pass instead of keeping them all.
    pub recompute: bool,
    pub walk: Walk,
}

/// The target after one reply token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Target(i32),
    /// No target after this token: the adapter does not apply to its row.
    Unlabelled,
}

impl Label {
    fn native(self) -> i32 {
        match self {
            Self::Target(token) => token,
            Self::Unlabelled => -1,
        }
    }
}

/// One reply to learn, on the sequence that served it.
pub struct Reply<'a> {
    pub seq: i32,
    /// The positions `[0, n_cached)` the sequence holds as served, before the reply.
    pub n_cached: i32,
    pub tokens: &'a [i32],
    /// One per token.
    pub labels: &'a [Label],
    /// The exact original served tail, restored afterwards (it may differ from a correction).
    pub restore: &'a [i32],
}

/// Why learning refused or failed, in the native code's own words.
#[derive(Debug, Clone, thiserror::Error)]
#[error("learning refused at {step}: {message}")]
pub struct LearnRefusal {
    pub step: &'static str,
    pub message: String,
}

/// A reply refused before it began: its admission comes back to its owner.
pub struct Refused {
    pub refusal: LearnRefusal,
    pub admission: Box<dyn ReplyAdmission>,
}

/// How a reply ended. Either way its admission comes back, so its owner releases it.
pub enum Finished {
    Learned { admission: Box<dyn ReplyAdmission>, loss: Option<f64> },
    Failed { admission: Box<dyn ReplyAdmission>, refusal: LearnRefusal },
}

/// After one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    More,
    Done,
}

/// The callbacks' one holder, owned by the context so it outlives every native reference to it: the
/// optimizer and the context keep these pointers past a reply (the fork's `llama-context-opt.cpp`
/// :192-193, :275-276, :330-331, :365-366). A reply places its admission here and takes it back;
/// while it is empty every allocation is refused.
pub(super) struct CallbackHolder {
    admission: Option<Box<dyn ReplyAdmission>>,
    optimizer: sys::ggml_opt_optimizer_params,
}

impl CallbackHolder {
    fn empty() -> Self {
        Self { admission: None, optimizer: unsafe { sys::ggml_opt_get_default_optimizer_params(std::ptr::null_mut()) } }
    }
}

/// Learning over one serving context.
pub struct Learner<'c, 'm> {
    ctx: &'c mut Context<'m>,
    model: &'m Model,
    /// A run whose restoration was not confirmed, until it is fenced.
    lost: Option<Lost>,
}

impl<'c, 'm> Learner<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>, model: &'m Model) -> Self {
        Self { ctx, model, lost: None }
    }

    /// Why the learner refuses replies: a run whose restoration was not confirmed. Reading it never
    /// clears it; only [`Learner::fence`] does.
    pub fn lost_custody(&self) -> Option<&LearnRefusal> {
        self.lost.as_ref().map(|lost| &lost.refusal)
    }

    /// Fence the sequence an unconfirmed run left: remove every cell it holds, so nothing of the
    /// interrupted reply can be read as her served state. Returns the admission the run still held
    /// (its bytes stay retained until its owner releases them), and clears the block.
    pub fn fence(&mut self) -> Result<Option<Fenced>, LearnRefusal> {
        let Some(lost) = self.lost.take() else { return Ok(None) };
        // -1, -1: llama.h's whole range of the sequence.
        if !self.ctx.memory_seq_rm(lost.seq, -1, -1) {
            let refusal = LearnRefusal { step: "fence", message: format!("sequence {} could not be cleared", lost.seq) };
            self.lost = Some(lost);
            return Err(refusal);
        }
        Ok(Some(Fenced { seq: lost.seq, admission: lost.admission }))
    }

    /// Select her shadow under this reply's admission and begin the reply.
    pub fn begin<'l>(
        &'l mut self,
        shadow: &Shadow<'_>,
        admission: Box<dyn ReplyAdmission>,
        reply: &Reply<'_>,
    ) -> Result<LearnRun<'l, 'c, 'm>, Refused> {
        if let Some(lost) = self.lost.as_ref() {
            return Err(Refused { refusal: lost.refusal.clone(), admission });
        }
        // Everything that can refuse without native state is checked before the shadow is selected.
        if reply.labels.len() != reply.tokens.len() {
            let refusal = LearnRefusal { step: "reply", message: format!("{} labels for {} tokens", reply.labels.len(), reply.tokens.len()) };
            return Err(Refused { refusal, admission });
        }
        let (Ok(n), Ok(n_restore)) = (i32::try_from(reply.tokens.len()), i32::try_from(reply.restore.len())) else {
            let refusal = LearnRefusal { step: "reply", message: "a reply longer than a position holds".into() };
            return Err(Refused { refusal, admission });
        };
        let Some(result) = NonNull::new(unsafe { sys::ggml_opt_result_init() }) else {
            return Err(Refused { refusal: LearnRefusal { step: "result", message: "no result".into() }, admission });
        };
        let tokens = reply.tokens.to_vec();
        let labels: Vec<i32> = reply.labels.iter().map(|label| label.native()).collect();
        let restore = reply.restore.to_vec();
        let data = {
            let holder = self.ctx.learn_callbacks.get_or_insert_with(|| Box::new(CallbackHolder::empty()));
            holder.optimizer = optimizer_params(shadow.optimizer);
            holder.admission = Some(admission);
            std::ptr::from_mut(&mut **holder).cast::<c_void>()
        };
        if !unsafe { sys::llama_opt_init(self.ctx.as_ptr(), self.model.as_ptr(), shadow_params(shadow, data)) } {
            unsafe { sys::ggml_opt_result_free(result.as_ptr()) };
            let refusal = self.refusal("select_shadow");
            return Err(Refused { refusal, admission: self.reclaim() });
        }
        let begun = unsafe {
            sys::llama_opt_learn_begin(
                self.ctx.as_ptr(),
                reply.seq,
                reply.n_cached,
                tokens.as_ptr(),
                labels.as_ptr(),
                n,
                restore.as_ptr(),
                n_restore,
                result.as_ptr(),
            )
        };
        if !begun {
            unsafe { sys::ggml_opt_result_free(result.as_ptr()) };
            let refusal = self.refusal("learn_begin");
            return Err(Refused { refusal, admission: self.reclaim() });
        }
        Ok(LearnRun { learner: self, result, seq: reply.seq, state: RunState::Running, _buffers: (tokens, labels, restore) })
    }

    /// Whether a reply's admission is in the holder (a run not yet finished or refused).
    fn holds_admission(&self) -> bool {
        self.ctx.learn_callbacks.as_ref().is_some_and(|holder| holder.admission.is_some())
    }

    /// Take the reply's admission back out of the holder: native callbacks reaching the holder from
    /// here on find it empty and refuse.
    fn reclaim_admission(&mut self) -> Option<Box<dyn ReplyAdmission>> {
        self.ctx.learn_callbacks.as_mut().and_then(|holder| holder.admission.take())
    }

    /// As [`Self::reclaim_admission`], for the paths that placed one themselves: the holder is filled
    /// by `begin` before any of them, so `Released` stands only for an admission already handed back.
    fn reclaim(&mut self) -> Box<dyn ReplyAdmission> {
        self.reclaim_admission().unwrap_or_else(|| Box::new(Released)) // unwrap_or_else: begin filled the holder; Released admits nothing
    }

    fn refusal(&mut self, step: &'static str) -> LearnRefusal {
        let failure = unsafe { sys::llama_opt_failure(self.ctx.as_ptr()) };
        let message = if failure.is_null() {
            "no reason given".to_owned()
        } else {
            unsafe { CStr::from_ptr(failure) }.to_string_lossy().into_owned()
        };
        LearnRefusal { step, message }
    }
}

/// One reply in progress. Holds the context until it finishes.
#[must_use = "a run is finished explicitly: dropping it drains and records a custody loss"]
pub struct LearnRun<'l, 'c, 'm> {
    learner: &'l mut Learner<'c, 'm>,
    result: NonNull<sys::ggml_opt_result>,
    seq: i32,
    state: RunState,
    /// The reply's tokens, labels and restore tail, alive while the reply is.
    _buffers: (Vec<i32>, Vec<i32>, Vec<i32>),
}

impl LearnRun<'_, '_, '_> {
    /// One chunk: a gradient chunk or the stable restoration. After the run ended, its end again:
    /// native code is never stepped past a terminal status.
    pub fn step(&mut self) -> Result<Progress, LearnRefusal> {
        if let Some(ended) = self.state.ended() {
            return ended;
        }
        let status = unsafe { sys::llama_opt_learn_step(self.learner.ctx.as_ptr()) };
        self.observe(status, "learn_step")
    }

    /// One indivisible graph node: the finest quantum, for a turn that cannot wait a whole chunk.
    pub fn graph_step(&mut self) -> Result<Progress, LearnRefusal> {
        if let Some(ended) = self.state.ended() {
            return ended;
        }
        let status = unsafe { sys::llama_opt_learn_graph_step(self.learner.ctx.as_ptr()) };
        self.observe(status, "learn_graph_step")
    }

    /// Whether a further gradient chunk remains (restoration needs no new grant).
    pub fn needs_gradient(&self) -> bool {
        unsafe { sys::llama_opt_learn_needs_gradient(self.learner.ctx.as_ptr()) }
    }

    /// Stop admitting gradient chunks; later steps only restore the served tail. Steps already
    /// committed stay committed. Call between steps; a repeat is harmless.
    pub fn drain(&mut self) {
        unsafe { sys::llama_opt_learn_drain(self.learner.ctx.as_ptr()) };
    }

    /// Run to the end: drain, then step until the restoration is confirmed or refused. Hands the
    /// admission back either way. A run that failed (now or at an earlier step) is `Failed`, never
    /// `Learned`, and its unconfirmed restoration blocks the learner until it is fenced. Consuming
    /// `self` makes a second finish unrepresentable.
    pub fn finish(mut self) -> Finished {
        let outcome = self.run_out();
        let admission = self.learner.reclaim();
        match outcome {
            Ok(()) => Finished::Learned { admission, loss: self.loss() },
            Err(refusal) => {
                self.learner.lost = Some(Lost { refusal: refusal.clone(), seq: self.seq, admission: None });
                Finished::Failed { admission, refusal }
            }
        }
    }

    fn run_out(&mut self) -> Result<(), LearnRefusal> {
        if self.state.is_running() {
            self.drain();
        }
        loop {
            match self.step()? {
                Progress::Done => return Ok(()),
                Progress::More => {}
            }
        }
    }

    fn observe(&mut self, status: sys::llama_opt_learn_status, step: &'static str) -> Result<Progress, LearnRefusal> {
        let native = if status == sys::llama_opt_learn_status_LLAMA_OPT_LEARN_MORE {
            Status::More
        } else if status == sys::llama_opt_learn_status_LLAMA_OPT_LEARN_DONE {
            Status::Done
        } else {
            Status::Failed
        };
        let learner = &mut *self.learner;
        self.state.observe(native, || learner.refusal(step))
    }

    fn loss(&self) -> Option<f64> {
        let (mut loss, mut uncertainty) = (0.0_f64, 0.0_f64);
        unsafe { sys::ggml_opt_result_loss(self.result.as_ptr(), &mut loss, &mut uncertainty) };
        loss.is_finite().then_some(loss)
    }

}

impl Drop for LearnRun<'_, '_, '_> {
    fn drop(&mut self) {
        if self.learner.holds_admission() {
            if let Err(refusal) = self.run_out() {
                // Nobody took the admission: the learner keeps it (its bytes retained) with the block.
                let admission = self.learner.reclaim_admission();
                self.learner.lost = Some(Lost { refusal, seq: self.seq, admission });
            }
        }
        unsafe { sys::ggml_opt_result_free(self.result.as_ptr()) };
    }
}

/// A run whose restoration was not confirmed: the learner refuses replies until it is fenced.
struct Lost {
    refusal: LearnRefusal,
    seq: i32,
    /// The admission nobody took (a dropped run): retained until the fence hands it back.
    admission: Option<Box<dyn ReplyAdmission>>,
}

/// A fenced sequence, and the admission a dropped run still held.
pub struct Fenced {
    pub seq: i32,
    pub admission: Option<Box<dyn ReplyAdmission>>,
}

/// What native code said after one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    More,
    Done,
    Failed,
}

/// A run's state. A failure is terminal and kept apart from a confirmed restoration, so neither a
/// later step nor finish can read a failed run as learned.
#[derive(Debug, Clone)]
enum RunState {
    Running,
    Restored,
    Failed(LearnRefusal),
}

impl RunState {
    fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// The run's end, once it has one.
    fn ended(&self) -> Option<Result<Progress, LearnRefusal>> {
        match self {
            Self::Running => None,
            Self::Restored => Some(Ok(Progress::Done)),
            Self::Failed(refusal) => Some(Err(refusal.clone())),
        }
    }

    fn observe(&mut self, status: Status, refusal: impl FnOnce() -> LearnRefusal) -> Result<Progress, LearnRefusal> {
        match status {
            Status::More => Ok(Progress::More),
            Status::Done => {
                *self = Self::Restored;
                Ok(Progress::Done)
            }
            Status::Failed => {
                let refusal = refusal();
                *self = Self::Failed(refusal.clone());
                Err(refusal)
            }
        }
    }
}

/// The admission of a run already handed back: it admits nothing.
struct Released;

impl ReplyAdmission for Released {
    fn optimizer_buffer(&self, _: BufferType, _: OptimizerBuffer<'_>, _: u64) -> bool {
        false
    }
    fn workspace(&self, _: BufferType, _: u64, _: u64) -> bool {
        false
    }
}

fn optimizer_params(optimizer: Optimizer) -> sys::ggml_opt_optimizer_params {
    let mut params = unsafe { sys::ggml_opt_get_default_optimizer_params(std::ptr::null_mut()) };
    match optimizer {
        Optimizer::AdamW { learning_rate, beta1, beta2, eps, weight_decay } => {
            params.adamw.alpha = learning_rate;
            params.adamw.beta1 = beta1;
            params.adamw.beta2 = beta2;
            params.adamw.eps = eps;
            params.adamw.wd = weight_decay;
        }
        Optimizer::Sgd { learning_rate, weight_decay } => {
            params.sgd.alpha = learning_rate;
            params.sgd.wd = weight_decay;
        }
    }
    params
}

/// llama.h encodes "the whole window" as a horizon of 0; a Rust horizon is never 0, so the two meet
/// only here.
fn native_horizon(horizon: Option<NonZeroU32>) -> u32 {
    const WHOLE_WINDOW: u32 = 0;
    match horizon {
        Some(positions) => positions.get(),
        None => WHOLE_WINDOW,
    }
}

fn shadow_params(shadow: &Shadow<'_>, data: *mut c_void) -> sys::llama_opt_params {
    sys::llama_opt_params {
        n_ctx_train: 0,
        param_filter: Some(sys::llama_opt_param_filter_all),
        param_filter_ud: std::ptr::null_mut(),
        get_opt_pars: Some(constant_optimizer),
        get_opt_pars_ud: data,
        optimizer_type: match shadow.optimizer {
            Optimizer::AdamW { .. } => sys::ggml_opt_optimizer_type_GGML_OPT_OPTIMIZER_TYPE_ADAMW,
            Optimizer::Sgd { .. } => sys::ggml_opt_optimizer_type_GGML_OPT_OPTIMIZER_TYPE_SGD,
        },
        adapter: shadow.adapter.ptr.as_ptr(),
        recompute: shadow.recompute,
        walk_exact: shadow.walk.exact,
        walk_horizon: native_horizon(shadow.walk.horizon),
        static_admission: Some(optimizer_buffer_admitted),
        static_admission_data: data,
        workspace_admission: Some(workspace_admitted),
        workspace_admission_data: data,
    }
}

unsafe extern "C" fn constant_optimizer(data: *mut c_void) -> sys::ggml_opt_optimizer_params {
    // SAFETY: `data` is the context's boxed CallbackHolder, freed only after the context itself.
    unsafe { (*data.cast::<CallbackHolder>()).optimizer }
}

unsafe extern "C" fn optimizer_buffer_admitted(
    buft: sys::ggml_backend_buffer_type_t,
    kind: *const c_char,
    bytes: usize,
    data: *mut c_void,
) -> bool {
    let (Some(buffer), false) = (NonNull::new(buft), kind.is_null()) else { return false };
    // SAFETY: as above; `kind` is a NUL-terminated name native code owns for the call.
    let (holder, kind) = unsafe { (&*data.cast::<CallbackHolder>(), CStr::from_ptr(kind)) };
    let Some(admission) = holder.admission.as_deref() else { return false };
    // A panic must never unwind into native code: it refuses the allocation instead.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        admission.optimizer_buffer(BufferType(buffer), OptimizerBuffer(kind), bytes as u64)
    }))
    .unwrap_or(false) // unwrap_or: a panicking admission refuses, never unwinds across the boundary
}

unsafe extern "C" fn workspace_admitted(
    buft: sys::ggml_backend_buffer_type_t,
    required: usize,
    held: usize,
    data: *mut c_void,
) -> bool {
    let Some(buffer) = NonNull::new(buft) else { return false };
    // SAFETY: as above.
    let holder = unsafe { &*data.cast::<CallbackHolder>() };
    let Some(admission) = holder.admission.as_deref() else { return false };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        admission.workspace(BufferType(buffer), required as u64, held as u64)
    }))
    .unwrap_or(false) // unwrap_or: a panicking admission refuses, never unwinds across the boundary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // what this catches: an unlabelled token sent as a real token id (training on a row that has no
    // target), or a target rewritten.
    #[test]
    fn an_unlabelled_token_is_the_native_no_target() {
        assert_eq!(Label::Unlabelled.native(), -1);
        assert_eq!(Label::Target(151_643).native(), 151_643);
    }

    // what this catches: a released admission that still admits, so a run already handed back
    // could grow memory nobody owns.
    #[test]
    fn a_handed_back_admission_admits_nothing() {
        let kind = CString::new("adamw_m").unwrap_or_default(); // unwrap_or_default: a literal without NUL
        let buffer = BufferType(NonNull::dangling());
        assert!(!Released.optimizer_buffer(buffer, OptimizerBuffer(&kind), 1));
        assert!(!Released.workspace(buffer, 1, 1));
    }

    fn refusal() -> LearnRefusal {
        LearnRefusal { step: "learn_step", message: "device lost".into() }
    }

    // what this catches (Codex on #6148): a native refusal marked as done, so a later finish
    // returned Learned and a drop recorded no lost custody for a run that failed.
    #[test]
    fn a_failure_after_a_step_stays_failed_through_later_steps_and_finish() {
        let mut state = RunState::Running;
        assert!(matches!(state.observe(Status::More, refusal), Ok(Progress::More)));
        assert!(matches!(state.observe(Status::Failed, refusal), Err(r) if r.message == "device lost"));
        assert!(!state.is_running());
        // A later step and the finish see the failure, never a confirmed restoration.
        assert!(matches!(state.ended(), Some(Err(r)) if r.step == "learn_step"));
        assert!(!matches!(state.ended(), Some(Ok(Progress::Done))));
    }

    // what this catches: a confirmed restoration overwritten by a later status, or read as running
    // so finish drains and steps a reply native code already ended.
    #[test]
    fn a_confirmed_restoration_is_terminal() {
        let mut state = RunState::Running;
        assert!(matches!(state.observe(Status::Done, refusal), Ok(Progress::Done)));
        assert!(matches!(state.ended(), Some(Ok(Progress::Done))));
        assert!(!state.is_running());
    }

    // what this catches: the native "whole window" encoding leaking past the boundary, or a real
    // horizon rewritten.
    #[test]
    fn the_whole_window_is_llamas_zero_only_at_the_boundary() {
        assert_eq!(native_horizon(None), 0);
        assert_eq!(native_horizon(NonZeroU32::new(512)), 512);
    }
}
