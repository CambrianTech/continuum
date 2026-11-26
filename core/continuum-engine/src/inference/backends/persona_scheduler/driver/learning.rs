//! The driver's learning (IN-PROCESS-LEARNING.md, L2): what the trainer told, worked on the engine's
//! one thread between her turns, never during one. Every outcome is said on `learning/said`; nothing
//! waits on an answer.
//!
//! A seat goes Measure (a dry run: nothing allocated) → Load (its weights, then its optimizer and
//! workspace measured) → Seat (retained under the optimizer lease) → Learn / Checkpoint → Release.
//! A lesson binds her and learns on her own sequence from the longest prefix she holds, asking
//! between chunks whether a turn is waiting; one waiting drains it, so a turn waits at most one chunk.

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU32;
use std::sync::Arc;

use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::turn::common_prefix;
use continuum_persona_llama::LearnReply;
use llama::{Finished, FreshGeometry, Label, Optimizer, ReplyAdmission, Shadow, Walk};

use super::Driver;
use crate::inference::backends::persona_scheduler::lesson::Rendered;
use crate::inference::learning::{
    say, GenerationOutcome, Learning as Told, OptimizerStart, Said, Seat, SeatId, SeatState, ShadowSource, StepId, StepOutcome,
};

/// What reaches the driver: every verb as told, except a lesson, which arrives rendered.
pub(crate) enum Job {
    /// Load, Seat, Checkpoint or Release, as the trainer told it.
    Told(Told),
    /// A lesson rendered with her template on the submitter's thread.
    Learn(Lesson),
    /// A Measure, its lesson rendered on the submitter's thread into the rows it is sized for.
    Measure(Measuring),
    /// A Measure whose lesson could not be rendered: the driver says whether that seat already holds
    /// anything (Kept) or never existed (Refused), never the port, which cannot see the seats (Codex).
    Unmeasured { seat: SeatId, why: Arc<str> },
}

/// A seat to measure, its lesson rendered into the rows its workspace is sized for.
pub(crate) struct Measuring {
    pub(crate) seat: SeatId,
    pub(crate) mind: PersonaId,
    pub(crate) shadow: ShadowSource,
    pub(crate) optimizer: Optimizer,
    pub(crate) reply_rows: NonZeroU32,
}

/// One lesson for a seat, rendered, with its workspace admission view.
pub(crate) struct Lesson {
    pub(crate) seat: SeatId,
    pub(crate) step: StepId,
    pub(crate) rendered: Rendered,
    pub(crate) admission: Box<dyn ReplyAdmission>,
}

impl Job {
    /// Say this job refused for `why`, nothing having been done.
    pub(crate) fn refuse(self, why: &str) {
        let why: Arc<str> = Arc::from(why);
        match self {
            Job::Learn(Lesson { seat, step, .. }) => say(Said::Step { seat, step, outcome: StepOutcome::Refused { why } }),
            Job::Measure(Measuring { seat, .. }) => say(Said::Seat { seat, state: SeatState::Refused { why } }),
            Job::Unmeasured { seat, .. } => say(Said::Seat { seat, state: SeatState::Refused { why } }),
            Job::Told(Told::Learn(learn)) => say(Said::Step { seat: learn.seat, step: learn.step, outcome: StepOutcome::Refused { why } }),
            Job::Told(Told::Measure(measure)) => say(Said::Seat { seat: measure.seat, state: SeatState::Refused { why } }),
            // A seat told to load, seat or release already holds what it holds: kept, never ended.
            Job::Told(Told::Load { seat }) => say(Said::Seat { seat, state: SeatState::Kept { why } }),
            Job::Told(Told::Seat(seated)) => say(Said::Seat { seat: seated.seat, state: SeatState::Kept { why } }),
            Job::Told(Told::Checkpoint { seat, .. }) => say(Said::Generation { seat, outcome: GenerationOutcome::Refused { why } }),
            Job::Told(Told::Release { seat }) => say(Said::Seat { seat, state: SeatState::Kept { why } }),
        }
    }
}

/// The learning the driver holds: its seats and what waits to be done.
#[derive(Default)]
pub(super) struct Learning {
    seats: HashMap<SeatId, Entry>,
    waiting: VecDeque<Job>,
}

impl Learning {
    pub(super) fn queue(&mut self, job: Job) {
        self.waiting.push_back(job);
    }

    /// How many jobs wait: the learning backlog the lane counts in the background class.
    pub(super) fn waiting_jobs(&self) -> usize {
        self.waiting.len()
    }

    pub(super) fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Everything still waiting, refused for `why` (the engine is stopping or closed).
    pub(super) fn refuse_waiting(&mut self, why: &str) {
        let waiting: Vec<Job> = self.waiting.drain(..).collect();
        for job in waiting {
            match job {
                // A measure for a seat that already holds something leaves it as it was.
                Job::Measure(Measuring { seat, .. }) | Job::Unmeasured { seat, .. } if self.seats.contains_key(&seat) => {
                    say(Said::Seat { seat, state: SeatState::Kept { why: why.into() } });
                }
                job => job.refuse(why),
            }
        }
    }
}

/// One seat: whose, how it learns, how long a lesson it is sized for, and how far along it is.
struct Entry {
    mind: PersonaId,
    optimizer: Optimizer,
    reply_rows: NonZeroU32,
    phase: Phase,
}

enum Phase {
    Measured { source: ShadowSource },
    Loaded { shadow: Shadow },
    Seated { shadow: Shadow },
    /// Held until a release proves it gone: a detach, a reply's restoration or a release was not
    /// confirmed, so the shadow may still be attached and, when `retained`, holds an optimizer.
    Quarantined { shadow: Shadow, retained: bool },
}

fn mind_of(persona: PersonaId) -> MindId {
    MindId { persona, epoch: Epoch(1) }
}

impl Driver<'_, '_> {
    /// Work the oldest waiting job. `go_on` says whether no turn waits.
    pub(super) fn learn_next(&mut self, go_on: &mut dyn FnMut() -> bool) {
        // timed whole: the lane's background class counts it as service and learns a job's cost
        let started = std::time::Instant::now();
        self.work_next(go_on);
        self.lane.learned(started.elapsed());
    }

    fn work_next(&mut self, go_on: &mut dyn FnMut() -> bool) {
        let Some(job) = self.learning.waiting.pop_front() else { return };
        match job {
            Job::Measure(sized) => self.measure(sized),
            Job::Unmeasured { seat, why } => {
                let state = if self.learning.seats.contains_key(&seat) { SeatState::Kept { why } } else { SeatState::Refused { why } };
                say(Said::Seat { seat, state });
            }
            told @ Job::Told(Told::Measure(_)) => told.refuse("the measure reached the engine unrendered"),
            Job::Told(Told::Load { seat }) => self.load(seat),
            Job::Told(Told::Seat(seated)) => self.seat(seated),
            Job::Told(Told::Checkpoint { seat, into, binding }) => self.checkpoint(seat, &into, &binding),
            Job::Told(Told::Release { seat }) => self.release(seat),
            Job::Learn(lesson) => self.lesson(lesson, go_on),
            // A lesson is rendered before it is queued; one that arrives unrendered is refused.
            told @ Job::Told(Told::Learn(_)) => told.refuse("the lesson reached the engine unrendered"),
        }
    }

    /// Every seat ends with the engine, through the same release a trainer tells: detached,
    /// its optimizer released, its last reference dropped, then said Released. One native code keeps
    /// stays quarantined, its guards kept, and goes with the context.
    pub(super) fn end_learning(&mut self, why: &str) {
        self.learning.refuse_waiting(why);
        let seats: Vec<SeatId> = self.learning.seats.keys().copied().collect();
        for seat in seats {
            self.release(seat);
        }
    }

    /// A command this seat cannot take now: it stays as it was.
    fn keep(&mut self, seat: SeatId, entry: Entry, why: &str) {
        self.learning.seats.insert(seat, entry);
        say(Said::Seat { seat, state: SeatState::Kept { why: why.into() } });
    }

    fn measure(&mut self, sized: Measuring) {
        let Measuring { seat, mind, shadow, optimizer, reply_rows } = sized;
        if self.learning.seats.contains_key(&seat) {
            return say(Said::Seat { seat, state: SeatState::Kept { why: "the seat is already measured".into() } });
        }
        let weights = match &shadow {
            ShadowSource::Fresh(fresh) => self.model.measure_fresh_shadow(&FreshGeometry {
                rank: fresh.rank,
                alpha: fresh.alpha,
                targets: &fresh.targets,
                seed: fresh.seed,
                top_layers: fresh.top_layers,
            }),
            ShadowSource::Generation(dir) => self.model.measure_adapter(&dir.join("adapter.gguf")),
        };
        match weights {
            Ok(weights) => {
                self.learning.seats.insert(seat, Entry { mind, optimizer, reply_rows, phase: Phase::Measured { source: shadow } });
                say(Said::Seat { seat, state: SeatState::Measured { weights } });
            }
            Err(refusal) => say(Said::Seat { seat, state: SeatState::Refused { why: refusal.to_string().into() } }),
        }
    }

    fn load(&mut self, seat: SeatId) {
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return say(Said::Seat { seat, state: SeatState::Refused { why: "load of a seat never measured".into() } });
        };
        let source = match entry.phase {
            Phase::Measured { source } => source,
            phase => return self.keep(seat, Entry { phase, ..entry }, "load of a seat past measuring"),
        };
        let loaded = match &source {
            // Written where nothing else reads it, loaded, and removed: the shadow lives in memory, and
            // only a checkpoint makes it durable.
            ShadowSource::Fresh(fresh) => {
                let path = std::env::temp_dir().join(format!("continuum-shadow-{}.gguf", seat.0));
                let geometry = FreshGeometry { rank: fresh.rank, alpha: fresh.alpha, targets: &fresh.targets, seed: fresh.seed, top_layers: fresh.top_layers };
                if let Err(refusal) = self.model.write_fresh_shadow(&path, &geometry) {
                    return say(Said::Seat { seat, state: SeatState::Refused { why: refusal.to_string().into() } });
                }
                let loaded = self.model.load_lora(&path);
                if let Err(kept) = std::fs::remove_file(&path) {
                    continuum_base::probe!(class = "learning.shadow_file.kept", path = %path.display(), error = %kept, "the fresh shadow's scratch file stays on disk");
                }
                loaded
            }
            ShadowSource::Generation(dir) => self.model.load_lora(dir.join("adapter.gguf")),
        };
        let shadow = match loaded {
            Ok(adapter) => Shadow { adapter: Arc::new(adapter), optimizer: entry.optimizer, recompute: false, walk: Walk::default() },
            Err(why) => return say(Said::Seat { seat, state: SeatState::Refused { why: why.into() } }),
        };
        let mind = mind_of(entry.mind);
        let measured = self.engine.attach_shadow(mind, &shadow).and_then(|()| self.engine.measure_shadow(mind, &shadow, entry.reply_rows));
        // Off the context before the shadow can be dropped; if that is not confirmed, the engine
        // holds it until a release proves it off.
        if let Err(attached) = self.engine.detach_shadow(mind) {
            self.learning.seats.insert(seat, Entry { phase: Phase::Quarantined { shadow, retained: false }, ..entry });
            return say(Said::Seat { seat, state: SeatState::Quarantined { why: attached.to_string().into() } });
        }
        match measured {
            Ok((optimizer, workspace)) => {
                let reply_rows = entry.reply_rows;
                self.learning.seats.insert(seat, Entry { phase: Phase::Loaded { shadow }, ..entry });
                say(Said::Seat { seat, state: SeatState::Loaded { optimizer, workspace, reply_rows } });
            }
            Err(refusal) => {
                // Detached: the last reference goes before the trainer hears nothing remains.
                drop(shadow);
                say(Said::Seat { seat, state: SeatState::Refused { why: refusal.to_string().into() } });
            }
        }
    }

    fn seat(&mut self, seated: Seat) {
        let Seat { seat, lease } = seated;
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return say(Said::Seat { seat, state: SeatState::Refused { why: "seat of a seat never loaded".into() } });
        };
        let shadow = match entry.phase {
            Phase::Loaded { shadow } => shadow,
            phase => return self.keep(seat, Entry { phase, ..entry }, "seat of a seat not loaded"),
        };
        match self.engine.retain_shadow(&shadow, lease) {
            Ok(()) => {
                self.learning.seats.insert(seat, Entry { phase: Phase::Seated { shadow }, ..entry });
                say(Said::Seat { seat, state: SeatState::Seated { optimizer: OptimizerStart::Cold } });
            }
            Err(_lease) => self.keep(seat, Entry { phase: Phase::Loaded { shadow }, ..entry }, "native code refused to retain the shadow"),
        }
    }

    fn checkpoint(&mut self, seat: SeatId, into: &std::path::Path, binding: &str) {
        let outcome = match self.learning.seats.get(&seat) {
            Some(Entry { mind, phase: Phase::Seated { shadow }, .. }) => match self.engine.save_shadow(mind_of(*mind), shadow, into, binding) {
                Ok(steps) => GenerationOutcome::Saved { into: into.to_path_buf(), steps },
                Err(refusal) => GenerationOutcome::Refused { why: refusal.to_string().into() },
            },
            _ => GenerationOutcome::Refused { why: "checkpoint of a seat not seated".into() },
        };
        say(Said::Generation { seat, outcome });
    }

    fn release(&mut self, seat: SeatId) {
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return say(Said::Seat { seat, state: SeatState::Released });
        };
        let mind = mind_of(entry.mind);
        let (shadow, retained) = match entry.phase {
            Phase::Measured { .. } => (None, false),
            // Detached since its measure: dropping it frees its weights.
            Phase::Loaded { shadow } => (Some(shadow), false),
            Phase::Seated { shadow } => (Some(shadow), true),
            Phase::Quarantined { shadow, retained } => {
                if let Err(attached) = self.engine.detach_shadow(mind) {
                    self.learning.seats.insert(seat, Entry { phase: Phase::Quarantined { shadow, retained }, ..entry });
                    return say(Said::Seat { seat, state: SeatState::Quarantined { why: attached.to_string().into() } });
                }
                (Some(shadow), retained)
            }
        };
        let released = match (&shadow, retained) {
            // The lease view drops here; the trainer releases its guards on hearing Released.
            (Some(held), true) => self.engine.release_shadow(mind, held).map(drop),
            _ => Ok(()),
        };
        if let Err(refusal) = released {
            if let Some(shadow) = shadow {
                self.learning.seats.insert(seat, Entry { phase: Phase::Quarantined { shadow, retained: true }, ..entry });
            }
            return say(Said::Seat { seat, state: SeatState::Quarantined { why: refusal.to_string().into() } });
        }
        // The last native reference goes before the trainer hears nothing remains.
        drop(shadow);
        say(Said::Seat { seat, state: SeatState::Released });
    }

    fn lesson(&mut self, lesson: Lesson, go_on: &mut dyn FnMut() -> bool) {
        let Lesson { seat, step, rendered, admission } = lesson;
        let refused = |why: String| say(Said::Step { seat, step, outcome: StepOutcome::Refused { why: why.into() } });
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return refused("a lesson for a seat never seated".into());
        };
        let Entry { mind, optimizer, reply_rows, phase } = entry;
        let shadow = match phase {
            Phase::Seated { shadow } => shadow,
            other => {
                self.learning.seats.insert(seat, Entry { mind, optimizer, reply_rows, phase: other });
                return refused("a lesson for a seat not seated".into());
            }
        };
        // Taken out while she learns (binding her needs the whole driver); put back whatever happens.
        let (outcome, quarantined) = self.learn_seated(Seated { mind, reply_rows, shadow: &shadow, rendered: &rendered }, admission, go_on);
        let phase = if quarantined.is_some() { Phase::Quarantined { shadow, retained: true } } else { Phase::Seated { shadow } };
        self.learning.seats.insert(seat, Entry { mind, optimizer, reply_rows, phase });
        if let Some(why) = quarantined {
            say(Said::Seat { seat, state: SeatState::Quarantined { why } });
        }
        say(Said::Step { seat, step, outcome });
    }

    /// Bind her, attach her shadow, learn one rendered lesson, and take the shadow off: the step's
    /// outcome, and why the seat is quarantined when native code could not confirm her restoration or
    /// the shadow's detach.
    fn learn_seated(&mut self, seated: Seated<'_>, admission: Box<dyn ReplyAdmission>, go_on: &mut dyn FnMut() -> bool) -> (StepOutcome, Option<Arc<str>>) {
        let refused = |why: String| (StepOutcome::Refused { why: why.into() }, None);
        if let Err(unbound) = self.switch_to(seated.mind, false) {
            return refused(format!("she could not be bound to learn: {unbound}"));
        }
        let mind = mind_of(seated.mind);
        // native learning begins only when her state ends at the lesson's context: what she holds of it
        // is reused, the rest prefilled forward-only (no shadow, no gradient)
        if let Err(why) = self.bring_to_lesson_context(seated.rendered) {
            return refused(format!("she could not be brought to the lesson's context: {why}"));
        }
        // her history read where it lies (130k tokens is 520 KB): never cloned to be read; it now ends at
        // the lesson's context, so the reply is her reply rows alone and restores no tail (her next turn
        // rewinds to the checkpoint kept where her own history and the lesson part)
        let reply = match self.engine.active_mut() {
            Some(her) => reply_for(mind, &her.parts().tokens.held, seated.rendered),
            None => return refused("she was not bound after reaching the lesson's context".into()),
        };
        let Some(reply) = reply else {
            return refused("the lesson has no reply past its context".into());
        };
        if reply.tokens.len() > seated.reply_rows.get() as usize {
            if let Some(need) = u32::try_from(reply.tokens.len()).ok().and_then(NonZeroU32::new) {
                return (StepOutcome::TooLong { need }, None);
            }
            return refused(format!("{} reply tokens past the {} the seat was measured for", reply.tokens.len(), seated.reply_rows));
        }
        let learned = match self.engine.attach_shadow(mind, seated.shadow) {
            Ok(()) => match self.engine.learn_active(seated.shadow, &reply, admission, go_on) {
                Ok(Finished::Learned { steps, loss, .. }) => (StepOutcome::Learned { steps, loss }, None),
                Ok(Finished::Unchanged { .. }) => (StepOutcome::Unchanged, None),
                Ok(Finished::Uncertain { steps, .. }) => (StepOutcome::Uncertain { steps }, None),
                Ok(Finished::Failed { refusal, .. }) => {
                    (StepOutcome::Refused { why: "her restoration was not confirmed: the seat is quarantined".into() }, Some(refusal.to_string().into()))
                }
                Err((refusal, _admission)) => refused(refusal.to_string()),
            },
            Err(refusal) => refused(refusal.to_string()),
        };
        match self.engine.detach_shadow(mind) {
            Ok(()) => learned,
            Err(attached) => (learned.0, Some(attached.to_string().into())),
        }
    }
}

impl Driver<'_, '_> {
    /// Her lesson's context, forward only: what she does not already hold of it is prefilled into her
    /// KV under her own genes with no shadow attached (plain KV, no gradient), after a checkpoint where
    /// her own history and the lesson part, so her next turn rewinds there. The step then trains only
    /// her reply against that KV, so its workspace is her reply against her context, never the whole
    /// lesson from scratch (the 5090, 2026-11-26: seats quoted 244 GB to 48.8 TB of workspace, a
    /// 100k-token lesson measured as a 100k-row training graph). Gradients do not reach the context's
    /// own keys and values: the frozen-prefix approximation that makes learning a small addition.
    fn bring_to_lesson_context(&mut self, rendered: &Rendered) -> Result<(), String> {
        let last_context = rendered.reply_from.checked_sub(1).ok_or("the lesson has no context before its reply")?;
        let her = self.engine.active_mut().ok_or("she is not bound")?;
        let shared = common_prefix(&her.parts().tokens.held, &rendered.tokens).min(last_context);
        let genes = her.parts().genome.served_genes().ok_or("her genes were not served")?;
        let basis = her.parts().genome.clone();
        self.reposition_to_served_prefix(shared).map_err(|why| format!("she could not be brought to her held prefix: {why}"))?;
        if shared == last_context {
            return Ok(());
        }
        // what she holds past the shared prefix is not the lesson's: her history ends there
        self.record(|her| her.tokens.cut(shared)).map_err(|why| why.to_string())?;
        // a resume point where her history and the lesson part, for every model (a rewind needs one):
        // her KV must end exactly there when it is taken (the native rewind checks it). A hybrid's state
        // stands there after reposition; an attention-only mind's KV still runs past it, so her last
        // shared token is decoded again at its own position, which truncates her KV there (one token).
        if shared > 0 {
            if !self.model.has_recurrent_state() {
                let last = *rendered.tokens.get(shared - 1).ok_or("the shared prefix is past the lesson")?;
                let at = llama::Pos::try_from(shared - 1).map_err(|_| "a prefix past what a sequence can address".to_string())?;
                self.engine
                    .submit(continuum_persona_llama::Step { tokens: continuum_persona_llama::Tokens::One(last), from: at, logits_at_end: false, genes: Arc::clone(&genes) })
                    .map_err(|why| why.to_string())?;
                self.engine.settle().map_err(|why| why.to_string())?;
            }
            let history = &mut self.engine.active_mut().ok_or("she is not bound")?.parts_mut().tokens;
            if let Some(mut blocks) = history.capture_buffer(super::history::Boundary::Learned, shared) {
                if matches!(self.engine.checkpoint_active(&mut blocks), Ok(pos) if pos as usize + 1 == shared) {
                    self.record(|her| her.tokens.save(super::history::Boundary::Learned, blocks, shared, &basis)).map_err(|why| why.to_string())?;
                }
            }
        }
        let context: Arc<[i32]> = Arc::from(rendered.tokens.get(..last_context).ok_or("the lesson ends before its reply")?);
        let mut pos = shared;
        while pos < last_context {
            let end = last_context.min(pos + self.batch.get());
            let at = llama::Pos::try_from(pos).map_err(|_| "a lesson longer than a sequence can address".to_string())?;
            self.engine
                .submit(continuum_persona_llama::Step { tokens: continuum_persona_llama::Tokens::Span { of: Arc::clone(&context), from: pos, to: end }, from: at, logits_at_end: false, genes: Arc::clone(&genes) })
                .map_err(|why| why.to_string())?;
            self.engine.settle().map_err(|why| why.to_string())?;
            self.record(|her| her.tokens.held.extend_from_slice(&context[pos..end])).map_err(|why| why.to_string())?;
            pos = end;
        }
        Ok(())
    }
}

/// A seated lesson's inputs: whose, how long a lesson it was measured for, its shadow and its tokens.
struct Seated<'a> {
    mind: PersonaId,
    reply_rows: NonZeroU32,
    shadow: &'a Shadow,
    rendered: &'a Rendered,
}

/// Her reply on her own sequence: from the longest prefix she holds of the lesson (never past the
/// token before her reply, whose label is the reply's first token), the lesson's tokens with a label
/// on every one whose next token is hers, and the tail she held past that prefix, restored after.
fn reply_for(mind: MindId, held: &[i32], rendered: &Rendered) -> Option<LearnReply> {
    let last_context = rendered.reply_from.checked_sub(1)?;
    let n_cached = common_prefix(held, &rendered.tokens).min(last_context);
    let tokens = rendered.tokens[n_cached..].to_vec();
    let labels = (n_cached..rendered.tokens.len())
        .map(|at| match rendered.tokens.get(at + 1) {
            Some(&next) if at + 1 >= rendered.reply_from => Label::Target(next),
            _ => Label::Unlabelled,
        })
        .collect();
    let restore = held[n_cached..].to_vec();
    Some(LearnReply { mind, n_cached: i32::try_from(n_cached).ok()?, tokens, labels, restore })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a reply that labels her context (teaching her what she was told), misses
    // her reply's first token, or restores anything but the exact tail she held.
    #[test]
    fn her_reply_learns_from_the_prefix_she_holds_and_restores_her_tail() {
        let rendered = Rendered { tokens: vec![1, 2, 3, 4, 50, 51], reply_from: 4 };
        let held = [1, 2, 9, 9];
        let reply = reply_for(mind_of(PersonaId(uuid::Uuid::nil())), &held, &rendered).expect("test: a reply");
        assert_eq!(reply.n_cached, 2, "learned from the prefix she holds");
        assert_eq!(reply.tokens, vec![3, 4, 50, 51]);
        assert_eq!(reply.labels, vec![Label::Unlabelled, Label::Target(50), Label::Target(51), Label::Unlabelled], "only her reply is a target");
        assert_eq!(reply.restore, vec![9, 9], "her exact tail comes back");
    }

    // what this catches: a lesson she already holds whole learned from past her reply's start, so
    // its first token would never be a target.
    #[test]
    fn a_lesson_she_holds_whole_still_learns_her_reply_from_its_first_token() {
        let rendered = Rendered { tokens: vec![1, 2, 3, 50], reply_from: 3 };
        let reply = reply_for(mind_of(PersonaId(uuid::Uuid::nil())), &[1, 2, 3, 50, 7], &rendered).expect("test: a reply");
        assert_eq!(reply.n_cached, 2);
        assert_eq!(reply.labels, vec![Label::Target(50), Label::Unlabelled]);
        assert_eq!(reply.restore, vec![3, 50, 7]);
    }
}
