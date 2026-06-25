//! `rag/compose` — batched RAG context composition across N sources in one call.

use std::sync::Arc;
use std::time::Instant;

use crate::log_info;
use crate::logging::TimingGuard;
use crate::modules::rag::{RagComposeRequest, RagComposeResult, RagSourceResult, RagState};

crate::action_command! {
    /// Compose a persona's RAG context by loading every requested source (memory,
    /// consciousness, scene, project, custom) in ONE batched call and aggregating the
    /// sections within the token budget. Replaces N per-source IPC round-trips with a
    /// single Rust-side pass.
    pub struct RagCompose { state: Arc<RagState> }
    name: "rag/compose",
    access: AiSafe,
    params: RagComposeRequest,
    output: RagComposeResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "rag_compose");
        let start = Instant::now();

        let persona_id = p.persona_id.clone();
        let room_id = p.room_id.clone();
        let query_text = p.query_text.clone();
        let sources = p.sources.clone();

        // ════════════════════════════════════════════════════════════════════
        // SEQUENTIAL SOURCE LOADING (CRITICAL FIX)
        //
        // Previously used par_iter() but this caused Rayon thread starvation:
        // - IPC dispatch uses rayon::spawn() for each request
        // - Rayon threads block on rx.recv_timeout(30s) waiting for tokio
        // - Tokio calls the handler which used par_iter()
        // - par_iter() needs Rayon threads - but they're all blocked!
        //
        // Sequential iteration is fine because individual source loading is fast
        // (~5ms each) and there are typically only 2-3 sources per compose. Now
        // async: the memory source's query embedding is produced via the
        // adapter-routed embedder (task #40). Await each source in order — no
        // Rayon, no thread starvation.
        // ════════════════════════════════════════════════════════════════════
        let mut source_results: Vec<RagSourceResult> = Vec::with_capacity(sources.len());
        for source in &sources {
            source_results.push(
                this.state
                    .load_source(source, &persona_id, &room_id, query_text.as_deref())
                    .await,
            );
        }

        let total_tokens: usize = source_results.iter().map(|r| r.tokens_used).sum();
        let sources_succeeded = source_results.iter().filter(|r| r.success).count();
        let sources_failed = source_results.len() - sources_succeeded;
        let compose_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        log_info!(
            "module",
            "rag_compose",
            "RAG compose for {}: {} sources ({} ok, {} failed), {} tokens in {:.1}ms",
            persona_id,
            sources.len(),
            sources_succeeded,
            sources_failed,
            total_tokens,
            compose_time_ms
        );

        let result = RagComposeResult {
            source_results,
            total_tokens,
            compose_time_ms,
            sources_succeeded,
            sources_failed,
        };

        // Infallible: each source loader absorbs its own failure into its
        // RagSourceResult (success:false + error), so compose itself never errors —
        // it reports per-source health in `sources_failed`. The macro's signature
        // still carries the CommandError contract for uniformity.
        Ok(result)
    }
}
