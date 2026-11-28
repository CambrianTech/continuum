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

/// A prefill chunk's time as `fixed + n·(per_token + depth·(p + n/2))`, for `n` tokens starting at position `p`:
/// attention reads every token before it, so a token costs more the deeper it lands (the M5, 2026-11-28: 1.08 ms
/// a fresh token under 40k, 3.62 at 100k and past; a line through shallow chunks sized a 158k chunk past every
/// other mind's bound). Fitted by least squares over `(n, n·(p + n/2))`, each sample's weight decaying by its
/// smoothing as newer ones arrive (an EWMA of the sufficient statistics); it retains ten numbers.
#[derive(Debug, Clone, Copy)]
pub struct ChunkFit {
    smoothing: Fraction,
    weight: f64,
    u: f64,
    v: f64,
    y: f64,
    uu: f64,
    uv: f64,
    vv: f64,
    uy: f64,
    vy: f64,
}

/// A prefill chunk as it ran: its tokens, the position its first token landed at, and how long it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub tokens: u64,
    pub at: u64,
    pub took: std::time::Duration,
}

/// What a chunk costs, in seconds: its fixed part, a token's part at position 0, and what each position of depth
/// adds to a token's part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkCost {
    pub fixed: f64,
    pub per_token: f64,
    pub depth: f64,
}

impl ChunkCost {
    /// A token's part at position `at`.
    pub fn per_token_at(&self, at: u64) -> f64 {
        self.per_token + self.depth * at as f64
    }
}

impl ChunkFit {
    pub fn new(smoothing: Fraction) -> Self {
        Self { smoothing, weight: 0.0, u: 0.0, v: 0.0, y: 0.0, uu: 0.0, uv: 0.0, vv: 0.0, uy: 0.0, vy: 0.0 }
    }

    /// One chunk as it ran.
    pub fn observe(&mut self, chunk: Chunk) {
        let (tokens, at) = (chunk.tokens as f64, chunk.at as f64);
        let (u, v, y) = (tokens, tokens * (at + tokens / 2.0), chunk.took.as_secs_f64());
        if !(u.is_finite() && v.is_finite() && y.is_finite()) {
            return;
        }
        let keep = 1.0 - self.smoothing.get();
        self.weight = keep * self.weight + 1.0;
        self.u = keep * self.u + u;
        self.v = keep * self.v + v;
        self.y = keep * self.y + y;
        self.uu = keep * self.uu + u * u;
        self.uv = keep * self.uv + u * v;
        self.vv = keep * self.vv + v * v;
        self.uy = keep * self.uy + u * y;
        self.vy = keep * self.vy + v * y;
    }

    /// The cost once its samples vary in size, with every part positive (the fixed part not negative). The depth
    /// part is fitted once they also vary in depth independently of size; until then it is a cost not yet seen
    /// (zero) and the rest is the line through size alone. `Unmeasured` before either, never a guess.
    pub fn fit(&self) -> Measured<ChunkCost> {
        if self.weight <= 1.0 {
            return Measured::Unmeasured;
        }
        let w = self.weight;
        let (mu, mv, my) = (self.u / w, self.v / w, self.y / w);
        let (cuu, cuv, cvv) = (self.uu / w - mu * mu, self.uv / w - mu * mv, self.vv / w - mv * mv);
        let (cuy, cvy) = (self.uy / w - mu * my, self.vy / w - mv * my);
        if !(cuu > f64::EPSILON * mu.abs().max(1.0)) {
            return Measured::Unmeasured;
        }
        // size and depth vary independently: the plane (its determinant clear of round-off against its terms)
        let det = cuu * cvv - cuv * cuv;
        if det > 1e-9 * cuu * cvv {
            let per_token = (cuy * cvv - cvy * cuv) / det;
            let depth = (cvy * cuu - cuy * cuv) / det;
            let fixed = my - per_token * mu - depth * mv;
            if per_token > 0.0 && depth >= 0.0 && fixed >= 0.0 {
                return Measured::Value(ChunkCost { fixed, per_token, depth });
            }
        }
        let per_token = cuy / cuu;
        let fixed = my - per_token * mu;
        if per_token > 0.0 && fixed >= 0.0 { Measured::Value(ChunkCost { fixed, per_token, depth: 0.0 }) } else { Measured::Unmeasured }
    }
}
