//! The parent: what every readiness part does, once (mod.rs). A leaf is handed to [`Readiness::new`] and started like
//! any component; it never sees the horizon, the order of work, the measurement or the table.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Advanced, Horizon, Needs, Readies, Ready, Table, HORIZON};
use crate::component::{Bus, Component, Ctx, Key, Taken, Wiring};
use crate::dataflow::{Provenance, Publisher, Snapshot};
use crate::decide::{Ewma, Fraction, Measured};
use crate::Qos;

/// What wakes a readiness part.
pub enum ReadinessIn<M> {
    /// The lane's newest horizon.
    Horizon(Arc<Horizon<M>>),
    /// Its next unit of work: its own input, so a newer horizon is heard between units.
    Continue,
}

impl<M: Send + Sync + 'static> From<Snapshot<Horizon<M>>> for ReadinessIn<M> {
    fn from(snapshot: Snapshot<Horizon<M>>) -> Self {
        Self::Horizon(snapshot.value)
    }
}

/// The key its next unit is scheduled under: one continuation, never a queue of them.
const CONTINUE: Key = Key::ONE;

enum State<R: Readies> {
    Ready,
    /// Her work, `None` only while a unit of it is advancing.
    Preparing(Option<R::Work>),
    Refused(R::Refusal),
}

struct Entry<R: Readies> {
    mind: R::Mind,
    slack: Duration,
    state: State<R>,
}

/// One preparation as a component: the leaf, and everything it shares with every other.
pub struct Readiness<R: Readies> {
    leaf: R,
    says: Publisher<Table<R::Mind, R::Refusal>>,
    /// The horizon's minds this part tracks, in the horizon's order.
    minds: Vec<Entry<R>>,
    /// Seconds a unit of its work takes (its own unit: bytes, tokens, nodes), over every unit it advanced.
    per_unit: Ewma,
    /// The run that last published, so a rest (handed no context) still says what it gave back.
    last: Option<Provenance>,
}

impl<R: Readies> Readiness<R> {
    /// The part for `leaf`, its table claimed on `bus` (one writer to a topic).
    pub fn new(leaf: R, bus: &Bus) -> Result<Self, Taken> {
        Ok(Self { leaf, says: bus.publisher(R::TOPIC, Key::ONE)?, minds: Vec::new(), per_unit: Ewma::new(Fraction::QUARTER), last: None })
    }

    /// Track the new horizon: let go of whoever left it, ask everyone else what she needs now.
    fn follow(&mut self, horizon: &Horizon<R::Mind>) {
        let mut kept = Vec::with_capacity(horizon.minds.len());
        for upcoming in &horizon.minds {
            let held = self.minds.iter().position(|entry| entry.mind == upcoming.mind).map(|at| self.minds.swap_remove(at));
            let state = match held {
                // a unit of her work is not advancing here (handle awaits it), so her work is hers to keep
                Some(Entry { state: State::Preparing(Some(work)), .. }) => State::Preparing(Some(work)),
                // made ready or refused before: asked again, since what it prepared may have changed
                _ => match self.leaf.needs(upcoming.mind) {
                    Needs::Nothing => State::Ready,
                    Needs::Work(work) => State::Preparing(Some(work)),
                },
            };
            kept.push(Entry { mind: upcoming.mind, slack: upcoming.slack, state });
        }
        for left in std::mem::replace(&mut self.minds, kept) {
            self.leaf.release(left.mind);
        }
    }

    /// One unit of the most pressed mind's work.
    async fn advance(&mut self) {
        let next = self.minds.iter().enumerate().filter(|(_, entry)| matches!(entry.state, State::Preparing(Some(_)))).min_by_key(|(_, entry)| entry.slack).map(|(at, _)| at);
        let Some(at) = next else { return };
        let State::Preparing(work) = &mut self.minds[at].state else { return };
        let Some(work) = work.take() else { return };
        let (mind, before) = (self.minds[at].mind, R::left(&work));
        let started = Instant::now();
        let advanced = self.leaf.advance(mind, work).await;
        let took = started.elapsed();
        let (state, after) = match advanced {
            Advanced::Done => (State::Ready, 0),
            Advanced::More(work) => {
                let after = R::left(&work);
                (State::Preparing(Some(work)), after)
            }
            Advanced::Refused(why) => (State::Refused(why), before),
        };
        let moved = before.saturating_sub(after);
        if moved > 0 {
            self.per_unit.observe(took.as_secs_f64() / moved as f64);
        }
        // her entry is where it was: nothing else ran while the unit advanced
        if let Some(entry) = self.minds.get_mut(at).filter(|entry| entry.mind == mind) {
            entry.state = state;
        }
    }

    /// Each mind's readiness: a preparing mind is ready once her work and every more pressed mind's ahead of her is.
    fn table(&self) -> Table<R::Mind, R::Refusal> {
        let per_unit = self.per_unit.mean();
        let mut preparing: Vec<(Duration, u64, R::Mind)> = self
            .minds
            .iter()
            .filter_map(|entry| match &entry.state {
                State::Preparing(Some(work)) => Some((entry.slack, R::left(work), entry.mind)),
                _ => None,
            })
            .collect();
        preparing.sort_by_key(|&(slack, _, _)| slack);
        let mut ahead = 0u64;
        let mut eta = Vec::with_capacity(preparing.len());
        for (_, left, mind) in preparing {
            ahead = ahead.saturating_add(left);
            eta.push((mind, per_unit.value().and_then(|seconds| Duration::try_from_secs_f64(seconds * ahead as f64).ok())));
        }
        let minds = self
            .minds
            .iter()
            .map(|entry| {
                let ready = match &entry.state {
                    State::Ready => Ready::Ready,
                    State::Refused(why) => Ready::Refused(why.clone()),
                    State::Preparing(_) => {
                        let at = eta.iter().find(|(mind, _)| *mind == entry.mind).and_then(|(_, eta)| *eta);
                        Ready::Preparing { eta: at.map_or(Measured::Unmeasured, Measured::Value) } // map_or: no unit measured yet is no estimate
                    }
                };
                (entry.mind, ready)
            })
            .collect();
        Table { minds }
    }

    fn publish_and_go_on(&mut self, ctx: &mut Ctx<Self>) {
        let table = self.table();
        ctx.publish(&mut self.says, table);
        self.last = Some(ctx.provenance());
        if self.minds.iter().any(|entry| matches!(entry.state, State::Preparing(Some(_)))) {
            ctx.after(CONTINUE, Duration::ZERO, ReadinessIn::Continue);
        } else {
            ctx.cancel(CONTINUE);
        }
    }
}

impl<R: Readies> Component for Readiness<R> {
    type Input = ReadinessIn<R::Mind>;
    const NAME: &'static str = R::NAME;
    const QOS: Qos = R::QOS;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Horizon<R::Mind>>(HORIZON, Key::ONE);
    }

    async fn handle(&mut self, input: ReadinessIn<R::Mind>, ctx: &mut Ctx<Self>) {
        match input {
            ReadinessIn::Horizon(horizon) => self.follow(&horizon),
            ReadinessIn::Continue => self.advance().await,
        }
        self.publish_and_go_on(ctx);
    }

    /// Everything it holds for every mind is given back, and its table says so (no mind ready by its account);
    /// the next horizon asks them again.
    async fn rest(&mut self) {
        for entry in std::mem::take(&mut self.minds) {
            self.leaf.release(entry.mind);
        }
        if let Some(provenance) = self.last.clone() {
            self.says.publish(Table { minds: Vec::new() }, provenance);
        }
    }
}
