//! `embedding/groups` — group TEXT candidates that say the same thing, in the ONE
//! process-wide embedding space.
//!
//! The text-level twin of `embedding/cluster` (which takes vectors). Its one job is
//! the duplicate listing: "which of these 118 cards are the same card" — measured
//! today as astropy-12907 open eight times — but it is written over `(id, text)`
//! so the same call groups people by bio, documents by body, anything.
//!
//! Grouping is connected components over pairs at or above a cosine floor. The
//! floor is derived from the embedder's MEASURED unrelated-null when it has one
//! (`mean + min_z × std`), so the call means the same thing under every embedder;
//! a raw `min_similarity` is honoured only when the embedder has not calibrated,
//! and the result names which rule decided (`gate`).
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::similar::SimilarCandidate;
use crate::cognition::embedding::EmbeddingProvider;
use crate::modules::embedding::detect_clusters;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

fn default_min_z() -> f32 {
    3.0
}
fn default_min_similarity() -> f32 {
    0.85
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/GroupsParams.ts"
)]
pub struct GroupsParams {
    pub candidates: Vec<SimilarCandidate>,
    /// Significance floor (standard deviations above the unrelated-pair mean) that
    /// joins two candidates, when the embedder is calibrated. Default 3.
    #[serde(default = "default_min_z")]
    pub min_z: f32,
    /// Raw cosine floor that joins two candidates, used ONLY when the embedder has
    /// no measured null. Default 0.85.
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarGroup.ts"
)]
pub struct SimilarGroup {
    /// Member ids, the representative (highest mean similarity to the rest) first.
    pub ids: Vec<String>,
    /// Mean intra-group cosine.
    pub cohesion: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/GroupsResult.ts"
)]
pub struct GroupsResult {
    /// Groups of two or more, strongest first. Singletons are not groups.
    pub groups: Vec<SimilarGroup>,
    pub group_count: usize,
    /// How many candidates sit in some group — the number a triage can retire.
    pub grouped: usize,
    pub total_candidates: usize,
    pub space: String,
    /// `"z"` (floor derived from the measured null) or `"threshold"` (raw floor).
    pub gate: String,
    /// The cosine floor that actually joined pairs, after the gate was applied.
    pub joined_at: f32,
    pub duration_ms: u64,
}

/// Group `candidates` in `embedder`'s space. Pure over the provider: the command
/// and `work/duplicates` both ride this.
pub async fn group_texts(
    embedder: &Arc<dyn EmbeddingProvider>,
    candidates: &[SimilarCandidate],
    min_z: f32,
    min_similarity: f32,
) -> Result<GroupsResult, CommandError> {
    let start = Instant::now();
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(candidates.len());
    for c in candidates {
        vectors.push(embedder.embed(&c.text).await);
    }
    let null = embedder.unrelated_null();
    let (gate, joined_at) = match null {
        Some((mean, std)) => ("z", mean + min_z * std),
        None => ("threshold", min_similarity),
    };
    let clusters = detect_clusters(&vectors, joined_at, 2);
    let groups: Vec<SimilarGroup> = clusters
        .into_iter()
        .map(|c| {
            let mut ids: Vec<String> = Vec::with_capacity(c.indices.len());
            ids.push(candidates[c.representative].id.clone());
            ids.extend(
                c.indices
                    .iter()
                    .filter(|i| **i != c.representative)
                    .map(|i| candidates[*i].id.clone()),
            );
            SimilarGroup {
                ids,
                cohesion: c.strength,
            }
        })
        .collect();
    let grouped = groups.iter().map(|g| g.ids.len()).sum();
    Ok(GroupsResult {
        group_count: groups.len(),
        groups,
        grouped,
        total_candidates: candidates.len(),
        space: embedder.id().to_string(),
        gate: gate.to_string(),
        joined_at,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub struct EmbeddingGroups {
    pub embedder: Arc<dyn EmbeddingProvider>,
}

#[async_trait]
impl ActionCommand for EmbeddingGroups {
    const NAME: &'static str = "embedding/groups";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Group text candidates that say the same thing (ids + texts in, groups of ids out, \
         strongest first, representative first within each group). The duplicate finder: hand it \
         card titles, bios, or documents. Pairs join at a floor derived from the embedder's \
         measured unrelated-null (minZ, default 3 standard deviations); the result names the \
         floor it used and which gate decided.";
    type Params = GroupsParams;
    type Output = GroupsResult;

    async fn run(&self, _ctx: &Ctx, p: GroupsParams) -> Result<GroupsResult, CommandError> {
        group_texts(&self.embedder, &p.candidates, p.min_z, p.min_similarity).await
    }
}

crate::register_command!(EmbeddingGroups);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::DeterministicEmbeddingProvider;

    // what this catches: identical texts under different ids land in ONE group with
    // the representative first, unrelated text stays out, and an uncalibrated
    // embedder reports `gate: threshold` with the raw floor it actually joined at.
    #[tokio::test]
    async fn identical_texts_group_and_the_gate_is_named() {
        let e: Arc<dyn EmbeddingProvider> = Arc::new(DeterministicEmbeddingProvider);
        let same = "Bug in expand of TensorProduct + Workaround + Fix";
        let cands: Vec<SimilarCandidate> = [
            ("a", same),
            ("b", same),
            (
                "c",
                "Live grid overflows the viewport at five or more tiles",
            ),
            ("d", same),
        ]
        .into_iter()
        .map(|(id, text)| SimilarCandidate {
            id: id.into(),
            text: text.into(),
        })
        .collect();
        let out = group_texts(&e, &cands, 3.0, 0.95).await.expect("groups");
        assert_eq!(out.gate, "threshold");
        assert_eq!(out.joined_at, 0.95);
        assert_eq!(out.group_count, 1, "{:?}", out.groups);
        let mut ids = out.groups[0].ids.clone();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "d"]);
        assert_eq!(out.grouped, 3);
        assert_eq!(out.total_candidates, 4);
    }
}
