//! Today's surface: the same fixed set of verbs in every epoch (the baseline to beat).

use crate::head::HeadSet;
use crate::history::History;
use crate::WorkingSetPolicy;

#[derive(Debug, Clone)]
pub struct StaticSet {
    label: String,
    head: HeadSet,
}

impl StaticSet {
    pub fn new(label: impl Into<String>, head: HeadSet) -> Self {
        Self { label: label.into(), head }
    }
}

impl WorkingSetPolicy for StaticSet {
    fn name(&self) -> String {
        self.label.clone()
    }

    /// The fixed set, whatever her past and the budget: today's surface is not budgeted per
    /// epoch, it is what it is.
    fn head_for_epoch(&self, _history: &History<'_>, _budget_tokens: u32) -> HeadSet {
        self.head.clone()
    }
}
