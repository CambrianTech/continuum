//! Views over a measured series: what a policy reads instead of a constant.
//! hot-path: the lane decides at every Core step (THE-LANE-ON-SAGE.md, "The types").

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
    /// One half: a median, an even split.
    pub const HALF: Self = Self(0.5);
    /// One quarter: an EWMA weight that remembers about the last four samples.
    pub const QUARTER: Self = Self(0.25);

    /// One over `n`: an EWMA weight that remembers about the last `n` samples.
    pub fn reciprocal(n: std::num::NonZeroUsize) -> Self {
        Self(1.0 / n.get() as f64)
    }

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
    /// In arrival order, to know which sample leaves next.
    samples: VecDeque<f64>,
    /// The same samples kept sorted as they arrive and leave (a binary search and one shift each), so
    /// a quantile is one index: no copy, no sort, no allocation after the window fills.
    sorted: Vec<f64>,
}

impl Window {
    pub fn new(capacity: std::num::NonZeroUsize) -> Self {
        Self { capacity, samples: VecDeque::with_capacity(capacity.get()), sorted: Vec::with_capacity(capacity.get()) } // cold: a view is built once; observing reuses its buffers
    }

    pub fn observe(&mut self, sample: f64) {
        if self.samples.len() == self.capacity.get() {
            if let Some(leaving) = self.samples.pop_front() {
                if let Ok(at) = self.sorted.binary_search_by(|kept| kept.total_cmp(&leaving)) {
                    self.sorted.remove(at);
                }
            }
        }
        self.samples.push_back(sample);
        let at = self.sorted.partition_point(|kept| kept.total_cmp(&sample).is_lt());
        self.sorted.insert(at, sample);
    }

    /// What it retains, declared: a view says how much history it keeps (THE-LANE-ON-SAGE.md).
    pub fn retains(&self) -> usize {
        self.capacity.get()
    }

    /// The `q` quantile of what it holds, by nearest rank; `Unmeasured` before any sample.
    pub fn quantile(&self, q: Fraction) -> Measured<f64> {
        let last = self.sorted.len().saturating_sub(1);
        let rank = (q.get() * last as f64).round() as usize;
        self.sorted.get(rank).map_or(Measured::Unmeasured, |&value| Measured::Value(value))
    }
}

/// A line `y = intercept + slope · x` fitted to a series by least squares, each sample's weight decaying
/// by its smoothing as newer ones arrive (an EWMA of the sufficient statistics): what a step's time is
/// as a fixed part and a per-token part, when its steps vary in size. It retains five numbers.
#[derive(Debug, Clone, Copy)]
pub struct LineFit {
    smoothing: Fraction,
    weight: f64,
    x: f64,
    y: f64,
    xx: f64,
    xy: f64,
}

impl LineFit {
    pub fn new(smoothing: Fraction) -> Self {
        Self { smoothing, weight: 0.0, x: 0.0, y: 0.0, xx: 0.0, xy: 0.0 }
    }

    pub fn observe(&mut self, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        let keep = 1.0 - self.smoothing.get();
        self.weight = keep * self.weight + 1.0;
        self.x = keep * self.x + x;
        self.y = keep * self.y + y;
        self.xx = keep * self.xx + x * x;
        self.xy = keep * self.xy + x * y;
    }

    /// `(intercept, slope)` once the samples it holds vary in `x` and give a line with a positive slope
    /// and a part that is not negative; `Unmeasured` before, never a guess.
    pub fn fit(&self) -> Measured<(f64, f64)> {
        if self.weight <= 1.0 {
            return Measured::Unmeasured;
        }
        let (mean_x, mean_y) = (self.x / self.weight, self.y / self.weight);
        let spread = self.xx / self.weight - mean_x * mean_x;
        if !(spread > f64::EPSILON * mean_x.abs().max(1.0)) {
            return Measured::Unmeasured;
        }
        let slope = (self.xy / self.weight - mean_x * mean_y) / spread;
        let intercept = mean_y - slope * mean_x;
        if slope > 0.0 && intercept >= 0.0 { Measured::Value((intercept, slope)) } else { Measured::Unmeasured }
    }
}
