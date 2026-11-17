//! Her genes at the start of EVERY turn she takes: a room turn, a question, and her own initiative
//! alike (GENE-REUSE-FORK-MINT §3d). Her genome ([`Genome`](crate::genome::persona_genome::Genome)) composes for the
//! turn's focused line on the served base, and her cycle, which owns the genes she runs with, adopts that answer
//! ([`WorkspaceCycle::adopt_genes`]): a new set pages in, the same set changes nothing, an unknown
//! genome keeps what she holds or defers her act. No turn path may skip it: a restart starts every
//! cycle empty, and before this an initiative turn after one ran on the base model (Benchy on the
//! M5, 2026-11-17 after the 21:41Z restart).

use crate::cognition::workspace::WorkspaceCycle;
use crate::genome::composer::Composition;

/// Compose her genes for a turn whose focus is `rendered` and have her cycle adopt them. Returns
/// whether her act may run this turn.
pub(crate) async fn page_turn_genes(cycle: &WorkspaceCycle, persona: uuid::Uuid, name: &str, rendered: &str) -> bool {
    let base = crate::inference::llama_server::current_serving().active_model;
    let composition = crate::genome::persona_genome::Genome::of(persona)
        .compose(base.as_deref(), crate::cognition::recall_faculty::focused_query(rendered))
        .await;
    let unknown = match &composition {
        Composition::Unknown(reason) => *reason,
        Composition::Genes(_) => "",
    };
    let adoption = cycle.adopt_genes(composition);
    continuum_base::probe!(
        class = "persona.genome.turn_genes",
        persona = %name,
        adoption = ?adoption,
        unknown = unknown,
        "her genes for this turn: paged, unchanged, held, or deferred (never an unknown read as empty)"
    );
    adoption.may_act()
}
