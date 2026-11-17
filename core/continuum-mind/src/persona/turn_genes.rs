//! Her genes, paged into her cycle at the start of EVERY turn she takes: a room turn, a question,
//! and her own initiative alike (GENE-REUSE-FORK-MINT §3d: the genes whose regions this turn's
//! focused line is inside, weighted by depth, `genome::composer`).
//!
//! Before this owner only the room turn and the question composed her genes, and her initiative
//! turns ran on whatever her cycle still held. A core restart starts every cycle empty, so after
//! one, every initiative act ran on the base model with no gene until a room turn came (the M5,
//! 2026-11-17 after the 21:41Z restart: Benchy's requests had carried `Aris/code_owner` all day
//! and his first act after it carried none). The genome she runs is part of her execution-ready
//! state; no turn path may skip it, and an UNKNOWN genome is never read as an empty one: with no
//! genome known for her, her act waits instead of running on the base model.

use crate::ai::types::ActiveAdapterRequest;
use crate::cognition::workspace::WorkspaceCycle;

/// What a turn runs with, decided before it generates.
#[derive(Debug, PartialEq)]
pub(crate) enum TurnGenes {
    /// The composer chose this set and it differs from what she holds: page it in.
    Composed(Vec<ActiveAdapterRequest>),
    /// The composer chose exactly what she holds: nothing changes, so her KV stays valid.
    Unchanged,
    /// The composer could not answer and she holds a genome: she keeps it.
    Held,
    /// The composer could not answer and she holds none: her act waits; a base-model act would
    /// silently drop her learning.
    Defer,
}

impl TurnGenes {
    /// PURE: what a turn does given the genes she `held` and what the composer answered.
    pub(crate) fn decide(held: &[ActiveAdapterRequest], composed: Option<Vec<ActiveAdapterRequest>>) -> Self {
        match composed {
            Some(genes) if genes.as_slice() == held => Self::Unchanged,
            Some(genes) => Self::Composed(genes),
            None if held.is_empty() => Self::Defer,
            None => Self::Held,
        }
    }

    /// Whether her act may run this turn.
    pub(crate) fn may_act(&self) -> bool {
        !matches!(self, Self::Defer)
    }
}

/// Compose her genes for a turn whose focus is `rendered` and settle them on `cycle`. Returns whether
/// her act may run this turn ([`TurnGenes::may_act`]); composed genes move onto her cycle.
pub(crate) async fn page_turn_genes(cycle: &WorkspaceCycle, persona: uuid::Uuid, name: &str, rendered: &str) -> bool {
    let need = crate::cognition::recall_faculty::focused_query(rendered);
    let (composed, unknown) = match crate::genome::composer::compose(persona, need).await {
        crate::genome::composer::Composition::Genes(phenotype) => (Some(phenotype.genes), ""),
        crate::genome::composer::Composition::Unknown(reason) => (None, reason),
    };
    let held = cycle.genome();
    let decision = TurnGenes::decide(&held, composed);
    continuum_base::probe!(
        class = "persona.genome.turn_genes",
        persona = %name,
        decision = ?std::mem::discriminant(&decision),
        held = held.len() as u64,
        unknown = unknown,
        "her genes for this turn: composed, unchanged, held, or deferred (never an unknown read as empty)"
    );
    let may_act = decision.may_act();
    if let TurnGenes::Composed(genes) = decision {
        cycle.page_in(genes);
    }
    may_act
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gene(path: &str) -> ActiveAdapterRequest {
        ActiveAdapterRequest { name: "code/owner".into(), path: path.into(), domain: String::new(), scale: 1.0 }
    }

    // what this catches (the M5 2026-11-17, Codex on #6117): a fresh cycle after a restart running
    // her act on the base model, whether the composer answered (her persisted gene must be paged in)
    // or could not (an unknown genome read as empty); and a repeated turn re-paging the same gene,
    // which flips her slot's adapters and clears her KV for nothing.
    #[test]
    fn a_turn_runs_her_genes_or_waits_never_the_base_model_by_default() {
        let aris = gene("/genome/Aris/code_owner/adapter.gguf");
        // Fresh cycle, her persisted gene composed: it is paged in.
        assert_eq!(TurnGenes::decide(&[], Some(vec![aris.clone()])), TurnGenes::Composed(vec![aris.clone()]));
        // Fresh cycle, the composer could not answer: her act waits.
        let fresh = TurnGenes::decide(&[], None);
        assert_eq!(fresh, TurnGenes::Defer);
        assert!(!fresh.may_act());
        // She holds a genome and the composer could not answer: she keeps it.
        assert_eq!(TurnGenes::decide(&[aris.clone()], None), TurnGenes::Held);
        // The same gene again: nothing is re-paged, her KV stays valid.
        assert_eq!(TurnGenes::decide(&[aris.clone()], Some(vec![aris.clone()])), TurnGenes::Unchanged);
        // A genuinely empty composition (no genes for her) is a real answer, distinct from unknown.
        assert_eq!(TurnGenes::decide(&[aris.clone()], Some(vec![])), TurnGenes::Composed(vec![]));
    }
}
