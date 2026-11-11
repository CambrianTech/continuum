//! What holding context costs an act, in seconds, measured on the node and model that serve it
//! (card 7b7a696a). Every token held slows every act: attention reads the whole KV while each
//! output token decodes and while each fresh token prefills, and the M5's MoE is memory-bandwidth
//! bound. The regression this measures (the M5, 2026-11-07): the pool regrew 152,832 -> 755,456
//! tokens, her prompts went from ~30k to 35-75k tokens, and the median act from 50 s to 104 s.
//!
//! Three lines, each fitted to her acts' own engine timings (`GenerationTiming`):
//! - decode seconds per output token against the context it decoded over (input + half the
//!   output), each act weighted by its output tokens;
//! - prefill seconds per fresh token against the context it prefilled over (cached + half the
//!   fresh), each act weighted by its fresh tokens;
//! - fresh tokens per act against the prompt: a held token is prefilled again whenever her cache
//!   is lost or her view turns over, so a larger context also brings more fresh tokens per act.
//!
//! An act's seconds holding `context` tokens is then `output x decode(context + output/2) +
//! fresh(context) x prefill(context - fresh/2)` at her mean output per act ([`ActTime::seconds`]),
//! and its derivative is the marginal seconds one more held token costs one act
//! ([`ActTime::seconds_per_token`]). A fourth line, engine seconds per act against the prompt, is
//! fitted directly beside them: the check the composed curve is held to. The fit keeps sufficient
//! statistics only (no sample list, no window to size); each slope carries its standard error;
//! too few acts to fit a line is the type [`ContextPrice::Unmeasured`], never a number.
//!
//! The validator is `cargo run -p continuum-context --example replay -- --act-price`, which feeds a
//! node's prompt captures through this same fit.

/// One completed act's engine timing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActSample {
    /// The prompt's tokens (`usage.inputTokens`).
    pub input: u32,
    /// The prompt tokens the engine resumed from its cache (`timing.cachedTokens`).
    pub cached: u32,
    /// The prompt tokens it prefilled (`timing.prefillTokens`).
    pub fresh: u32,
    pub prefill_ms: f64,
    /// Output tokens decoded (`timing.decodeTokens`).
    pub output: u32,
    pub decode_ms: f64,
}

/// A weighted least-squares line kept as running co-moments (West's update: stable where the
/// context's mean dwarfs its spread).
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Moments {
    acts: u32,
    weight: f64,
    mean_x: f64,
    mean_y: f64,
    sxx: f64,
    sxy: f64,
    syy: f64,
    min_x: f64,
    max_x: f64,
}

impl Moments {
    fn observe(&mut self, x: f64, y: f64, weight: f64) {
        if !(weight > 0.0 && x.is_finite() && y.is_finite()) {
            return;
        }
        self.acts += 1;
        self.min_x = if self.acts == 1 { x } else { self.min_x.min(x) };
        self.max_x = if self.acts == 1 { x } else { self.max_x.max(x) };
        self.weight += weight;
        let (dx, dy) = (x - self.mean_x, y - self.mean_y);
        self.mean_x += dx * weight / self.weight;
        self.mean_y += dy * weight / self.weight;
        self.sxx += weight * dx * (x - self.mean_x);
        self.sxy += weight * dx * (y - self.mean_y);
        self.syy += weight * dy * (y - self.mean_y);
    }

    /// The fitted line; `None` below three acts or with no spread in context (the slope and its
    /// error are then undetermined). The error counts each ACT as one observation (the weights
    /// normalized to the act count): an act's tokens share one engine state, never independent
    /// draws.
    fn line(&self) -> Option<Line> {
        if self.acts < 3 || !(self.sxx > 0.0) {
            return None;
        }
        let slope = self.sxy / self.sxx;
        let residual = (self.syy - slope * self.sxy).max(0.0) / f64::from(self.acts - 2);
        Some(Line {
            intercept: self.mean_y - slope * self.mean_x,
            slope,
            slope_se: (residual / self.sxx).sqrt(),
            intercept_se: (residual * (1.0 / self.weight + self.mean_x * self.mean_x / self.sxx)).sqrt(),
            acts: self.acts,
            context: (self.min_x as u32, self.max_x as u32),
        })
    }
}

/// A quantity as a line in the context held: `intercept + slope x context`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub intercept: f64,
    pub slope: f64,
    /// One standard error of the slope.
    pub slope_se: f64,
    pub intercept_se: f64,
    /// The acts it was fitted to.
    pub acts: u32,
    /// The context those acts spanned: measured inside, extrapolated outside.
    pub context: (u32, u32),
}

impl Line {
    pub fn at(&self, context: f64) -> f64 {
        self.intercept + self.slope * context
    }
}

/// The running fit of one model's acts on one node.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ActTimeFit {
    decode: Moments,
    prefill: Moments,
    /// Absent in a fit saved by a build that kept warm acts apart (0b1450291, reverted): it loads
    /// empty and refills, never refusing her whole checkpoint (it did: every seat failed with
    /// "missing field `fresh`", the M5, 2026-11-11 00:43Z).
    #[serde(default)]
    fresh: Moments,
    observed: Moments,
    acts: u32,
    output: f64,
}

impl ActTimeFit {
    /// Fold one completed act in. An act with no output measures no decode, one with nothing fresh
    /// no prefill; both still count toward her output per act and her fresh tokens by context.
    pub fn observe(&mut self, act: &ActSample) {
        self.acts += 1;
        self.output += f64::from(act.output);
        let input = f64::from(act.input);
        self.fresh.observe(input, f64::from(act.fresh), 1.0);
        self.observed.observe(input, (act.decode_ms + act.prefill_ms) / 1_000.0, 1.0);
        if act.output > 0 {
            let over = input + f64::from(act.output) / 2.0;
            self.decode.observe(over, act.decode_ms / 1_000.0 / f64::from(act.output), f64::from(act.output));
        }
        if act.fresh > 0 {
            let over = f64::from(act.cached) + f64::from(act.fresh) / 2.0;
            self.prefill.observe(over, act.prefill_ms / 1_000.0 / f64::from(act.fresh), f64::from(act.fresh));
        }
    }

    /// The acts folded in.
    pub fn acts(&self) -> u32 {
        self.acts
    }

    /// What holding context costs her act, or `Unmeasured` while any line is undetermined or the
    /// marginal seconds at her mean context are within one standard error of none (a few acts'
    /// noise is never a price: it would collapse her on a slope nobody measured).
    pub fn price(&self) -> ContextPrice {
        let (Some(decode), Some(prefill), Some(fresh), Some(observed)) = (self.decode.line(), self.prefill.line(), self.fresh.line(), self.observed.line()) else {
            return ContextPrice::Unmeasured;
        };
        let time = ActTime { decode, prefill, fresh, observed, output: self.output / f64::from(self.acts) };
        let typical = self.fresh.mean_x;
        if time.seconds_per_token(typical) > time.seconds_per_token_se(typical) {
            ContextPrice::Measured(time)
        } else {
            ContextPrice::Unmeasured
        }
    }
}

/// An act's seconds as a function of the context it holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActTime {
    /// Decode seconds per output token by context.
    pub decode: Line,
    /// Prefill seconds per fresh token by context.
    pub prefill: Line,
    /// Fresh tokens per act by prompt tokens.
    pub fresh: Line,
    /// Engine seconds per act by prompt tokens, fitted directly: the composed curve's check.
    pub observed: Line,
    /// Her mean output tokens per act.
    pub output: f64,
}

impl ActTime {
    /// The prompt contexts her acts were measured over: the curve is a measurement inside, an
    /// extrapolation outside, and a price choice never leaves it (`allocation::priced`).
    pub fn measured(&self) -> (u32, u32) {
        self.observed.context
    }

    /// Her fresh tokens per act holding `context`: never below none, never above the prompt.
    pub fn fresh_at(&self, context: f64) -> f64 {
        self.fresh.at(context).clamp(0.0, context.max(0.0))
    }

    /// Seconds one act spends holding `context` tokens.
    pub fn seconds(&self, context: f64) -> f64 {
        let fresh = self.fresh_at(context);
        self.output * self.decode.at(context + self.output / 2.0) + fresh * self.prefill.at(context - fresh / 2.0)
    }

    /// The marginal seconds one more held token costs one act at `context`: d(seconds per act) /
    /// d(context tokens), the derivative of [`Self::seconds`].
    pub fn seconds_per_token(&self, context: f64) -> f64 {
        let fresh = self.fresh_at(context);
        let fresh_slope = if fresh <= 0.0 {
            0.0
        } else if fresh >= context {
            1.0
        } else {
            self.fresh.slope
        };
        self.output * self.decode.slope
            + fresh_slope * self.prefill.at(context - fresh / 2.0)
            + fresh * self.prefill.slope * (1.0 - fresh_slope / 2.0)
    }

    /// One standard error of [`Self::seconds_per_token`] at `context`, from the three slopes'
    /// errors (the lines are fitted independently).
    pub fn seconds_per_token_se(&self, context: f64) -> f64 {
        let fresh = self.fresh_at(context);
        let terms = [
            self.output * self.decode.slope_se,
            self.prefill.at(context - fresh / 2.0) * self.fresh.slope_se,
            fresh * self.prefill.slope_se,
        ];
        terms.iter().map(|term| term * term).sum::<f64>().sqrt()
    }
}

/// The price of context for one mind's act on the node and model serving her.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ContextPrice {
    /// Too few acts to fit every line: no cost term, and her allocation's receipt says so.
    #[default]
    Unmeasured,
    Measured(ActTime),
}

#[cfg(test)]
mod tests;
