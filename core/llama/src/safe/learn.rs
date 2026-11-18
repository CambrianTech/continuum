//! Learning on a live sequence of a serving context (IN-PROCESS-LEARNING.md, L1): the fork's
//! resumable `llama_opt_learn_*`, with no policy. One reply trains on one sequence. Each step runs
//! at most one chunk (a gradient chunk or the stable restoration), so other sequences may decode
//! between steps. Afterwards the reply is restored under her stable gene set, leaving the sequence
//! as serving left it.
//!
//! Ownership, so native code never calls into something gone:
//! - the run owns its admission (and the optimizer parameters its callback returns) in one boxed
//!   binding, from the shadow's selection until a CONFIRMED restoration ([`LearnRun::finish`]),
//!   and hands it back either way;
//! - the run owns the reply's buffers and the native result for as long as the reply is active;
//! - finishing is explicit and fallible. Dropping an unfinished run drains it and records the loss
//!   of custody on its [`Learner`], which refuses its next reply until the owner reads it.
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

/// The boxed binding native callbacks reach: stable in memory for the run's whole life.
struct Binding {
    admission: Box<dyn ReplyAdmission>,
    optimizer: sys::ggml_opt_optimizer_params,
}

/// Learning over one serving context.
pub struct Learner<'c, 'm> {
    ctx: &'c mut Context<'m>,
    model: &'m Model,
    /// A run dropped without finishing whose restoration was not confirmed.
    lost: Option<LearnRefusal>,
}

impl<'c, 'm> Learner<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>, model: &'m Model) -> Self {
        Self { ctx, model, lost: None }
    }

    /// A custody loss recorded by an unfinished run's drop, taken by its owner.
    pub fn take_lost_custody(&mut self) -> Option<LearnRefusal> {
        self.lost.take()
    }

    /// Select her shadow under this reply's admission and begin the reply.
    pub fn begin<'l>(
        &'l mut self,
        shadow: &Shadow<'_>,
        admission: Box<dyn ReplyAdmission>,
        reply: &Reply<'_>,
    ) -> Result<LearnRun<'l, 'c, 'm>, Refused> {
        if let Some(lost) = self.lost.clone() {
            return Err(Refused { refusal: lost, admission });
        }
        if reply.labels.len() != reply.tokens.len() {
            let refusal = LearnRefusal { step: "reply", message: format!("{} labels for {} tokens", reply.labels.len(), reply.tokens.len()) };
            return Err(Refused { refusal, admission });
        }
        let mut binding = Box::new(Binding { admission, optimizer: optimizer_params(shadow.optimizer) });
        let params = shadow_params(shadow, &mut binding);
        if !unsafe { sys::llama_opt_init(self.ctx.as_ptr(), self.model.as_ptr(), params) } {
            return Err(Refused { refusal: self.refusal("select_shadow"), admission: binding.admission });
        }
        let tokens = reply.tokens.to_vec();
        let labels: Vec<i32> = reply.labels.iter().map(|label| label.native()).collect();
        let restore = reply.restore.to_vec();
        let (Ok(n), Ok(n_restore)) = (i32::try_from(tokens.len()), i32::try_from(restore.len())) else {
            let refusal = LearnRefusal { step: "reply", message: "a reply longer than a position holds".into() };
            return Err(Refused { refusal, admission: binding.admission });
        };
        let Some(result) = NonNull::new(unsafe { sys::ggml_opt_result_init() }) else {
            return Err(Refused { refusal: LearnRefusal { step: "result", message: "no result".into() }, admission: binding.admission });
        };
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
            return Err(Refused { refusal: self.refusal("learn_begin"), admission: binding.admission });
        }
        Ok(LearnRun { learner: self, binding: Some(binding), result, done: false, _buffers: (tokens, labels, restore) })
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
    binding: Option<Box<Binding>>,
    result: NonNull<sys::ggml_opt_result>,
    done: bool,
    /// The reply's tokens, labels and restore tail, alive while the reply is.
    _buffers: (Vec<i32>, Vec<i32>, Vec<i32>),
}

impl LearnRun<'_, '_, '_> {
    /// One chunk: a gradient chunk or the stable restoration.
    pub fn step(&mut self) -> Result<Progress, LearnRefusal> {
        let status = unsafe { sys::llama_opt_learn_step(self.learner.ctx.as_ptr()) };
        self.progress(status, "learn_step")
    }

    /// One indivisible graph node: the finest quantum, for a turn that cannot wait a whole chunk.
    pub fn graph_step(&mut self) -> Result<Progress, LearnRefusal> {
        let status = unsafe { sys::llama_opt_learn_graph_step(self.learner.ctx.as_ptr()) };
        self.progress(status, "learn_graph_step")
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
    /// admission back either way.
    pub fn finish(mut self) -> Finished {
        let outcome = self.run_out();
        let admission = self.take_admission();
        match outcome {
            Ok(()) => Finished::Learned { admission, loss: self.loss() },
            Err(refusal) => Finished::Failed { admission, refusal },
        }
    }

    fn run_out(&mut self) -> Result<(), LearnRefusal> {
        if !self.done {
            self.drain();
        }
        while !self.done {
            self.step()?;
        }
        Ok(())
    }

    fn progress(&mut self, status: sys::llama_opt_learn_status, step: &'static str) -> Result<Progress, LearnRefusal> {
        if status == sys::llama_opt_learn_status_LLAMA_OPT_LEARN_MORE {
            Ok(Progress::More)
        } else if status == sys::llama_opt_learn_status_LLAMA_OPT_LEARN_DONE {
            self.done = true;
            Ok(Progress::Done)
        } else {
            self.done = true;
            Err(self.learner.refusal(step))
        }
    }

    fn loss(&self) -> Option<f64> {
        let (mut loss, mut uncertainty) = (0.0_f64, 0.0_f64);
        unsafe { sys::ggml_opt_result_loss(self.result.as_ptr(), &mut loss, &mut uncertainty) };
        loss.is_finite().then_some(loss)
    }

    fn take_admission(&mut self) -> Box<dyn ReplyAdmission> {
        match self.binding.take() {
            Some(binding) => binding.admission,
            None => Box::new(Released),
        }
    }
}

impl Drop for LearnRun<'_, '_, '_> {
    fn drop(&mut self) {
        if self.binding.is_some() {
            if let Err(refusal) = self.run_out() {
                self.learner.lost = Some(refusal);
            }
        }
        unsafe { sys::ggml_opt_result_free(self.result.as_ptr()) };
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

fn shadow_params(shadow: &Shadow<'_>, binding: &mut Binding) -> sys::llama_opt_params {
    let data = std::ptr::from_mut(binding).cast::<c_void>();
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
    // SAFETY: `data` is the run's boxed Binding, alive until the run finishes or drops.
    unsafe { (*data.cast::<Binding>()).optimizer }
}

unsafe extern "C" fn optimizer_buffer_admitted(
    buft: sys::ggml_backend_buffer_type_t,
    kind: *const c_char,
    bytes: usize,
    data: *mut c_void,
) -> bool {
    let (Some(buffer), false) = (NonNull::new(buft), kind.is_null()) else { return false };
    // SAFETY: as above; `kind` is a NUL-terminated name native code owns for the call.
    let (binding, kind) = unsafe { (&*data.cast::<Binding>(), CStr::from_ptr(kind)) };
    // A panic must never unwind into native code: it refuses the allocation instead.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        binding.admission.optimizer_buffer(BufferType(buffer), OptimizerBuffer(kind), bytes as u64)
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
    let binding = unsafe { &*data.cast::<Binding>() };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        binding.admission.workspace(BufferType(buffer), required as u64, held as u64)
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
}
