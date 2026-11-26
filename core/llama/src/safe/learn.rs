//! Learning on a live sequence of a serving context (IN-PROCESS-LEARNING.md, L1): the fork's
//! resumable `llama_opt_learn_*`, with no policy. One reply trains on one sequence. Each step runs
//! at most one chunk (a gradient chunk or the stable restoration), so other sequences may decode
//! between steps. Afterwards the reply is restored under her stable gene set, leaving the sequence
//! as serving left it.
//!
//! Ownership lives on the CONTEXT, because native code keeps its pointers past a reply and past any
//! one [`Learner`]:
//! - every native callback reaches ONE holder the context owns (freed after `llama_free`). A reply
//!   places its admission there; an empty holder refuses every allocation;
//! - the trained shadow adapter is shared into that holder for as long as an optimizer may reference
//!   it, and the model is the context's own;
//! - a run whose restoration was not confirmed QUARANTINES the context: its admission (bytes still
//!   retained) stays in the holder, and every later reply on that context is refused, whoever asks,
//!   until [`Learner::recover`] proves the optimizer and its workspace released. The sequence itself
//!   is left as native code left it, for its owner to restore from its own preserved state.
//!
//! The shadow adapter must already be attached to the context at a non-zero scale (the caller binds
//! her gene set), with F32 A/B tensors; otherwise selection is refused by name.

use std::ffi::{c_char, c_void, CStr};
use std::num::{NonZeroU32, NonZeroU64};
use std::ptr::NonNull;
use std::sync::Arc;

use super::optimizer_slots::{OptimizerLease, Role, Slot, Slots};

mod memory;
pub use memory::{MemoryRole, MemoryRow};
mod shadow;
pub use shadow::{FreshGeometry, ShadowMeasure};

use super::*;

/// One allocator a buffer comes from: the physical device it lives on, as the backend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferType(pub(super) NonNull<sys::ggml_backend_buffer_type>);

// SAFETY: a ggml buffer type is a backend-owned singleton that lives for the process and is only
// read through its query functions, so its pointer may cross threads with the slot that names it.
unsafe impl Send for BufferType {}
unsafe impl Sync for BufferType {}

impl BufferType {
    /// The backend's own name for this buffer type ("CUDA0", "Metal", "CPU", ...).
    pub fn name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(sys::ggml_backend_buft_name(self.0.as_ptr())) }.to_str()
    }

    /// The device's own name ("CUDA0", "MTL0", "CPU", ...), when the buffer type has a device.
    pub fn device(&self) -> Option<Result<&str, std::str::Utf8Error>> {
        let device = unsafe { sys::ggml_backend_buft_get_device(self.0.as_ptr()) };
        if device.is_null() {
            return None;
        }
        let name = unsafe { sys::ggml_backend_dev_name(device) };
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_str())
    }

    /// Whether these bytes are host memory (RAM), not device memory.
    pub fn is_host(&self) -> bool {
        unsafe { sys::ggml_backend_buft_is_host(self.0.as_ptr()) }
    }
}

/// The memory owner's answer for one reply's shared workspace. Native code asks it before it
/// allocates; it must not allocate itself.
pub trait ReplyAdmission: Send {
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

/// The shadow adapter she learns into, and how. It must already be retained on the context
/// ([`Learner::retain_shadow`]), which keeps it, and its optimizer lease, for as long as native code
/// keeps its optimizer.
pub struct Shadow {
    pub adapter: Arc<LoraAdapter>,
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

/// A reply refused before native state changed: its admission comes back to its owner.
pub struct Refused {
    pub refusal: LearnRefusal,
    pub admission: Box<dyn ReplyAdmission>,
}

/// How a reply ended. Every restored variant hands the reply's admission back for its owner to
/// release; which one it is comes from the shadow's own native step count, never from the status.
pub enum Finished {
    /// Restored under her stable set, with optimizer steps committed to her shadow (a drain after a
    /// commit keeps the steps already committed). The loss is the committed steps' loss.
    Learned { admission: Box<dyn ReplyAdmission>, steps: NonZeroU64, loss: Option<f64> },
    /// Restored with nothing committed: drained (or finished) before a gradient step landed.
    Unchanged { admission: Box<dyn ReplyAdmission> },
    /// Restored, but native code marks her shadow's effects uncertain: it may hold partial writes
    /// the committed count does not represent, so the shadow must not be kept or published as
    /// learned. `steps` is the count native code did commit.
    Uncertain { admission: Box<dyn ReplyAdmission>, steps: u64 },
    /// Not confirmed restored: the context is quarantined and keeps the admission (its bytes
    /// retained) until [`Learner::recover`] proves the optimizer and workspace released.
    Failed { refusal: LearnRefusal, seq: i32 },
}

/// What a verified recovery hands back.
pub struct Recovered {
    /// The sequence the failed run left: its owner restores it from its own preserved state.
    pub seq: i32,
    /// The admission the failed run held, now safe to release.
    pub admission: Box<dyn ReplyAdmission>,
}

/// After one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    More,
    Done,
}

/// The callbacks' one holder, owned by the context so it outlives every native reference to it: the
/// optimizer and the context keep these pointers past a reply (the fork's `llama-context-opt.cpp`
/// :192-193, :275-276, :330-331, :365-366).
pub(super) struct CallbackHolder {
    /// The active reply's workspace admission; empty refuses every workspace growth.
    admission: Option<Box<dyn ReplyAdmission>>,
    optimizer: sys::ggml_opt_optimizer_params,
    /// Every shadow native code may keep an optimizer for (native keeps one per adapter across
    /// switches), each with its lease, until a confirmed release.
    shadows: Shadows<Retained>,
    /// The shadow selected for the current reply: optimizer buffers are charged to its lease.
    selected: Option<ShadowKey>,
    /// A run whose restoration was not confirmed, and the admission it held.
    quarantine: Option<Quarantine>,
    /// Native code owned optimizer memory nobody admitted: accounting is wrong, so no reply begins
    /// until an owner looks.
    accounting: Option<LearnRefusal>,
}

/// A retained shadow: the adapter (kept alive), its optimizer lease, and what each of its optimizer
/// buffers holds now.
struct Retained {
    _adapter: Arc<LoraAdapter>,
    lease: Box<dyn OptimizerLease>,
    slots: Slots,
}

/// Which adapter a native optimizer is keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShadowKey(NonNull<sys::llama_adapter_lora>);

impl ShadowKey {
    fn of(adapter: &LoraAdapter) -> Self {
        Self(adapter.ptr)
    }
}

/// The retained shadows, by native key. Retaining one twice is refused (its first lease stays), and
/// only a confirmed release takes one out, so a switch or a refused selection never drops one.
struct Shadows<H>(Vec<(ShadowKey, H)>);

impl<H> Shadows<H> {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn retain(&mut self, key: ShadowKey, held: H) -> Result<(), H> {
        if self.get(key).is_some() {
            return Err(held);
        }
        self.0.push((key, held));
        Ok(())
    }

    fn get(&self, key: ShadowKey) -> Option<&H> {
        self.0.iter().find(|(held, _)| *held == key).map(|(_, held)| held)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn iter(&self) -> impl Iterator<Item = &(ShadowKey, H)> {
        self.0.iter()
    }

    fn take(&mut self, key: ShadowKey) -> Option<H> {
        let index = self.0.iter().position(|(held, _)| *held == key)?;
        Some(self.0.swap_remove(index).1)
    }
}

struct Quarantine {
    refusal: LearnRefusal,
    seq: i32,
    admission: Option<Box<dyn ReplyAdmission>>,
}

impl CallbackHolder {
    fn empty() -> Self {
        let optimizer = unsafe { sys::ggml_opt_get_default_optimizer_params(std::ptr::null_mut()) };
        Self { admission: None, optimizer, shadows: Shadows::new(), selected: None, quarantine: None, accounting: None }
    }
}

/// Learning over one serving context. Holds no state of its own: the context does.
pub struct Learner<'c, 'm> {
    ctx: &'c mut Context<'m>,
}

impl<'c, 'm> Learner<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>) -> Self {
        Self { ctx }
    }

    fn holder(&mut self) -> &mut CallbackHolder {
        self.ctx.learn_callbacks.get_or_insert_with(|| Box::new(CallbackHolder::empty()))
    }

    /// Why the context refuses replies: a run whose restoration was not confirmed. Reading it never
    /// clears it.
    pub fn quarantine(&self) -> Option<&LearnRefusal> {
        self.ctx.learn_callbacks.as_ref().and_then(|holder| holder.quarantine.as_ref()).map(|q| &q.refusal)
    }

    /// Why accounting is not trusted: native code owned optimizer memory nobody admitted. No reply
    /// begins while this is set.
    pub fn accounting(&self) -> Option<&LearnRefusal> {
        self.ctx.learn_callbacks.as_ref().and_then(|holder| holder.accounting.as_ref())
    }

    /// After an optimizer operation (never on ordinary decode), on the context's owner: visit the
    /// optimizer buffers native code owns and reconcile every retained shadow's slots to them.
    /// Charged but not resident is released; resident but not admitted, or not attributable to a
    /// retained shadow and a known role, is recorded as an accounting defect.
    fn reconcile(&mut self) {
        let mut resident = Resident::default();
        unsafe { sys::llama_opt_visit_memory(self.ctx.as_ptr(), Some(visited), std::ptr::from_mut(&mut resident).cast()) };
        let holder = self.holder();
        let mut defect = resident.unattributed(&holder.shadows);
        for (key, retained) in holder.shadows.iter() {
            for role in Role::ALL {
                if let Some(unadmitted) = retained.slots.reconcile(&*retained.lease, role, resident.bytes(*key, role)) {
                    defect.get_or_insert(LearnRefusal {
                        step: "accounting",
                        message: format!("{} resident {:?} optimizer bytes were never admitted", unadmitted.bytes, unadmitted.role),
                    });
                }
            }
        }
        if defect.is_some() {
            holder.accounting = defect;
        }
    }

    /// Retain a shadow before she learns into it: the context keeps the adapter and its optimizer
    /// lease until [`Learner::release_shadow`]. Retaining one already retained hands the new lease
    /// back (the first one stays, with whatever it already admitted).
    pub fn retain_shadow(&mut self, adapter: &Arc<LoraAdapter>, lease: Box<dyn OptimizerLease>) -> Result<(), Box<dyn OptimizerLease>> {
        let key = ShadowKey::of(adapter);
        self.holder().shadows.retain(key, Retained { _adapter: Arc::clone(adapter), lease, slots: Slots::default() }).map_err(|refused| refused.lease)
    }

    /// Release a shadow's optimizer: only when native code confirms no optimizer references the
    /// adapter any more (`llama_opt_release`) does the context let go of the adapter and hand back
    /// its lease, whose bytes the owner then releases.
    pub fn release_shadow(&mut self, adapter: &LoraAdapter) -> Result<Box<dyn OptimizerLease>, LearnRefusal> {
        let key = ShadowKey::of(adapter);
        if self.holder().shadows.get(key).is_none() {
            return Err(LearnRefusal { step: "release_shadow", message: "the shadow is not retained on this context".into() });
        }
        if !unsafe { sys::llama_opt_release(self.ctx.as_ptr(), key.0.as_ptr()) } {
            return Err(self.refusal("release_shadow"));
        }
        let holder = self.holder();
        if holder.selected == Some(key) {
            holder.selected = None;
        }
        let Some(retained) = holder.shadows.take(key) else {
            return Err(LearnRefusal { step: "release_shadow", message: "the shadow is not retained on this context".into() });
        };
        // Native code let the optimizer go: every slot it held is proven free. The visit then checks
        // nothing is left behind for it.
        retained.slots.release_all(&*retained.lease);
        self.reconcile();
        Ok(retained.lease)
    }

    /// Prove the failed run's training workspace released, then lift the quarantine and hand back
    /// the reply admission it held. The order is native code's: the workspace is released only once
    /// no optimizer holds the scheduler (`opt_release_workspace` refuses while any shadow keeps one),
    /// so every retained shadow is released first through [`Learner::release_shadow`], each handing
    /// back its lease. Until then this refuses and changes nothing: the quarantine, the retained
    /// shadows and their leases all stay. The sequence is untouched: its owner restores it.
    pub fn recover(&mut self) -> Result<Option<Recovered>, LearnRefusal> {
        if self.quarantine().is_none() {
            return Ok(None);
        }
        let retained = self.holder().shadows.len();
        if retained > 0 {
            return Err(LearnRefusal { step: "recover", message: format!("{retained} shadow(s) still retained: release each before the workspace") });
        }
        if !unsafe { sys::llama_opt_release_workspace(self.ctx.as_ptr()) } {
            return Err(self.refusal("release_workspace"));
        }
        let Some(quarantine) = self.holder().quarantine.take() else { return Ok(None) };
        let admission = quarantine.admission.unwrap_or_else(|| Box::new(Released)); // unwrap_or_else: a quarantine always holds its run's admission; Released admits nothing
        Ok(Some(Recovered { seq: quarantine.seq, admission }))
    }

    /// Select her shadow under this reply's admission and begin the reply.
    pub fn begin<'l>(&'l mut self, shadow: &Shadow, admission: Box<dyn ReplyAdmission>, reply: &Reply<'_>) -> Result<LearnRun<'l, 'c, 'm>, Refused> {
        if let Some(refusal) = self.quarantine().or(self.accounting()) {
            return Err(Refused { refusal: refusal.clone(), admission });
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
        // A reply trains on per-token logits: a pooled (embedding) context has none, and native code
        // would refuse only at the first labelled step (Codex's BigMama replay at 7f8462502).
        if unsafe { sys::llama_pooling_type(self.ctx.as_ptr()) } != sys::llama_pooling_type_LLAMA_POOLING_TYPE_NONE {
            let refusal = LearnRefusal { step: "context", message: "a pooled context cannot learn: create it with PoolingType::None".into() };
            return Err(Refused { refusal, admission });
        }
        let key = ShadowKey::of(&shadow.adapter);
        if self.holder().shadows.get(key).is_none() {
            let refusal = LearnRefusal { step: "select_shadow", message: "the shadow is not retained on this context".into() };
            return Err(Refused { refusal, admission });
        }
        let Some(result) = NonNull::new(unsafe { sys::ggml_opt_result_init() }) else {
            return Err(Refused { refusal: LearnRefusal { step: "result", message: "no result".into() }, admission });
        };
        let tokens = reply.tokens.to_vec();
        let labels: Vec<i32> = reply.labels.iter().map(|label| label.native()).collect();
        let restore = reply.restore.to_vec();
        let data = {
            let holder = self.holder();
            holder.optimizer = optimizer_params(shadow.optimizer);
            holder.admission = Some(admission);
            holder.selected = Some(key);
            std::ptr::from_mut(holder).cast::<c_void>()
        };
        let params = shadow_params(shadow, data);
        let selected = unsafe { sys::llama_opt_init(self.ctx.as_ptr(), self.ctx.model.as_ptr(), params) };
        // The init ended, admitted or refused: native code's own buffers say what survived.
        self.reconcile();
        if !selected {
            unsafe { sys::ggml_opt_result_free(result.as_ptr()) };
            let refusal = self.refusal("select_shadow");
            return Err(Refused { refusal, admission: self.reclaim() });
        }
        // Her shadow's own count before the reply: what it committed is measured against this.
        let steps_before = unsafe { sys::llama_opt_shadow_steps(self.ctx.as_ptr(), key.0.as_ptr()) };
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
        Ok(LearnRun { learner: self, result, seq: reply.seq, shadow: key, steps_before, state: RunState::Running, _buffers: (tokens, labels, restore) })
    }

    /// Take the reply's admission back out of the holder (native state unchanged by the reply):
    /// callbacks reaching the holder from here on find it empty and refuse.
    fn reclaim(&mut self) -> Box<dyn ReplyAdmission> {
        self.holder().admission.take().unwrap_or_else(|| Box::new(Released)) // unwrap_or_else: begin filled the holder; Released admits nothing
    }

    /// Quarantine the context after a run whose restoration was not confirmed: its admission stays in
    /// the holder's quarantine, so no allocation is admitted and no bytes are released.
    fn quarantine_after(&mut self, refusal: &LearnRefusal, seq: i32) {
        let holder = self.holder();
        let admission = holder.admission.take();
        holder.quarantine = Some(Quarantine { refusal: refusal.clone(), seq, admission });
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
#[must_use = "a run is finished explicitly: dropping it drains, and an unconfirmed restoration quarantines the context"]
pub struct LearnRun<'l, 'c, 'm> {
    learner: &'l mut Learner<'c, 'm>,
    result: NonNull<sys::ggml_opt_result>,
    seq: i32,
    /// Her shadow, and its native step count when the reply began.
    shadow: ShadowKey,
    steps_before: i64,
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
        self.reconciled();
        self.observe(status, "learn_step")
    }

    /// After a native step: its graph build may have asked for optimizer buffers, so native code says
    /// what survived. If accounting is no longer trusted, no further gradient runs: the reply drains
    /// and only restores, and it can no longer finish as Learned.
    fn reconciled(&mut self) {
        self.learner.reconcile();
        if self.learner.accounting().is_some() {
            self.drain();
        }
    }

    /// One indivisible graph node: the finest quantum, for a turn that cannot wait a whole chunk.
    pub fn graph_step(&mut self) -> Result<Progress, LearnRefusal> {
        if let Some(ended) = self.state.ended() {
            return ended;
        }
        let status = unsafe { sys::llama_opt_learn_graph_step(self.learner.ctx.as_ptr()) };
        self.reconciled();
        self.observe(status, "learn_graph_step")
    }

    /// Whether a further gradient chunk remains (restoration needs no new grant).
    pub fn needs_gradient(&mut self) -> bool {
        unsafe { sys::llama_opt_learn_needs_gradient(self.learner.ctx.as_ptr()) }
    }

    /// Stop admitting gradient chunks; later steps only restore the served tail. Steps already
    /// committed stay committed. Call between steps; a repeat is harmless.
    pub fn drain(&mut self) {
        unsafe { sys::llama_opt_learn_drain(self.learner.ctx.as_ptr()) };
    }

    /// Run to the end: drain, then step until the restoration is confirmed or refused. A confirmed
    /// restoration hands the admission back; anything else quarantines the context. Consuming `self`
    /// makes a second finish unrepresentable.
    pub fn finish(mut self) -> Finished {
        match self.run_out() {
            Ok(()) => {
                let ctx = self.learner.ctx.as_ptr();
                let after = unsafe { sys::llama_opt_shadow_steps(ctx, self.shadow.0.as_ptr()) };
                // Her shadow's effects are uncertain if native code says so, or if what it owns no
                // longer matches what was admitted: either way the reply is never reported Learned.
                let quarantined = unsafe { sys::llama_opt_shadow_quarantined(self.shadow.0.as_ptr()) };
                let uncertain = quarantined || self.learner.accounting().is_some();
                let committed = committed_steps(self.steps_before, after);
                let loss = self.loss();
                restored(self.learner.reclaim(), committed, uncertain, loss)
            }
            Err(refusal) => {
                self.learner.quarantine_after(&refusal, self.seq);
                Finished::Failed { refusal, seq: self.seq }
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
        // finish() already reclaimed or quarantined; an unfinished run is run out here.
        let unfinished = self.learner.ctx.learn_callbacks.as_ref().is_some_and(|holder| holder.admission.is_some());
        if unfinished {
            match self.run_out() {
                Ok(()) => drop(self.learner.reclaim()),
                Err(refusal) => self.learner.quarantine_after(&refusal, self.seq),
            }
        }
        unsafe { sys::ggml_opt_result_free(self.result.as_ptr()) };
    }
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

/// An admission already handed back: it admits nothing.
struct Released;

impl ReplyAdmission for Released {
    fn workspace(&self, _: BufferType, _: u64, _: u64) -> bool {
        false
    }
}

impl Optimizer {
    /// AdamW at `learning_rate`, every other parameter ggml's own default: the one owner of those
    /// numbers, which the llama-server learning path used, so a learner never types its betas.
    pub fn adamw(learning_rate: f32) -> Self {
        let defaults = unsafe { sys::ggml_opt_get_default_optimizer_params(std::ptr::null_mut()) }.adamw;
        Optimizer::AdamW { learning_rate, beta1: defaults.beta1, beta2: defaults.beta2, eps: defaults.eps, weight_decay: defaults.wd }
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
/// The steps a reply committed to her shadow: its count after, less its count before. A count that
/// did not rise (or a native count below zero) committed nothing.
fn committed_steps(before: i64, after: i64) -> u64 {
    u64::try_from(after.saturating_sub(before)).unwrap_or_default() // unwrap_or_default: a count that fell is no commit
}

/// What a confirmed restoration was: learned only when steps actually committed, and never when
/// native code marks the shadow's effects uncertain. A loss is reported only for committed steps.
fn restored(admission: Box<dyn ReplyAdmission>, committed: u64, uncertain: bool, loss: Option<f64>) -> Finished {
    if uncertain {
        return Finished::Uncertain { admission, steps: committed };
    }
    match NonZeroU64::new(committed) {
        Some(steps) => Finished::Learned { admission, steps, loss },
        None => Finished::Unchanged { admission },
    }
}

fn native_horizon(horizon: Option<NonZeroU32>) -> u32 {
    const WHOLE_WINDOW: u32 = 0;
    match horizon {
        Some(positions) => positions.get(),
        None => WHOLE_WINDOW,
    }
}

fn shadow_params(shadow: &Shadow, data: *mut c_void) -> sys::llama_opt_params {
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

/// What one visit found native code owning: bytes per shadow and role, and any buffer it could not
/// attribute (an unknown kind).
#[derive(Debug, Default)]
struct Resident {
    rows: Vec<(ShadowKey, Role, u64)>,
    unknown: Option<String>,
}

impl Resident {
    fn bytes(&self, key: ShadowKey, role: Role) -> u64 {
        self.rows.iter().filter(|(k, r, _)| *k == key && *r == role).map(|(_, _, bytes)| *bytes).sum()
    }

    /// A buffer of an unknown kind, or one owned for a shadow this context does not retain.
    fn unattributed<H>(&self, shadows: &Shadows<H>) -> Option<LearnRefusal> {
        if let Some(kind) = &self.unknown {
            return Some(LearnRefusal { step: "accounting", message: format!("native code owns an optimizer buffer this crate cannot attribute: {kind:?}") });
        }
        self.rows.iter().find(|(key, _, bytes)| *bytes > 0 && shadows.get(*key).is_none()).map(|(_, role, bytes)| LearnRefusal {
            step: "accounting",
            message: format!("native code owns {bytes} {role:?} optimizer bytes for a shadow this context does not retain"),
        })
    }
}

unsafe extern "C" fn visited(
    shadow: *const sys::llama_adapter_lora,
    _device: *const c_char,
    _buffer_type: *const c_char,
    kind: *const c_char,
    bytes: usize,
    _host: bool,
    data: *mut c_void,
) {
    // A panic must never unwind into native code.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `data` is the Resident `reconcile` passed for this synchronous visit.
        let resident = unsafe { &mut *data.cast::<Resident>() };
        let (Some(shadow), false) = (NonNull::new(shadow.cast_mut()), kind.is_null()) else { return };
        // SAFETY: `kind` is a NUL-terminated name native code owns for the call.
        let kind = unsafe { CStr::from_ptr(kind) };
        match (Role::of(kind), u64::try_from(bytes)) {
            (Some(role), Ok(bytes)) => resident.rows.push((ShadowKey(shadow), role, bytes)),
            (Some(_), Err(_)) => resident.unknown = Some(format!("{} with a size past u64", kind.to_string_lossy())),
            (None, _) => resident.unknown = Some(kind.to_string_lossy().into_owned()),
        }
    }));
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
    // Optimizer buffers belong to the selected shadow's lease, never to a reply.
    let Some(retained) = holder.selected.and_then(|key| holder.shadows.get(key)) else { return false };
    // A buffer this crate cannot name is refused, never charged under a guessed role.
    let Some(role) = Role::of(kind) else { return false };
    let Ok(bytes) = u64::try_from(bytes) else { return false };
    let slot = Slot { role, device: BufferType(buffer) };
    // A panic must never unwind into native code: it refuses the allocation instead.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| retained.slots.ask(&*retained.lease, slot, bytes)))
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
        assert!(!Released.workspace(BufferType(NonNull::dangling()), 1, 1));
    }

    fn refusal() -> LearnRefusal {
        LearnRefusal { step: "learn_step", message: "device lost".into() }
    }

    // what this catches (Codex on #6148): a native refusal marked as done, so a later finish
    // returned Learned for a run that failed.
    #[test]
    fn a_failure_after_a_step_stays_failed_through_later_steps_and_finish() {
        let mut state = RunState::Running;
        assert!(matches!(state.observe(Status::More, refusal), Ok(Progress::More)));
        assert!(matches!(state.observe(Status::Failed, refusal), Err(r) if r.message == "device lost"));
        assert!(!state.is_running());
        assert!(matches!(state.ended(), Some(Err(r)) if r.step == "learn_step"));
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

    /// Counts the optimizer bytes it admitted and never releases them by itself: its owner does.
    struct Counting(std::sync::Arc<std::sync::atomic::AtomicU64>);

    impl OptimizerLease for Counting {
        fn admit(&self, _: Slot, bytes: u64) -> bool {
            self.0.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
            true
        }
        fn release(&self, _: Slot, bytes: u64) {
            self.0.fetch_sub(bytes, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Admits every workspace check, so a quarantine test sees only where the admission lives.
    struct Open;

    impl ReplyAdmission for Open {
        fn workspace(&self, _: BufferType, _: u64, _: u64) -> bool {
            true
        }
    }

    fn key(n: usize) -> ShadowKey {
        ShadowKey(NonNull::dangling().map_addr(|addr| addr.saturating_add(n * 64)))
    }

    // what this catches (Codex on #6148): a quarantine that let go of the failed run's admission, or
    // a holder that still admitted workspace while quarantined.
    #[test]
    fn a_quarantine_keeps_the_failed_runs_admission_and_admits_nothing() {
        let mut holder = CallbackHolder::empty();
        holder.admission = Some(Box::new(Open));
        let admission = holder.admission.take();
        holder.quarantine = Some(Quarantine { refusal: refusal(), seq: 3, admission });
        assert!(holder.admission.is_none(), "an empty holder refuses every workspace growth");
        assert!(holder.quarantine.as_ref().is_some_and(|q| q.admission.is_some() && q.seq == 3));
    }

    // what this catches (Codex on #6148): switching A -> B -> A, or a refused selection of B, dropping
    // a shadow (its adapter and its optimizer lease) while native code keeps its optimizer, with the
    // caller's own handles gone.
    #[test]
    fn a_switch_or_a_refused_selection_never_drops_a_retained_shadow() {
        let admitted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lease = |bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>| -> Box<dyn OptimizerLease> {
            Box::new(Counting(std::sync::Arc::clone(bytes)))
        };
        let mut shadows: Shadows<Box<dyn OptimizerLease>> = Shadows::new();
        let (a, b) = (key(1), key(2));
        assert!(shadows.retain(a, lease(&admitted)).is_ok());
        let slot = Slot { role: Role::Static, device: BufferType(NonNull::dangling()) };
        assert!(shadows.get(a).is_some_and(|l| l.admit(slot, 64)));
        // B is retained; a selection of B native code refuses changes nothing retained.
        assert!(shadows.retain(b, lease(&admitted)).is_ok());
        // Back to A: retaining it again hands the new lease back; the first, with its bytes, stays.
        assert!(shadows.retain(a, lease(&admitted)).is_err());
        assert!(shadows.get(a).is_some() && shadows.get(b).is_some());
        assert_eq!(admitted.load(std::sync::atomic::Ordering::Relaxed), 64, "A's optimizer bytes stay leased across the switches");
        // Only a confirmed release takes a shadow out.
        assert!(shadows.take(b).is_some());
        assert!(shadows.get(b).is_none() && shadows.get(a).is_some());
    }

    // what this catches (Codex on #6148): a begin then an immediate finish (drained before any
    // gradient) reported as Learned with zero committed updates; a drain after a commit losing the
    // steps it kept; a shadow native code marks uncertain read as learned; a loss on no commit.
    #[test]
    fn learned_only_when_her_shadows_own_count_rose_and_its_effects_are_certain() {
        let admission = || -> Box<dyn ReplyAdmission> { Box::new(Open) };
        assert_eq!(committed_steps(7, 7), 0, "drained before a gradient: nothing committed");
        assert_eq!(committed_steps(7, 9), 2, "a drain after commits keeps those committed");
        assert_eq!(committed_steps(9, 7), 0, "a count that fell is no commit");
        assert!(matches!(restored(admission(), 0, false, Some(1.5)), Finished::Unchanged { .. }));
        assert!(matches!(restored(admission(), 2, false, Some(1.5)), Finished::Learned { steps, loss: Some(_), .. } if steps.get() == 2));
        assert!(matches!(restored(admission(), 2, true, Some(1.5)), Finished::Uncertain { steps: 2, .. }));
        assert!(matches!(restored(admission(), 0, true, None), Finished::Uncertain { steps: 0, .. }));
    }

    // what this catches (Codex on #6148): recovery calling native code to release the workspace while
    // a shadow still holds the scheduler (native refuses: opt_configured), or dropping the
    // quarantine or a retained shadow's lease on the way.
    #[test]
    fn recovery_waits_for_every_retained_shadow_and_keeps_them_until_released() {
        let mut shadows: Shadows<Box<dyn OptimizerLease>> = Shadows::new();
        let admitted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        assert!(shadows.retain(key(1), Box::new(Counting(std::sync::Arc::clone(&admitted)))).is_ok());
        assert!(shadows.retain(key(2), Box::new(Counting(admitted))).is_ok());
        assert_eq!(shadows.len(), 2, "recover refuses before any native call while this is nonzero");
        assert!(shadows.take(key(1)).is_some());
        assert_eq!(shadows.len(), 1, "one release leaves the other shadow, and recovery, waiting");
        assert!(shadows.take(key(2)).is_some());
        assert_eq!(shadows.len(), 0);
    }

    // what this catches (Codex on #6149): a visit's buffer of an unknown kind, or owned for a shadow
    // this context does not retain, absorbed quietly instead of an accounting defect; and one
    // shadow's roles summed into another's.
    #[test]
    fn a_visit_attributes_bytes_by_shadow_and_role_or_names_the_defect() {
        let mut shadows: Shadows<()> = Shadows::new();
        assert!(shadows.retain(key(1), ()).is_ok());
        let resident = Resident { rows: vec![(key(1), Role::Static, 270_368), (key(1), Role::Host, 32)], unknown: None };
        assert_eq!((resident.bytes(key(1), Role::Static), resident.bytes(key(1), Role::Host)), (270_368, 32));
        assert!(resident.unattributed(&shadows).is_none());
        let stray = Resident { rows: vec![(key(2), Role::Host, 32)], unknown: None };
        assert!(stray.unattributed(&shadows).is_some_and(|r| r.step == "accounting"));
        let unknown = Resident { rows: Vec::new(), unknown: Some("optimizer_moments".into()) };
        assert!(unknown.unattributed(&shadows).is_some_and(|r| r.message.contains("optimizer_moments")));
    }

    // what this catches: the native "whole window" encoding leaking past the boundary, or a real
    // horizon rewritten.
    #[test]
    fn the_whole_window_is_llamas_zero_only_at_the_boundary() {
        assert_eq!(native_horizon(None), 0);
        assert_eq!(native_horizon(NonZeroU32::new(512)), 512);
    }
}
