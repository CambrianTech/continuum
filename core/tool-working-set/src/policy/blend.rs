//! The learned blend: a logistic regression over every signal (her recent and standing habits,
//! the activity's, the transition chain, the society, relevance), fitted on earlier epochs or
//! learning online from each epoch it served.

use crate::features::Signals;
use crate::head::HeadSet;
use crate::history::History;
use crate::logistic::{Example, Logistic};
use crate::policy::Pricing;
use crate::relevance::Relevance;
use crate::vocab::ToolId;
use crate::WorkingSetPolicy;

/// The examples one ended epoch teaches: every tool any signal named, and every tool she
/// called, each labelled with whether she called it.
pub fn examples(signals: &Signals, used: &[ToolId]) -> Vec<Example> {
    (0..signals.long.len())
        .filter(|tool| signals.any(*tool) || used.iter().any(|called| called.index() == *tool))
        .map(|tool| (signals.row(tool), used.iter().any(|called| called.index() == tool)))
        .collect()
}

pub struct Blend<R: Relevance> {
    label: String,
    relevance: R,
    model: Logistic,
    /// `Some(rate)`: keep learning after every epoch it served, at this step size.
    online_rate: Option<f64>,
    pricing: Pricing,
}

impl<R: Relevance> Blend<R> {
    /// A blend that keeps `model` fixed.
    pub fn fitted(model: Logistic, relevance: R, pricing: Pricing) -> Self {
        Self { label: "blend (fitted on train)".to_owned(), relevance, model, online_rate: None, pricing }
    }

    /// A blend that starts knowing nothing and learns from every epoch it serves.
    pub fn online(rate: f64, relevance: R, pricing: Pricing) -> Self {
        Self { label: "blend (online)".to_owned(), relevance, model: Logistic::default(), online_rate: Some(rate), pricing }
    }

    pub fn model(&self) -> &Logistic {
        &self.model
    }
}

impl<R: Relevance> WorkingSetPolicy for Blend<R> {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet {
        let signals = Signals::of(history, &self.relevance);
        let chance: Vec<f32> = (0..history.vocab.len())
            .map(|tool| if signals.any(tool) { self.model.chance(&signals.row(tool)) } else { 0.0 })
            .collect();
        self.pricing.head(history, &chance, budget_tokens)
    }

    fn observe(&mut self, history: &History<'_>, used: &[ToolId]) {
        if let Some(rate) = self.online_rate {
            let signals = Signals::of(history, &self.relevance);
            self.model.learn(&examples(&signals, used), rate);
        }
    }
}
