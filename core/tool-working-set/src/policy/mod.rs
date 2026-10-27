//! The candidate policies. Each estimates the chance she calls each tool in the coming epoch
//! and hands it to the one decision rule, [`crate::head::choose`], at one shared price.

pub mod blend;
pub mod cold_start;
pub mod frequency;
pub mod static_set;
pub mod transition;

use crate::cost::CostModel;
use crate::head::{choose, HeadSet};
use crate::history::History;
use crate::vocab::ToolId;

/// What every estimating policy decides with: the measured price, and the verbs always held
/// (the one that finds the rest).
#[derive(Debug, Clone)]
pub struct Pricing {
    pub cost: CostModel,
    pub pinned: Vec<ToolId>,
}

impl Pricing {
    pub fn head(&self, history: &History<'_>, chance: &[f32], budget_tokens: u32) -> HeadSet {
        choose(history.vocab, chance, &self.cost, budget_tokens, &self.pinned)
    }
}
