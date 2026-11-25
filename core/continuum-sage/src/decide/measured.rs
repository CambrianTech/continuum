//! Views over a measured series: what a policy reads instead of a constant.

use std::collections::VecDeque;

/// A measurement or its explicit absence. Never a default standing in for a number nobody measured:
/// a policy that reads `Unmeasured` treats it as a cost it has not seen (CLAUDE.md: no constant where
/// a measurement exists, and no measurement invented where none does).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Measured<T> {
    #[default]
    Unmeasured,
    Value(T),
}

impl<T: Copy> Measured<T> {
    pub fn value(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Unmeasured => None,
        }
    }
}

/// A fraction in (0, 1] (an EWMA's weight on its newest sample), or in [0, 1] (a quantile): refused
/// outside its range by construction, never clamped into it.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Fraction(f64);

impl Fraction {
    /// In [0, 1]; `None` otherwise (and for NaN).
    pub fn of(value: f64) -> Option<Self> {
        (0.0..=1.0).contains(&value).then_some(Self(value))
    }

    /// In (0, 1]: a weight that moves the mean at all.
    pub fn positive(value: f64) -> Option<Self> {
        Self::of(value).filter(|fraction| fraction.0 > 0.0)
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// An exponentially weighted mean: follows a series, weighting the newest sample by its smoothing.
/// It retains one number.
#[derive(Debug, Clone, Copy)]
pub struct Ewma {
    smoothing: Fraction,
    mean: Measured<f64>,
}

impl Ewma {
    /// `smoothing` from [`Fraction::positive`]: a weight of zero would never move the mean.
    pub fn new(smoothing: Fraction) -> Self {
        Self { smoothing, mean: Measured::Unmeasured }
    }

    pub fn observe(&mut self, sample: f64) {
        self.mean = Measured::Value(match self.mean {
            Measured::Value(mean) => mean + self.smoothing.get() * (sample - mean),
            Measured::Unmeasured => sample,
        });
    }

    pub fn mean(&self) -> Measured<f64> {
        self.mean
    }
}

/// The last `capacity` samples of a series, for a policy that needs the distribution (a quantile, a
/// spread) rather than a mean. It retains exactly what it declares.
#[derive(Debug, Clone)]
pub struct Window {
    capacity: std::num::NonZeroUsize,
    samples: VecDeque<f64>,
}

impl Window {
    pub fn new(capacity: std::num::NonZeroUsize) -> Self {
        Self { capacity, samples: VecDeque::with_capacity(capacity.get()) }
    }

    pub fn observe(&mut self, sample: f64) {
        if self.samples.len() == self.capacity.get() {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// What it retains, declared: a view says how much history it keeps (THE-LANE-ON-SAGE.md).
    pub fn retains(&self) -> usize {
        self.capacity.get()
    }

    /// The `q` quantile of what it holds, by nearest rank; `Unmeasured` before any sample.
    pub fn quantile(&self, q: Fraction) -> Measured<f64> {
        let mut sorted: Vec<f64> = self.samples.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let last = sorted.len().saturating_sub(1);
        let rank = (q.get() * last as f64).round() as usize;
        sorted.get(rank).map_or(Measured::Unmeasured, |&value| Measured::Value(value))
    }
}
