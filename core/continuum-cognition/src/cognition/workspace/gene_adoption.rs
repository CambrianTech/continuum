//! What her cycle does with the genome the composer answered for a turn
//! ([`WorkspaceCycle::adopt_genes`](super::WorkspaceCycle::adopt_genes)). The cycle owns the genes
//! she runs with, so it owns this decision: a new set pages in, the same set changes nothing (her
//! slot's adapters never flip, so her KV stays valid), and an unknown genome is never read as an
//! empty one: she keeps the genome she holds, or her act waits (the M5, 2026-11-17: after a
//! restart her cycle held none and her acts ran on the base model).

use crate::ai::types::ActiveAdapterRequest;
use crate::genome::composer::Composition;

/// How her cycle adopted a turn's genes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneAdoption {
    /// The composer chose a set different from what she held: it is paged in.
    Paged,
    /// The composer chose exactly what she holds: nothing changes.
    Unchanged,
    /// Her genome is unknown this turn and she holds one: she keeps it.
    Held,
    /// Her genome is unknown and she holds none: her act waits; a base-model act would drop her
    /// learning silently.
    Defer,
}

impl GeneAdoption {
    /// PURE: the adoption of `composition` by a cycle holding `held`, and the genes to page in.
    pub fn decide(held: &[ActiveAdapterRequest], composition: Composition) -> (Self, Option<Vec<ActiveAdapterRequest>>) {
        match composition {
            Composition::Genes(phenotype) if phenotype.genes.as_slice() == held => (Self::Unchanged, None),
            Composition::Genes(phenotype) => (Self::Paged, Some(phenotype.genes)),
            Composition::Unknown(_) if held.is_empty() => (Self::Defer, None),
            Composition::Unknown(_) => (Self::Held, None),
        }
    }

    /// Whether her act may run this turn.
    pub fn may_act(self) -> bool {
        self != Self::Defer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::composer::Phenotype;

    fn gene(path: &str) -> ActiveAdapterRequest {
        ActiveAdapterRequest { name: "code/owner".into(), path: path.into(), domain: String::new(), scale: 1.0 }
    }

    fn genes(genes: Vec<ActiveAdapterRequest>) -> Composition {
        Composition::Genes(Phenotype { genes })
    }

    // what this catches (the M5 2026-11-17, Codex on #6117): a fresh cycle after a restart running
    // her act on the base model, whether the composer answered (her persisted gene must page in) or
    // could not (an unknown genome read as empty); and a repeated turn re-paging the same gene,
    // which flips her slot's adapters and clears her KV for nothing.
    #[test]
    fn her_cycle_adopts_her_genes_or_waits_never_the_base_model_by_default() {
        let aris = gene("/genome/Aris/code_owner/adapter.gguf");
        assert_eq!(GeneAdoption::decide(&[], genes(vec![aris.clone()])), (GeneAdoption::Paged, Some(vec![aris.clone()])), "fresh cycle: her gene pages in");
        let (fresh, paged) = GeneAdoption::decide(&[], Composition::Unknown("the gene trial file did not read"));
        assert_eq!((fresh, paged), (GeneAdoption::Defer, None), "fresh cycle, unknown genome: her act waits");
        assert!(!fresh.may_act());
        assert_eq!(GeneAdoption::decide(&[aris.clone()], Composition::Unknown("unknown base")), (GeneAdoption::Held, None), "she keeps the genome she holds");
        assert_eq!(GeneAdoption::decide(&[aris.clone()], genes(vec![aris.clone()])), (GeneAdoption::Unchanged, None), "the same gene is not re-paged");
        assert_eq!(GeneAdoption::decide(&[aris], genes(Vec::new())), (GeneAdoption::Paged, Some(Vec::new())), "no genes is an answer, distinct from unknown");
    }
}
