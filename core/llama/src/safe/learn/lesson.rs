//! A lesson taken as her ordinary decode: her shadow selected as the context's trainable adapter,
//! labelled rows in her batches, then concluded. The context's holder keeps the open lesson's record
//! (which shadow, its step count at the start), so the lesson is bound to the context that opened it,
//! its shadow stays pinned until it ends, and its owner can settle it even if the handle is gone.

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// A lesson taken as her ordinary decode ([`Learner::select`]). It borrows nothing, so her labelled
/// decodes run on the context in between; [`Learner::conclude`] ends it and hands the admission
/// back. If the handle is lost, [`Learner::abandon`] settles the same lesson from the context.
#[must_use = "a lesson is concluded explicitly, which hands its admission back"]
#[derive(Debug)]
pub struct Lesson {
    binding: Binding,
}

/// Which context lesson a [`Lesson`] is: the holder that issued it and its generation there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    holder: u64,
    generation: u64,
}

/// The open lesson's record on its context: the shadow it trains (pinned until it ends) and that
/// shadow's native step count when it began.
#[derive(Debug, Clone, Copy)]
pub(super) struct Open {
    generation: u64,
    pub(super) shadow: ShadowKey,
    pub(super) steps_before: i64,
}

/// The lessons one context has opened: the holder's identity, the next generation, and the one open
/// now (its admission is the holder's). At most one is open: a second would overwrite the first's
/// admission.
pub(in crate::safe) struct Lessons {
    holder: u64,
    next: u64,
    open: Option<Open>,
}

impl Lessons {
    pub(in crate::safe) fn new() -> Self {
        static HOLDERS: AtomicU64 = AtomicU64::new(1);
        Self { holder: HOLDERS.fetch_add(1, Ordering::Relaxed), next: 1, open: None }
    }

    fn open(&mut self, shadow: ShadowKey, steps_before: i64) -> Binding {
        let generation = self.next;
        self.next += 1;
        self.open = Some(Open { generation, shadow, steps_before });
        Binding { holder: self.holder, generation }
    }

    fn holds(&self, binding: Binding) -> bool {
        binding.holder == self.holder && self.open.is_some_and(|open| open.generation == binding.generation)
    }

    /// The open lesson's record, if one is open.
    pub(super) fn current(&self) -> Option<Open> {
        self.open
    }

    /// Whether `shadow` is the one an open lesson trains: it may not be released until the lesson ends.
    pub(super) fn pins(&self, shadow: ShadowKey) -> bool {
        self.open.is_some_and(|open| open.shadow == shadow)
    }

    /// The open lesson's admission has left the holder (concluded, reclaimed or quarantined).
    pub(in crate::safe) fn close(&mut self) {
        self.open = None;
    }
}

/// A lesson [`Learner::conclude`] did not issue on this context's open lesson: a lesson from another
/// context, or one already settled. Nothing was counted and no admission moved; the lesson is handed
/// back.
#[derive(Debug, thiserror::Error)]
#[error("this lesson is not the one open on this context: nothing counted, no admission returned")]
pub struct ForeignLesson {
    pub lesson: Lesson,
}

impl<'c, 'm> Learner<'c, 'm> {
    /// Make her shadow the context's trainable adapter under `admission`, for a lesson taken as her
    /// ordinary decode: labelled rows in a [`Batch::labelled`] then train it in the same graph as
    /// everything else. Returns the lesson as an owned value that borrows nothing, so the context is
    /// free for her decodes; [`Learner::conclude`] ends it. Refused, with the admission handed back,
    /// while another lesson or reply is open on the context (settle a lost one with
    /// [`Learner::abandon`]), while the context is quarantined or its accounting untrusted, on a
    /// pooled context, or for a shadow not retained here.
    pub fn select(&mut self, shadow: &Shadow, admission: Box<dyn ReplyAdmission>) -> Result<Lesson, Refused> {
        if let Some(refusal) = self.quarantine().or(self.accounting()) {
            return Err(Refused { refusal: refusal.clone(), admission });
        }
        if self.holder().admission.is_some() {
            let refusal = LearnRefusal { step: "select_shadow", message: "a lesson or reply is already open on this context: conclude or abandon it first".into() };
            return Err(Refused { refusal, admission });
        }
        // A lesson trains on per-token logits: a pooled (embedding) context has none, and native
        // code would refuse only at the first labelled step (Codex's BigMama replay at 7f8462502).
        if unsafe { sys::llama_pooling_type(self.ctx.as_ptr()) } != sys::llama_pooling_type_LLAMA_POOLING_TYPE_NONE {
            let refusal = LearnRefusal { step: "context", message: "a pooled context cannot learn: create it with PoolingType::None".into() };
            return Err(Refused { refusal, admission });
        }
        let key = ShadowKey::of(&shadow.adapter);
        if self.holder().shadows.get(key).is_none() {
            let refusal = LearnRefusal { step: "select_shadow", message: "the shadow is not retained on this context".into() };
            return Err(Refused { refusal, admission });
        }
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
            let refusal = self.refusal("select_shadow");
            return Err(Refused { refusal, admission: self.reclaim() });
        }
        // Her shadow's own count before the lesson: what it committed is measured against this.
        let steps_before = unsafe { sys::llama_opt_shadow_steps(self.ctx.as_ptr(), key.0.as_ptr()) };
        Ok(Lesson { binding: self.holder().lessons.open(key, steps_before) })
    }

    /// End a lesson taken through [`Learner::select`], verified as this context's open lesson first.
    /// A foreign or already settled lesson changes nothing.
    pub fn conclude(&mut self, lesson: Lesson) -> Result<Finished, ForeignLesson> {
        if !self.holder().lessons.holds(lesson.binding) {
            return Err(ForeignLesson { lesson });
        }
        Ok(self.settle())
    }

    /// Settle the open lesson whose handle was lost, as its owner: the same accounting as
    /// [`Learner::conclude`], from the context's own record, so exactly one admission comes back and
    /// its committed steps are counted. `None` when no lesson is open.
    pub fn abandon(&mut self) -> Option<Finished> {
        self.holder().lessons.current().map(|_| self.settle())
    }

    /// Native code's buffers are reconciled, the shadow's committed steps are counted from its own
    /// native count, and the admission comes back. Effects native code marks uncertain, or untrusted
    /// accounting, are never `Learned`. Only called with a lesson open.
    fn settle(&mut self) -> Finished {
        let Some(open) = self.holder().lessons.current() else {
            return Finished::Unchanged { admission: self.reclaim() };
        };
        self.reconcile();
        let ctx = self.ctx.as_ptr();
        let after = unsafe { sys::llama_opt_shadow_steps(ctx, open.shadow.0.as_ptr()) };
        let quarantined = unsafe { sys::llama_opt_shadow_quarantined(open.shadow.0.as_ptr()) };
        let uncertain = quarantined || self.accounting().is_some();
        restored(self.reclaim(), committed_steps(open.steps_before, after), uncertain, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(at: usize) -> ShadowKey {
        ShadowKey(NonNull::new(at as *mut sys::llama_adapter_lora).expect("a non-null fixture key"))
    }

    // what this catches: a lesson concluding on a context that did not issue it, or a settled
    // lesson's binding still honoured after a new lesson opened (counting another lesson's steps
    // and handing back its admission).
    #[test]
    fn a_lesson_concludes_only_itself() {
        let (mut here, mut there) = (Lessons::new(), Lessons::new());
        let first = here.open(key(8), 0);
        assert!(here.holds(first));
        assert!(!there.holds(first), "a lesson from another context");
        here.close();
        assert!(!here.holds(first), "a settled lesson");
        let second = here.open(key(8), 3);
        assert!(!here.holds(first) && here.holds(second), "a stale lesson after a new one opened");
        let elsewhere = there.open(key(8), 0);
        assert!(!here.holds(elsewhere), "same generation, another context");
    }

    // what this catches: the shadow an open lesson trains released under it (its steps then counted
    // against a freed key), or an unrelated shadow held hostage by the lesson.
    #[test]
    fn only_the_open_lessons_shadow_is_pinned() {
        let mut lessons = Lessons::new();
        assert!(!lessons.pins(key(8)), "nothing open");
        let _ = lessons.open(key(8), 0);
        assert!(lessons.pins(key(8)) && !lessons.pins(key(16)));
        lessons.close();
        assert!(!lessons.pins(key(8)), "released once the lesson ends");
    }
}
