//! The head an epoch carries, and the one decision rule every policy shares.

use crate::cost::CostModel;
use crate::vocab::{ToolId, Vocabulary};

/// The tool schemas in the head of her prompt for one epoch, in canonical order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadSet {
    tools: Vec<ToolId>,
    tokens: u64,
}

impl HeadSet {
    /// The head holding exactly `tools`.
    pub fn of(vocab: &Vocabulary, mut tools: Vec<ToolId>) -> Self {
        tools.sort_unstable();
        tools.dedup();
        let tokens = vocab.tokens_of(&tools);
        Self { tools, tokens }
    }

    pub fn contains(&self, tool: ToolId) -> bool {
        self.tools.binary_search(&tool).is_ok()
    }

    pub fn tools(&self) -> &[ToolId] {
        &self.tools
    }

    /// What the head costs, prefilled once per epoch.
    pub fn tokens(&self) -> u64 {
        self.tokens
    }
}

/// The head that minimizes her expected cost, given `chance[t]`: the chance she calls tool
/// `t` this epoch (indexed by [`ToolId::index`]).
///
/// Holding `t` wastes its schema tokens `s` when she does not call it; leaving it out costs a
/// miss `m` when she does. So `t` belongs in the head exactly when `p m > s (1 - p)`. Within
/// `budget_tokens` the tools go in by expected saving per schema token. `pinned` (the verb that
/// finds the others) is always in, first.
pub fn choose(vocab: &Vocabulary, chance: &[f32], cost: &CostModel, budget_tokens: u32, pinned: &[ToolId]) -> HeadSet {
    let miss = cost.miss_tokens();
    let mut ranked: Vec<(f64, ToolId)> = vocab
        .ids()
        .filter(|tool| !pinned.contains(tool))
        .filter_map(|tool| {
            let p = f64::from(*chance.get(tool.index())?);
            let schema = f64::from(vocab.tokens(tool).max(1));
            let saving = p * miss - schema * (1.0 - p);
            (saving > 0.0).then_some((saving / schema, tool))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut tools: Vec<ToolId> = pinned.to_vec();
    let mut used = vocab.tokens_of(pinned);
    for (_, tool) in ranked {
        let schema = u64::from(vocab.tokens(tool));
        if used + schema <= u64::from(budget_tokens) {
            tools.push(tool);
            used += schema;
        }
    }
    HeadSet::of(vocab, tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::ToolSchema;

    fn vocab() -> Vocabulary {
        let schema = |name: &str, tokens| ToolSchema { name: name.into(), description: String::new(), tokens };
        Vocabulary::new(vec![schema("commands/help", 100), schema("code/read", 100), schema("code/edit", 500), schema("web/fetch", 100)])
            .expect("small vocabulary")
    }

    fn cost(miss_tokens: f64) -> CostModel {
        // one act decodes `miss_tokens` at equal rates and has no overhead: a miss costs exactly that
        CostModel { prefill_tokens_per_s: 1.0, decode_tokens_per_s: 1.0, act_decode_tokens: miss_tokens, act_overhead_ms: 0.0, samples: 1 }
    }

    // what this catches: the rule drifting from its derivation. With a miss at 900 tokens a
    // 100-token schema pays from p > 0.1 and a 500-token one from p > 0.357; the pinned finder
    // is in whatever its chance; the budget drops the lowest saving per token first.
    #[test]
    fn a_tool_is_held_exactly_when_its_expected_miss_outweighs_its_expected_waste() {
        let vocab = vocab();
        let id = |name| vocab.id(name).expect("listed");
        let (help, read, edit, fetch) = (id("commands/help"), id("code/read"), id("code/edit"), id("web/fetch"));
        let mut chance = vec![0.0f32; vocab.len()];
        chance[read.index()] = 0.11;
        chance[edit.index()] = 0.34;
        chance[fetch.index()] = 0.09;
        let head = choose(&vocab, &chance, &cost(900.0), 10_000, &[help]);
        assert_eq!(head.tools(), HeadSet::of(&vocab, vec![help, read]).tools());
        chance[edit.index()] = 0.40;
        let roomy = choose(&vocab, &chance, &cost(900.0), 10_000, &[help]);
        assert_eq!((roomy.tools().len(), roomy.tokens()), (3, 700));
        let tight = choose(&vocab, &chance, &cost(900.0), 250, &[help]);
        assert!(tight.contains(help) && tight.contains(read) && !tight.contains(edit), "{tight:?}");
    }
}
