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
    say, GenerationOutcome, Learning as Told, Measure, OptimizerStart, Said, Seat, SeatId, SeatState, ShadowSource, StepId, StepOutcome,
};

/// What reaches the driver: every verb as told, except a lesson, which arrives rendered.
pub(crate) enum Job {
    /// Measure, Load, Seat, Checkpoint or Release, as the trainer told it.
    Told(Told),
    /// A lesson rendered with her template on the submitter's thread.
    Learn { seat: SeatId, step: StepId, rendered: Rendered, admission: Box<dyn ReplyAdmission> },
}

impl Job {
    /// Say this job refused for `why`, nothing having been done.
    pub(crate) fn refuse(self, why: &str) {
        let why: Arc<str> = Arc::from(why);
        match self {
            Job::Learn { seat, step, .. } => say(Said::Step { seat, step, outcome: StepOutcome::Refused { why } }),
            Job::Told(Told::Learn(learn)) => say(Said::Step { seat: learn.seat, step: learn.step, outcome: StepOutcome::Refused { why } }),
            Job::Told(Told::Measure(measure)) => say(Said::Seat { seat: measure.seat, state: SeatState::Refused { why } }),
            Job::Told(Told::Load { seat }) => say(Said::Seat { seat, state: SeatState::Refused { why } }),
            Job::Told(Told::Seat(seated)) => say(Said::Seat { seat: seated.seat, state: SeatState::Refused { why } }),
            Job::Told(Told::Checkpoint { seat, .. }) => say(Said::Generation { seat, outcome: GenerationOutcome::Refused { why } }),
            // A release refused leaves the seat as it was: nothing more was allocated, nothing freed.
            Job::Told(Told::Release { seat }) => say(Said::Seat { seat, state: SeatState::Quarantined { why } }),
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

    pub(super) fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Everything still waiting, refused for `why` (the engine is stopping or closed).
    pub(super) fn refuse_waiting(&mut self, why: &str) {
        for job in self.waiting.drain(..) {
            job.refuse(why);
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
    /// A reply's restoration or a release was not confirmed: held until a release is.
    Quarantined { shadow: Shadow },
}

fn mind_of(persona: PersonaId) -> MindId {
    MindId { persona, epoch: Epoch(1) }
}

impl Driver<'_, '_> {
    /// Work the oldest waiting job. `go_on` says whether no turn waits.
    pub(super) fn learn_next(&mut self, go_on: &mut dyn FnMut() -> bool) {
        let Some(job) = self.learning.waiting.pop_front() else { return };
        match job {
            Job::Told(Told::Measure(measure)) => self.measure(measure),
            Job::Told(Told::Load { seat }) => self.load(seat),
            Job::Told(Told::Seat(seated)) => self.seat(seated),
            Job::Told(Told::Checkpoint { seat, into, binding }) => self.checkpoint(seat, &into, &binding),
            Job::Told(Told::Release { seat }) => self.release(seat),
            Job::Learn { seat, step, rendered, admission } => self.lesson(seat, step, &rendered, admission, go_on),
            // A lesson is rendered before it is queued; one that arrives unrendered is refused.
            told @ Job::Told(Told::Learn(_)) => told.refuse("the lesson reached the engine unrendered"),
        }
    }

    fn measure(&mut self, measure: Measure) {
        let Measure { seat, mind, shadow, optimizer, reply_rows } = measure;
        if self.learning.seats.contains_key(&seat) {
            return say(Said::Seat { seat, state: SeatState::Refused { why: "the seat is already measured".into() } });
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
        let Phase::Measured { source } = entry.phase else {
            let why = "load of a seat past measuring";
            self.learning.seats.insert(seat, entry);
            return say(Said::Seat { seat, state: SeatState::Refused { why: why.into() } });
        };
        let loaded = match &source {
            // Written where nothing else reads it, loaded, and removed: the shadow lives in memory, and
            // only a checkpoint makes it durable.
            ShadowSource::Fresh(fresh) => {
                let path = std::env::temp_dir().join(format!("continuum-shadow-{}.gguf", seat.0));
                let written = self.model.write_fresh_shadow(&path, &FreshGeometry {
                    rank: fresh.rank,
                    alpha: fresh.alpha,
                    targets: &fresh.targets,
                    seed: fresh.seed,
                    top_layers: fresh.top_layers,
                });
                let loaded = written.map_err(|refusal| refusal.to_string()).and_then(|()| self.model.load_lora(&path));
                let _ = std::fs::remove_file(&path);
                loaded
            }
            ShadowSource::Generation(dir) => self.model.load_lora(dir.join("adapter.gguf")),
        };
        let adapter = match loaded {
            Ok(adapter) => Arc::new(adapter),
            Err(why) => return say(Said::Seat { seat, state: SeatState::Refused { why: why.into() } }),
        };
        let shadow = Shadow { adapter, optimizer: entry.optimizer, recompute: false, walk: Walk::default() };
        match self.engine.measure_shadow(mind_of(entry.mind), &shadow, entry.reply_rows) {
            Ok((optimizer, workspace)) => {
                let reply_rows = entry.reply_rows;
                self.learning.seats.insert(seat, Entry { phase: Phase::Loaded { shadow }, ..entry });
                say(Said::Seat { seat, state: SeatState::Loaded { optimizer, workspace, reply_rows } });
            }
            // The shadow is detached and dropped here: its weights are freed, so nothing remains.
            Err(refusal) => say(Said::Seat { seat, state: SeatState::Refused { why: refusal.to_string().into() } }),
        }
    }

    fn seat(&mut self, seated: Seat) {
        let Seat { seat, lease } = seated;
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return say(Said::Seat { seat, state: SeatState::Refused { why: "seat of a seat never loaded".into() } });
        };
        let Phase::Loaded { shadow } = entry.phase else {
            self.learning.seats.insert(seat, entry);
            return say(Said::Seat { seat, state: SeatState::Refused { why: "seat of a seat not loaded".into() } });
        };
        match self.engine.retain_shadow(&shadow, lease) {
            Ok(()) => {
                self.learning.seats.insert(seat, Entry { phase: Phase::Seated { shadow }, ..entry });
                say(Said::Seat { seat, state: SeatState::Seated { optimizer: OptimizerStart::Cold } });
            }
            Err(_lease) => {
                self.learning.seats.insert(seat, Entry { phase: Phase::Loaded { shadow }, ..entry });
                say(Said::Seat { seat, state: SeatState::Refused { why: "the shadow is already retained".into() } });
            }
        }
    }

    fn checkpoint(&mut self, seat: SeatId, into: &std::path::Path, binding: &str) {
        let outcome = match self.learning.seats.get(&seat) {
            Some(Entry { mind, phase: Phase::Seated { shadow }, .. }) => {
                let (mind, shadow) = (*mind, Shadow { adapter: Arc::clone(&shadow.adapter), ..*shadow });
                match self.engine.save_shadow(mind_of(mind), &shadow, into, binding) {
                    Ok(steps) => GenerationOutcome::Saved { into: into.to_path_buf(), steps },
                    Err(refusal) => GenerationOutcome::Refused { why: refusal.to_string().into() },
                }
            }
            _ => GenerationOutcome::Refused { why: "checkpoint of a seat not seated".into() },
        };
        say(Said::Generation { seat, outcome });
    }

    fn release(&mut self, seat: SeatId) {
        let Some(entry) = self.learning.seats.remove(&seat) else {
            return say(Said::Seat { seat, state: SeatState::Released });
        };
        let shadow = match entry.phase {
            // Never retained: dropping the shadow frees its weights.
            Phase::Measured { .. } | Phase::Loaded { .. } => return say(Said::Seat { seat, state: SeatState::Released }),
            Phase::Seated { shadow } | Phase::Quarantined { shadow } => shadow,
        };
        match self.engine.release_shadow(mind_of(entry.mind), &shadow) {
            // The lease view drops here; the trainer releases its guards on hearing Released.
            Ok(_lease) => say(Said::Seat { seat, state: SeatState::Released }),
            Err(refusal) => {
                say(Said::Seat { seat, state: SeatState::Quarantined { why: refusal.to_string().into() } });
                self.learning.seats.insert(seat, Entry { phase: Phase::Quarantined { shadow }, ..entry });
            }
        }
    }

    fn lesson(&mut self, seat: SeatId, step: StepId, rendered: &Rendered, admission: Box<dyn ReplyAdmission>, go_on: &mut dyn FnMut() -> bool) {
        let refused = |why: &str| say(Said::Step { seat, step, outcome: StepOutcome::Refused { why: why.into() } });
        let Some(Entry { mind, reply_rows, phase: Phase::Seated { shadow }, .. }) = self.learning.seats.get(&seat) else {
            return refused("a lesson for a seat not seated");
        };
        let (mind, reply_rows, shadow) = (*mind, *reply_rows, Shadow { adapter: Arc::clone(&shadow.adapter), ..*shadow });
        if let Err(unbound) = self.switch_to(mind, false) {
            return refused(&format!("she could not be bound to learn: {unbound}"));
        }
        let held = match self.engine.active_mut() {
            Some(her) => her.parts().tokens.held.clone(),
            None => return refused("she was not bound after the switch"),
        };
        let reply = match reply_for(&held, rendered) {
            Some(reply) => reply,
            None => return refused("the lesson has no reply past what she holds"),
        };
        if reply.tokens.len() > reply_rows.get() as usize {
            return refused(&format!("{} reply tokens past the {} the seat was measured for", reply.tokens.len(), reply_rows));
        }
        let outcome = match self.engine.learn_active(&shadow, &reply, admission, go_on) {
            Ok(Finished::Learned { steps, loss, .. }) => StepOutcome::Learned { steps, loss },
            Ok(Finished::Unchanged { .. }) => StepOutcome::Unchanged,
            Ok(Finished::Uncertain { steps, .. }) => StepOutcome::Uncertain { steps },
            Ok(Finished::Failed { refusal, .. }) => {
                let why: Arc<str> = refusal.to_string().into();
                if let Some(entry) = self.learning.seats.remove(&seat) {
                    let shadow = match entry.phase {
                        Phase::Seated { shadow } | Phase::Quarantined { shadow } | Phase::Loaded { shadow } => shadow,
                        Phase::Measured { .. } => shadow,
                    };
                    self.learning.seats.insert(seat, Entry { phase: Phase::Quarantined { shadow }, ..entry });
                }
                say(Said::Seat { seat, state: SeatState::Quarantined { why: Arc::clone(&why) } });
                StepOutcome::Refused { why }
            }
            Err((refusal, _admission)) => StepOutcome::Refused { why: refusal.to_string().into() },
        };
        say(Said::Step { seat, step, outcome });
    }
}

/// Her reply on her own sequence: from the longest prefix she holds of the lesson (never past the
/// token before her reply, whose label is the reply's first token), the lesson's tokens with a label
/// on every one whose next token is hers, and the tail she held past that prefix, restored after.
fn reply_for(held: &[i32], rendered: &Rendered) -> Option<LearnReply> {
    let last_context = rendered.reply_from.checked_sub(1)?;
    let n_cached = common_prefix(held, &rendered.tokens).min(last_context);
    let tokens = rendered.tokens[n_cached..].to_vec();
    let labels = (n_cached..rendered.tokens.len())
        .map(|at| match rendered.tokens.get(at + 1) {
            Some(&next) if at + 1 >= rendered.reply_from => Label::Target(next),
            _ => Label::Unlabelled,
        })
        .collect();
    let n_cached = i32::try_from(n_cached).ok()?;
    Some(LearnReply { n_cached, tokens, labels, restore: held[n_cached as usize..].to_vec() })
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
        let reply = reply_for(&held, &rendered).expect("test: a reply");
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
        let reply = reply_for(&[1, 2, 3, 50, 7], &rendered).expect("test: a reply");
        assert_eq!(reply.n_cached, 2);
        assert_eq!(reply.labels, vec![Label::Target(50), Label::Unlabelled]);
        assert_eq!(reply.restore, vec![3, 50, 7]);
    }
}
