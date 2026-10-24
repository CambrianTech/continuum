//! What tends to follow what she just did: her call-to-call chain run forward from her last call.

use crate::features::transition;
use crate::head::HeadSet;
use crate::history::History;
use crate::policy::Pricing;
use crate::WorkingSetPolicy;

#[derive(Debug, Clone)]
pub struct Transition {
    pricing: Pricing,
}

impl Transition {
    pub fn new(pricing: Pricing) -> Self {
        Self { pricing }
    }
}

impl WorkingSetPolicy for Transition {
    fn name(&self) -> String {
        "transition".to_owned()
    }

    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet {
        self.pricing.head(history, &transition(history), budget_tokens)
    }
}
