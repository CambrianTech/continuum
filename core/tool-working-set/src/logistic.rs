//! Logistic regression over the blend's input rows: fitted once in a batch, or online.
//!
//! Inputs are standardized by a running mean and variance (Welford), so the online learner
//! never scales by numbers from its future; the last column is the bias and is not scaled.

use crate::features::FEATURES;

/// One training example: a tool's input row at a turnover, and whether she called it.
pub type Example = ([f32; FEATURES], bool);

/// Serializable so a mind's learned weights and running scale survive a restart (the core
/// keeps them as an ORM entity).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Logistic {
    weights: [f64; FEATURES],
    count: f64,
    mean: [f64; FEATURES],
    squares: [f64; FEATURES],
}

impl Default for Logistic {
    fn default() -> Self {
        Self { weights: [0.0; FEATURES], count: 0.0, mean: [0.0; FEATURES], squares: [0.0; FEATURES] }
    }
}

impl Logistic {
    /// Batch gradient descent on the mean log-loss of `examples`, scaled by their own moments.
    pub fn fit(examples: &[Example], iterations: usize, rate: f64) -> Self {
        let mut model = Self::default();
        for (row, _) in examples {
            model.observe_scale(row);
        }
        let scaled: Vec<([f64; FEATURES], f64)> =
            examples.iter().map(|(row, called)| (model.scale(row), if *called { 1.0 } else { 0.0 })).collect();
        let count = scaled.len().max(1) as f64;
        for _ in 0..iterations {
            let mut gradient = [0.0f64; FEATURES];
            for (x, y) in &scaled {
                let error = sigmoid(dot(&model.weights, x)) - y;
                for (g, xi) in gradient.iter_mut().zip(x) {
                    *g += error * xi;
                }
            }
            for (w, g) in model.weights.iter_mut().zip(gradient) {
                *w -= rate * g / count;
            }
        }
        model
    }

    /// One online step: fold `examples` into the scale, then one gradient step per example.
    pub fn learn(&mut self, examples: &[Example], rate: f64) {
        for (row, _) in examples {
            self.observe_scale(row);
        }
        for (row, called) in examples {
            let x = self.scale(row);
            let error = sigmoid(dot(&self.weights, &x)) - if *called { 1.0 } else { 0.0 };
            for (w, xi) in self.weights.iter_mut().zip(x) {
                *w -= rate * error * xi;
            }
        }
    }

    /// The chance she calls the tool whose input row is `row`.
    pub fn chance(&self, row: &[f32; FEATURES]) -> f32 {
        sigmoid(dot(&self.weights, &self.scale(row))) as f32
    }

    /// The weights on the standardized inputs, bias last: how much each signal mattered.
    pub fn weights(&self) -> [f64; FEATURES] {
        self.weights
    }

    fn observe_scale(&mut self, row: &[f32; FEATURES]) {
        self.count += 1.0;
        for ((mean, squares), value) in self.mean.iter_mut().zip(self.squares.iter_mut()).zip(row) {
            let value = f64::from(*value);
            let delta = value - *mean;
            *mean += delta / self.count;
            *squares += delta * (value - *mean);
        }
    }

    fn scale(&self, row: &[f32; FEATURES]) -> [f64; FEATURES] {
        let mut x = [0.0f64; FEATURES];
        for (index, value) in row.iter().enumerate() {
            let value = f64::from(*value);
            x[index] = if index + 1 == FEATURES {
                value
            } else {
                let deviation = if self.count > 1.0 { (self.squares[index] / (self.count - 1.0)).sqrt() } else { 0.0 };
                if deviation > 0.0 { (value - self.mean[index]) / deviation } else { 0.0 }
            };
        }
        x
    }
}

fn dot(weights: &[f64; FEATURES], x: &[f64; FEATURES]) -> f64 {
    weights.iter().zip(x).map(|(w, xi)| w * xi).sum()
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z.clamp(-30.0, 30.0)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(signal: f32) -> [f32; FEATURES] {
        let mut row = [0.0; FEATURES];
        row[0] = signal;
        row[FEATURES - 1] = 1.0;
        row
    }

    // what this catches: a sign or scaling error that would make the blend read evidence for
    // a tool as evidence against it, in either the batch fit or the online learner.
    #[test]
    fn both_learners_rank_a_tool_with_more_evidence_higher() {
        let examples: Vec<Example> = (0..200).map(|i| (row((i % 10) as f32), i % 10 >= 7)).collect();
        let batch = Logistic::fit(&examples, 300, 0.5);
        assert!(batch.chance(&row(9.0)) > 0.8 && batch.chance(&row(0.0)) < 0.2, "{:?}", batch.weights());
        let mut online = Logistic::default();
        for chunk in examples.chunks(10) {
            online.learn(chunk, 0.05);
        }
        assert!(online.chance(&row(9.0)) > online.chance(&row(0.0)), "{:?}", online.weights());
    }
}
