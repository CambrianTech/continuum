//! Her own habits: the decayed fraction of her past epochs that called each tool.

use crate::features::{occurrence, DAY_MS};
use crate::head::HeadSet;
use crate::history::History;
use crate::policy::Pricing;
use crate::WorkingSetPolicy;

#[derive(Debug, Clone)]
pub struct Frequency {
    /// `None`: every past epoch weighs the same.
    half_life_ms: Option<u64>,
    pricing: Pricing,
}

impl Frequency {
    pub fn new(half_life_ms: Option<u64>, pricing: Pricing) -> Self {
        Self { half_life_ms, pricing }
    }
}

impl WorkingSetPolicy for Frequency {
    fn name(&self) -> String {
        match self.half_life_ms {
            Some(half_life) if half_life % DAY_MS == 0 => format!("frequency {}d", half_life / DAY_MS),
            Some(half_life) => format!("frequency {}h", half_life / 3_600_000),
            None => "frequency all".to_owned(),
        }
    }

    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet {
        self.pricing.head(history, &occurrence(history, self.half_life_ms, None), budget_tokens)
    }
}

/// The society's habits: the other minds' recent usage, what a mind with no past of her own
/// starts from.
#[derive(Debug, Clone)]
pub struct Population {
    pricing: Pricing,
}

impl Population {
    pub fn new(pricing: Pricing) -> Self {
        Self { pricing }
    }
}

impl WorkingSetPolicy for Population {
    fn name(&self) -> String {
        "population (society)".to_owned()
    }

    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet {
        match history.population {
            Some(chance) => self.pricing.head(history, chance, budget_tokens),
            None => self.pricing.head(history, &[], budget_tokens),
        }
    }
}
