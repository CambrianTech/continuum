//! A stock online learner: ridge regression of several targets on one feature vector, updated one
//! observation at a time, that forgets toward its prior. Any use case's policy can use it (the
//! linear UCB does, `linear_ucb.rs`); none writes its own.
//! hot-path: a lane policy predicts at every Core step and learns at every outcome.
//!
//! It keeps the regularized Gram matrix `A = PRIOR·I + Σ γ^age x xᵀ` and `b_m = Σ γ^age y_m x`, and
//! after each observation the Cholesky factor of `A` and every target's weights `θ_m = A⁻¹ b_m`.
//! Forgetting decays each observation's weight by `γ = 1 - 1/memory` per later observation, and the
//! Gram matrix decays toward the prior, never toward zero: `A` stays at least `PRIOR·I`, so a
//! direction the data stops exciting returns to the prior's uncertainty and no further (the windup an
//! exponentially forgetting recursive least squares has). Each target's residual scale is a mean of
//! its squared prediction errors over the same memory, so a prediction's confidence is in the target's
//! own units: `σ_m · √(xᵀA⁻¹x)` is one standard error.
//!
//! Fixed-size arrays: no allocation, `D²` per prediction and `D³/6` per observation (`D` is a
//! handful of features, so a few hundred flops against a step of milliseconds).

use std::num::NonZeroUsize;

use super::measured::{Ewma, Fraction, Measured};
use super::policy::Refused;

/// The ridge prior: one pseudo-observation of zero per feature. Features are scaled to about one
/// (a use case states its features that way), so this is one sample's worth of belief that a weight
/// is zero, which the first observations outweigh.
const PRIOR: f64 = 1.0;

/// What it learned, and what it derives from that.
#[derive(Debug, Clone, Copy)]
pub struct Ridge<const D: usize, const M: usize> {
    gram: [[f64; D]; D],
    moment: [[f64; D]; M],
    residual: [Ewma; M],
    /// Derived from `gram` and `moment` after every change: the lower Cholesky factor and the weights.
    factor: [[f64; D]; D],
    weights: [[f64; D]; M],
    keep: f64,
    smoothing: Fraction,
}

/// One prediction: every target's mean, and the standard-error width `√(xᵀA⁻¹x)` shared by all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Predicted<const M: usize> {
    pub mean: [f64; M],
    pub width: f64,
}

impl<const D: usize, const M: usize> Ridge<D, M> {
    /// It forgets an observation's weight by a factor `1/e` over `memory` later ones.
    pub fn new(memory: NonZeroUsize) -> Self {
        let smoothing = Fraction::reciprocal(memory);
        let keep = 1.0 - smoothing.get();
        let mut gram = [[0.0; D]; D];
        let mut factor = [[0.0; D]; D];
        for i in 0..D {
            gram[i][i] = PRIOR;
            factor[i][i] = PRIOR.sqrt();
        }
        Self { gram, moment: [[0.0; D]; M], residual: [Ewma::new(smoothing); M], factor, weights: [[0.0; D]; M], keep, smoothing }
    }

    /// The memory it was built with, in observations.
    pub fn memory(&self) -> f64 {
        1.0 / (1.0 - self.keep)
    }

    pub fn predict(&self, x: &[f64; D]) -> Predicted<M> {
        let mut mean = [0.0; M];
        for (m, weights) in self.weights.iter().enumerate() {
            mean[m] = dot(weights, x);
        }
        let z = forward(&self.factor, x);
        Predicted { mean, width: dot(&z, &z).sqrt() }
    }

    /// Each target's residual scale: the root mean squared error of its predictions so far, in its own
    /// units; `Unmeasured` before its first observation.
    pub fn scale(&self, target: usize) -> Measured<f64> {
        self.residual.get(target).map_or(Measured::Unmeasured, |residual| residual.mean().value().map_or(Measured::Unmeasured, |square| Measured::Value(square.sqrt())))
    }

    /// Learn that features `x` led to targets `y`. Refused (the learner unchanged) when the decayed
    /// Gram matrix does not factor, which a prior of at least `PRIOR·I` rules out short of a NaN input.
    pub fn observe(&mut self, x: &[f64; D], y: &[f64; M]) -> Result<(), Refused> {
        if !x.iter().chain(y.iter()).all(|value| value.is_finite()) {
            return Err(Refused::State { algorithm: "ridge", bytes: 0 });
        }
        let mut gram = self.gram;
        for i in 0..D {
            for j in 0..D {
                let prior = if i == j { PRIOR } else { 0.0 };
                gram[i][j] = self.keep * (gram[i][j] - prior) + prior + x[i] * x[j];
            }
        }
        let factor = cholesky(&gram).ok_or(Refused::State { algorithm: "ridge", bytes: 0 })?;
        for m in 0..M {
            self.residual[m].observe((y[m] - dot(&self.weights[m], x)).powi(2));
            for i in 0..D {
                self.moment[m][i] = self.keep * self.moment[m][i] + y[m] * x[i];
            }
            self.weights[m] = backward(&factor, &forward(&factor, &self.moment[m]));
        }
        self.gram = gram;
        self.factor = factor;
        Ok(())
    }

    /// The bytes of what it learned: the Gram matrix, the moments, each residual scale (a flag byte,
    /// then its mean square).
    pub fn save(&self, into: &mut Vec<u8>) {
        for value in self.gram.iter().flatten().chain(self.moment.iter().flatten()) {
            into.extend_from_slice(&value.to_le_bytes());
        }
        for residual in &self.residual {
            let (flag, square) = match residual.mean() {
                Measured::Value(square) => (1, square),
                Measured::Unmeasured => (0, f64::NAN),
            };
            into.push(flag);
            into.extend_from_slice(&square.to_le_bytes());
        }
    }

    /// The bytes `save` wrote for this shape, or refused whole.
    pub fn load(&mut self, from: &[u8], algorithm: &'static str) -> Result<(), Refused> {
        let refused = Refused::State { algorithm, bytes: from.len() };
        if from.len() != Self::saved_len() {
            return Err(refused);
        }
        let mut reals = from.chunks_exact(8).map(|bytes| bytes.try_into().map(f64::from_le_bytes).map_err(|_| refused));
        let mut gram = [[0.0; D]; D];
        let mut moment = [[0.0; D]; M];
        for value in gram.iter_mut().flatten().chain(moment.iter_mut().flatten()) {
            *value = reals.next().ok_or(refused)??;
        }
        let factor = cholesky(&gram).ok_or(refused)?;
        let mut residual = self.residual;
        let tail = &from[8 * (D * D + M * D)..];
        for (m, saved) in tail.chunks_exact(9).enumerate() {
            let (flag, square) = saved.split_first().ok_or(refused)?;
            let square = f64::from_le_bytes(square.try_into().map_err(|_| refused)?);
            residual[m] = Ewma::new(self.smoothing);
            match flag {
                0 => {}
                1 if square.is_finite() && square >= 0.0 => residual[m].observe(square),
                _ => return Err(refused),
            }
        }
        for m in 0..M {
            self.weights[m] = backward(&factor, &forward(&factor, &moment[m]));
        }
        (self.gram, self.moment, self.residual, self.factor) = (gram, moment, residual, factor);
        Ok(())
    }

    fn saved_len() -> usize {
        8 * (D * D + M * D) + 9 * M
    }
}

fn dot<const D: usize>(a: &[f64; D], b: &[f64; D]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

/// The lower Cholesky factor of a symmetric positive definite matrix; `None` when it is not.
fn cholesky<const D: usize>(a: &[[f64; D]; D]) -> Option<[[f64; D]; D]> {
    let mut l = [[0.0; D]; D];
    for i in 0..D {
        for j in 0..=i {
            let sum = a[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            if i == j {
                if !(sum > 0.0) {
                    return None;
                }
                l[i][i] = sum.sqrt();
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    Some(l)
}

/// Solve `L z = x`.
fn forward<const D: usize>(l: &[[f64; D]; D], x: &[f64; D]) -> [f64; D] {
    let mut z = [0.0; D];
    for i in 0..D {
        z[i] = (x[i] - (0..i).map(|k| l[i][k] * z[k]).sum::<f64>()) / l[i][i];
    }
    z
}

/// Solve `Lᵀ θ = z`.
fn backward<const D: usize>(l: &[[f64; D]; D], z: &[f64; D]) -> [f64; D] {
    let mut theta = [0.0; D];
    for i in (0..D).rev() {
        theta[i] = (z[i] - (i + 1..D).map(|k| l[k][i] * theta[k]).sum::<f64>()) / l[i][i];
    }
    theta
}
