//! Her genes, paged into her cycle at the start of EVERY turn she takes: a room turn, a question,
//! and her own initiative alike (GENE-REUSE-FORK-MINT §3d: the genes whose regions this turn's
//! focused line is inside, weighted by depth, `genome::composer`).
//!
//! Before this owner only the room turn and the question composed her genes, and her initiative
//! turns ran on whatever her cycle still held. A core restart starts every cycle empty, so after
//! one, every initiative act ran on the base model with no gene until a room turn came (the M5,
//! 2026-11-17 after the 21:41Z restart: Benchy's requests had carried `Aris/code_owner` all day
//! and his first act after it carried none). The genome she runs is part of her execution-ready
//! state; no turn path may skip it.

use crate::cognition::workspace::WorkspaceCycle;

/// Compose her genes for a turn whose focus is `rendered` and page them into `cycle`. When the
/// composer cannot read her genome it says why itself and she keeps the genome she has.
pub(crate) async fn page_turn_genes(cycle: &WorkspaceCycle, persona: uuid::Uuid, name: &str, rendered: &str) {
    let need = crate::cognition::recall_faculty::focused_query(rendered);
    let Some(phenotype) = crate::genome::composer::compose(persona, need).await else {
        return;
    };
    continuum_base::probe!(
        class = "persona.genome.turn_genes",
        persona = %name,
        genes = phenotype.genes.len() as u64,
        phenotype = %crate::genome::composer::render(&phenotype),
        "the phenotype this turn runs with: each gene and its scale"
    );
    cycle.page_in(phenotype.genes);
}
