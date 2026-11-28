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
pub enum ReadinessIn<M, A> {
    /// The lane's newest horizon.
    Horizon(Arc<Horizon<M>>),
    /// Its next unit of work: its own input, so a newer horizon is heard between units.
    Continue,
    /// One of the leaf's own inputs (a block to hold, a fence that fired).
    Act(A),
}

impl<M: Send + Sync + 'static, A> From<Snapshot<Horizon<M>>> for ReadinessIn<M, A> {
    fn from(snapshot: Snapshot<Horizon<M>>) -> Self {
        Self::Horizon(snapshot.value)
    }
}

/// The key its next unit is scheduled under: one continuation, never a queue of them.
const CONTINUE: Key = Key::ONE;

enum State<R: Readies> {
    Ready,
    /// Her work, waiting for its next unit.
    Preparing(R::Work),
    /// A unit of hers in flight since `since`, `left` units of work when it started; its end comes as an act, and the
    /// leaf holds what is moving.
    InFlight { left: u64, since: Instant },
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

    fn asked(&mut self, mind: R::Mind) -> State<R> {
        match self.leaf.needs(mind) {
            Needs::Nothing => State::Ready,
            Needs::Work(work) => State::Preparing(work),
        }
    }

    /// Track the new horizon: let go of whoever left it, ask everyone else what she needs now. Work under way is
    /// kept as it is; a mind made ready or refused before is asked again, since what it prepared may have changed.
    fn follow(&mut self, horizon: &Horizon<R::Mind>) {
        let mut kept = Vec::with_capacity(horizon.minds.len());
        for upcoming in &horizon.minds {
            let held = self.minds.iter().position(|entry| entry.mind == upcoming.mind).map(|at| self.minds.swap_remove(at));
            let state = match held {
                Some(Entry { state: state @ (State::Preparing(_) | State::InFlight { .. }), .. }) => state,
                _ => self.asked(upcoming.mind),
            };
            kept.push(Entry { mind: upcoming.mind, slack: upcoming.slack, state });
        }
        for left in std::mem::replace(&mut self.minds, kept) {
            self.leaf.release(left.mind);
        }
    }

    /// After an act: every mind not being prepared is asked again (a block held for her, a newer one superseding).
    fn reask(&mut self) {
        for at in 0..self.minds.len() {
            if matches!(self.minds[at].state, State::Ready | State::Refused(_)) {
                self.minds[at].state = self.asked(self.minds[at].mind);
            }
        }
    }

    /// One unit of the most pressed mind's work, unless one is in flight.
    async fn advance(&mut self) {
        if self.minds.iter().any(|entry| matches!(entry.state, State::InFlight { .. })) {
            return;
        }
        let next = self.minds.iter().enumerate().filter(|(_, entry)| matches!(entry.state, State::Preparing(_))).min_by_key(|(_, entry)| entry.slack).map(|(at, _)| at);
        let Some(at) = next else { return };
        let mind = self.minds[at].mind;
        let State::Preparing(work) = std::mem::replace(&mut self.minds[at].state, State::Ready) else { return };
        let (before, since) = (R::left(&work), Instant::now());
        let advanced = self.leaf.advance(mind, work).await;
        let state = self.ended(advanced, before, since);
        // her entry is where it was: nothing else ran while the unit advanced
        if let Some(entry) = self.minds.get_mut(at).filter(|entry| entry.mind == mind) {
            entry.state = state;
        }
    }

    /// A unit's end: its cost measured against what it moved, and her next state.
    fn ended(&mut self, advanced: Advanced<R::Work, R::Refusal>, before: u64, since: Instant) -> State<R> {
        let (state, after) = match advanced {
            Advanced::Done => (State::Ready, 0),
            Advanced::More(work) => {
                let after = R::left(&work);
                (State::Preparing(work), after)
            }
            Advanced::Started => return State::InFlight { left: before, since },
            Advanced::Refused(why) => (State::Refused(why), before),
        };
        let moved = before.saturating_sub(after);
        if moved > 0 {
            self.per_unit.observe(since.elapsed().as_secs_f64() / moved as f64);
        }
        state
    }

    /// One of the leaf's acts; one that ends a unit in flight ends it for its mind.
    fn act(&mut self, act: R::Act) {
        if let Some((mind, advanced)) = self.leaf.act(act) {
            if let Some(at) = self.minds.iter().position(|entry| entry.mind == mind && matches!(entry.state, State::InFlight { .. })) {
                if let State::InFlight { left, since } = std::mem::replace(&mut self.minds[at].state, State::Ready) {
                    self.minds[at].state = self.ended(advanced, left, since);
                }
            }
        }
        self.reask();
    }

    /// Each mind's readiness: a preparing mind is ready once her work and every more pressed mind's ahead of her is.
    fn table(&self) -> Table<R::Mind, R::Refusal> {
        let per_unit = self.per_unit.mean();
        let mut preparing: Vec<(Duration, u64, R::Mind)> = self
            .minds
            .iter()
            .filter_map(|entry| match &entry.state {
                State::Preparing(work) => Some((entry.slack, R::left(work), entry.mind)),
                State::InFlight { left, .. } => Some((entry.slack, *left, entry.mind)),
                State::Ready | State::Refused(_) => None,
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
                    State::Preparing(_) | State::InFlight { .. } => {
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
        let idle = !self.minds.iter().any(|entry| matches!(entry.state, State::InFlight { .. }));
        if idle && self.minds.iter().any(|entry| matches!(entry.state, State::Preparing(_))) {
            ctx.after(CONTINUE, Duration::ZERO, ReadinessIn::Continue);
        } else {
            ctx.cancel(CONTINUE);
        }
    }
}

impl<R: Readies> Component for Readiness<R> {
    type Input = ReadinessIn<R::Mind, R::Act>;
    const NAME: &'static str = R::NAME;
    const QOS: Qos = R::QOS;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Horizon<R::Mind>>(HORIZON, Key::ONE);
    }

    async fn handle(&mut self, input: Self::Input, ctx: &mut Ctx<Self>) {
        match input {
            ReadinessIn::Horizon(horizon) => self.follow(&horizon),
            ReadinessIn::Continue => self.advance().await,
            ReadinessIn::Act(act) => self.act(act),
        }
        self.publish_and_go_on(ctx);
    }

    /// Everything it holds for every mind is given back, and its table says so (no mind ready by its account); the
    /// next horizon asks them again.
    async fn rest(&mut self) {
        for entry in std::mem::take(&mut self.minds) {
            self.leaf.release(entry.mind);
        }
        if let Some(provenance) = self.last.clone() {
            self.says.publish(Table { minds: Vec::new() }, provenance);
        }
    }
}
