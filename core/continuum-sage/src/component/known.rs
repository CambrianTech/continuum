//! What a component knows, and how new an input is to it (object model 5.7: novelty is measured
//! against what a part knows, over any history).
//!
//! In CBAR, terrain the compass or SLAM had never covered was frontier, and the color finder, the
//! semantic segmentation and the surface normals each decided on their own to run on it. Here every
//! component keeps its own knowledge behind [`Known`] and maps the [`Novelty`] it reports to its own
//! [`super::Urgency`]: frontier is urgent, the familiar is content (reuse or interpolate the last
//! output). A last-frame comparison is the simplest knowledge ([`LastSeen`]); coverage over a long
//! history is another ([`Seen`]); a model's expectation is a third, written by the part that owns it.

use std::collections::HashMap;
use std::hash::Hash;

/// How unknown an input is to a component: 0 is fully known, 1 is never seen.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Novelty(f32);

impl Novelty {
    /// Fully known: nothing here it has not already taken in.
    pub const KNOWN: Self = Self(0.0);
    /// Never seen: frontier.
    pub const FRONTIER: Self = Self(1.0);

    /// A measured novelty in `0..=1`; `None` outside it (or not a number).
    pub fn new(value: f32) -> Option<Self> {
        (0.0..=1.0).contains(&value).then_some(Self(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

/// A component's knowledge of its inputs, whatever shape it takes.
pub trait Known<I: ?Sized> {
    /// How unknown `input` is to what it knows now.
    fn novelty(&self, input: &I) -> Novelty;
    /// Fold `input` into what it knows.
    fn learn(&mut self, input: &I);
}

/// The simplest knowledge: the last input taken in. The same input again is known; any other is new.
#[derive(Debug, Clone, Default)]
pub struct LastSeen<T> {
    last: Option<T>,
}

impl<T: PartialEq + Clone> Known<T> for LastSeen<T> {
    fn novelty(&self, input: &T) -> Novelty {
        match &self.last {
            Some(last) if last == input => Novelty::KNOWN,
            _ => Novelty::FRONTIER,
        }
    }

    fn learn(&mut self, input: &T) {
        self.last = Some(input.clone());
    }
}

/// Knowledge as coverage over a long history: which things it has met and how often (cells of a
/// frontier map, the people, rooms and topics a mind has met). Never met is frontier; met `n` times
/// is `1 / (1 + n)`, the gist of familiarity (each meeting halves what is left to learn, roughly),
/// never a threshold.
#[derive(Debug, Clone)]
pub struct Seen<K> {
    met: HashMap<K, u32>,
}

impl<K> Default for Seen<K> {
    fn default() -> Self {
        Self { met: HashMap::new() }
    }
}

impl<K: Hash + Eq + Clone> Known<K> for Seen<K> {
    fn novelty(&self, input: &K) -> Novelty {
        // Never met is zero meetings: a count, not an unknown.
        let met = match self.met.get(input) {
            Some(n) => *n as f32,
            None => 0.0,
        };
        Novelty(1.0 / (1.0 + met))
    }

    fn learn(&mut self, input: &K) {
        let key = input.clone(); // its knowledge keeps its own copy of what it met
        *self.met.entry(key).or_insert(0) += 1;
    }
}
