//! The cold start: no habits read at all, only how relevant each tool is to what she is
//! doing, turned into a chance by a calibration learned from earlier epochs.

use crate::head::HeadSet;
use crate::history::History;
use crate::policy::Pricing;
use crate::relevance::Relevance;
use crate::WorkingSetPolicy;

/// How often a tool was called in epochs where its relevance fell in each quantile bin.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Upper relevance bound of every bin but the last, ascending.
    edges: Vec<f32>,
    /// Observed rate per bin, with one call and one non-call of prior (Laplace), so an empty
    /// bin reads one half rather than a certainty.
    rates: Vec<f32>,
}

impl Calibration {
    /// Fit `bins` equal-count bins over `examples` (relevance, whether she called the tool).
    pub fn fit(examples: &[(f32, bool)], bins: usize) -> Self {
        let mut relevances: Vec<f32> = examples.iter().map(|(relevance, _)| *relevance).collect();
        relevances.sort_by(f32::total_cmp);
        let bins = bins.max(1);
        let edges: Vec<f32> = (1..bins)
            .filter_map(|bin| relevances.get(bin * relevances.len() / bins).copied())
            .collect();
        let mut called = vec![0.0f32; edges.len() + 1];
        let mut seen = vec![0.0f32; edges.len() + 1];
        let mut calibration = Self { edges, rates: Vec::new() };
        for (relevance, was_called) in examples {
            let bin = calibration.bin(*relevance);
            seen[bin] += 1.0;
            if *was_called {
                called[bin] += 1.0;
            }
        }
        calibration.rates = called.iter().zip(&seen).map(|(called, seen)| (called + 1.0) / (seen + 2.0)).collect();
        calibration
    }

    fn bin(&self, relevance: f32) -> usize {
        self.edges.iter().position(|edge| relevance < *edge).unwrap_or(self.edges.len()) // unwrap_or: above every edge is the last bin
    }

    pub fn chance(&self, relevance: f32) -> f32 {
        self.rates[self.bin(relevance)]
    }
}

pub struct ColdStart<R: Relevance> {
    relevance: R,
    calibration: Calibration,
    pricing: Pricing,
}

impl<R: Relevance> ColdStart<R> {
    pub fn new(relevance: R, calibration: Calibration, pricing: Pricing) -> Self {
        Self { relevance, calibration, pricing }
    }
}

impl<R: Relevance> WorkingSetPolicy for ColdStart<R> {
    fn name(&self) -> String {
        "relevance cold start".to_owned()
    }

    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet {
        let context = history.recent_text(crate::features::RECENT_ACTS);
        let chance: Vec<f32> = if context.is_empty() {
            Vec::new()
        } else {
            self.relevance.relevance(&context).into_iter().map(|relevance| self.calibration.chance(relevance)).collect()
        };
        self.pricing.head(history, &chance, budget_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the calibration reading relevance backwards, or a bin with no
    // examples claiming a certainty.
    #[test]
    fn calibration_maps_higher_relevance_to_the_rate_observed_there() {
        let examples: Vec<(f32, bool)> = (0..100).map(|i| (i as f32 / 100.0, i >= 80)).collect();
        let calibration = Calibration::fit(&examples, 5);
        assert!(calibration.chance(0.95) > 0.9, "{calibration:?}");
        assert!(calibration.chance(0.05) < 0.1, "{calibration:?}");
        let empty = Calibration::fit(&[], 5);
        assert_eq!(empty.chance(0.5), 0.5);
    }
}
