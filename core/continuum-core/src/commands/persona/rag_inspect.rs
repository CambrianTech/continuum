//! `persona/rag-inspect` — introspect what a persona's RAG pipeline would feed the
//! model at this step, as a typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand).
//!
//! Dep-holding: the command captures the owning module's
//! [`PersonaResolver`](crate::modules::persona_rag_inspect::PersonaResolver) (name →
//! persona_id + transcript reader + optional inference adapter). The command body is
//! the free fn
//! [`inspect_persona`](crate::modules::persona_rag_inspect::inspect_persona) — the
//! command and the module's tests share ONE implementation. Assembled by
//! [`PersonaRagInspectModule::commands`](crate::modules::persona_rag_inspect::PersonaRagInspectModule::commands).

use std::sync::Arc;

use crate::modules::persona_rag_inspect::{
    inspect_persona, PersonaResolver, RagInspectParams, RagInspectResult,
};
use crate::sdk_codegen::{CommandError, DynCommand};

crate::action_command! {
    /// Introspect a persona's RAG state: reconstruct the exact allocation + per-source
    /// deliveries the pipeline would feed the model right now, so any AI can answer
    /// "would I respond as it requests?", "which layer is broken?", and "is this
    /// contextually relevant?". Set `chainInference` to also capture the model's actual
    /// response. Read-only (AiSafe).
    pub struct PersonaRagInspect { resolver: Arc<dyn PersonaResolver> }
    name: "persona/rag-inspect",
    access: AiSafe,
    params: RagInspectParams,
    output: RagInspectResult,
    run(this, _ctx, p) => {
        inspect_persona(&this.resolver, p)
            .await
            .map_err(CommandError::Internal)
    }
}

/// The dep-holding `persona/rag-inspect` command object over the module's resolver.
/// Called from
/// [`PersonaRagInspectModule::commands`](crate::modules::persona_rag_inspect::PersonaRagInspectModule::commands).
pub fn command_objects(resolver: Arc<dyn PersonaResolver>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(PersonaRagInspect { resolver })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the command carries its `persona/rag-inspect` wire name (the
    // routing key every caller binds to) and stays AiSafe (read-only introspection). A
    // regression that renamed the path or widened access to a write-capable level —
    // letting an autonomous caller mutate through a "read" command — is caught here.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(PersonaRagInspect::NAME, "persona/rag-inspect");
        assert_eq!(PersonaRagInspect::ACCESS, AccessLevel::AiSafe);
    }
}
