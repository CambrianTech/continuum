//! Each mind's decode graph kept across a switch (fork #170). The Core binds every mind into its one sequence, so the
//! engine cannot tell one mind's step from another's: the tier names her at her bind, and keeps a graph for each mind
//! it serves. Who it serves is counted, never configured: each mind's blocks hold a token of the tier's, so the count
//! is the blocks alive, and a mind whose blocks drop gives her graph back at the next bind.

use std::sync::Arc;

use llama::Context;

/// The tier's side: the token every resident's blocks hold, and how many graphs the context keeps now.
pub(super) struct KeptGraphs {
    residents: Arc<()>,
    kept_for: usize,
}

/// A mind's side, held in her blocks for as long as she is resident.
pub(super) struct Resident {
    _held: Arc<()>,
}

impl KeptGraphs {
    pub(super) fn new() -> Self {
        Self { residents: Arc::new(()), kept_for: 0 }
    }

    pub(super) fn resident(&self) -> Resident {
        Resident { _held: Arc::clone(&self.residents) }
    }

    /// At her bind into `seq`: a graph kept for each mind the tier serves, and `seq` named as hers.
    pub(super) fn bind(&mut self, ctx: &mut Context<'_>, seq: i32, owner: u64) {
        let minds = Arc::strong_count(&self.residents) - 1; // the tier holds one
        if minds != self.kept_for {
            ctx.keep_graphs(minds);
            self.kept_for = minds;
            continuum_base::probe!(class = "persona.graphs.kept", minds = minds, bytes = ctx.kept_graph_bytes(),
                "the context keeps a decode graph for each mind the tier serves");
        }
        ctx.name_graph_owner(seq, owner);
    }
}
