//! A lesson taken as her ordinary decode: her shadow selected as the context's trainable adapter,
//! labelled rows in her batches, then concluded. A lesson is bound to the context and the
//! admission that opened it, so it can only ever count its own steps and hand back its own admission.

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// A lesson taken as her ordinary decode ([`Learner::select`]): which shadow trains, its native step
/// count when the lesson began, and the context lesson it is bound to. It borrows nothing, so her
/// labelled decodes run on the context in between; [`Learner::conclude`] ends it and hands the
/// admission back.
#[must_use = "a lesson is concluded explicitly, which hands its admission back"]
#[derive(Debug)]
pub struct Lesson {
    pub(super) shadow: ShadowKey,
    pub(super) steps_before: i64,
    binding: Binding,
}

/// Which context lesson a [`Lesson`] is: the holder that issued it and its generation there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    holder: u64,
    generation: u64,
}

/// The lessons one context has opened: the holder's identity, the next generation, and the one open
/// now (its admission is the holder's). At most one is open: a second would overwrite the first's
/// admission.
pub(in crate::safe) struct Lessons {
    holder: u64,
    next: u64,
    open: Option<u64>,
}

impl Lessons {
    pub(in crate::safe) fn new() -> Self {
        static HOLDERS: AtomicU64 = AtomicU64::new(1);
        Self { holder: HOLDERS.fetch_add(1, Ordering::Relaxed), next: 1, open: None }
    }

    fn open(&mut self) -> Binding {
        let generation = self.next;
        self.next += 1;
        self.open = Some(generation);
        Binding { holder: self.holder, generation }
    }

    fn holds(&self, binding: Binding) -> bool {
        binding.holder == self.holder && self.open == Some(binding.generation)
    }

    /// The open lesson's admission has left the holder (concluded, reclaimed or quarantined).
    pub(in crate::safe) fn close(&mut self) {
        self.open = None;
    }
}

/// A lesson [`Learner::conclude`] did not issue on this context's open lesson: a lesson from another
/// context, or one already concluded. Nothing was counted and no admission moved; the lesson is
/// handed back.
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
    /// while another lesson or reply is open on the context (an abandoned lesson shows here, never
    /// silently replaced), while the context is quarantined or its accounting untrusted, on a pooled
    /// context, or for a shadow not retained here.
    pub fn select(&mut self, shadow: &Shadow, admission: Box<dyn ReplyAdmission>) -> Result<Lesson, Refused> {
        if let Some(refusal) = self.quarantine().or(self.accounting()) {
            return Err(Refused { refusal: refusal.clone(), admission });
        }
        if self.holder().admission.is_some() {
            let refusal = LearnRefusal { step: "select_shadow", message: "a lesson or reply is already open on this context: conclude it first".into() };
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
        let (data, binding) = {
            let holder = self.holder();
            holder.optimizer = optimizer_params(shadow.optimizer);
            holder.admission = Some(admission);
            holder.selected = Some(key);
            let binding = holder.lessons.open();
            (std::ptr::from_mut(holder).cast::<c_void>(), binding)
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
        Ok(Lesson { shadow: key, steps_before, binding })
    }

    /// End a lesson taken through [`Learner::select`]: verified as this context's open lesson first,
    /// then native code's buffers are reconciled, her shadow's committed steps are counted from its own
    /// native count, and the admission comes back. Effects native code marks uncertain, or untrusted
    /// accounting, are never `Learned`. A foreign or already concluded lesson changes nothing.
    pub fn conclude(&mut self, lesson: Lesson) -> Result<Finished, ForeignLesson> {
        if !self.holder().lessons.holds(lesson.binding) {
            return Err(ForeignLesson { lesson });
        }
        self.reconcile();
        let ctx = self.ctx.as_ptr();
        let after = unsafe { sys::llama_opt_shadow_steps(ctx, lesson.shadow.0.as_ptr()) };
        let quarantined = unsafe { sys::llama_opt_shadow_quarantined(lesson.shadow.0.as_ptr()) };
        let uncertain = quarantined || self.accounting().is_some();
        Ok(restored(self.reclaim(), committed_steps(lesson.steps_before, after), uncertain, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a lesson concluding on a context that did not issue it, or a concluded
    // lesson's binding still honoured after a new lesson opened (counting another lesson's steps
    // and handing back its admission).
    #[test]
    fn a_lesson_concludes_only_itself() {
        let (mut here, mut there) = (Lessons::new(), Lessons::new());
        let first = here.open();
        assert!(here.holds(first));
        assert!(!there.holds(first), "a lesson from another context");
        here.close();
        assert!(!here.holds(first), "a concluded lesson");
        let second = here.open();
        assert!(!here.holds(first) && here.holds(second), "a stale lesson after a new one opened");
        let elsewhere = there.open();
        assert!(!here.holds(elsewhere), "same generation, another context");
    }
}
