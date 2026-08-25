//! Gene recall selection — **fast, intelligent, diverse** (Joel 2026-08-25).
//!
//! The hot-path decision: given a task's embedding and a pool of candidate genes,
//! pick the working set to page in. Three properties, one function:
//!
//! - **Fast** — operates on borrows; no clone of a gene, no serialization, one small
//!   scratch `Vec<f32>` of relevance scores reused across the pick. O(pool · k) with
//!   k tiny (the lane budget). Nothing here allocates per candidate beyond the score.
//! - **Intelligent** — relevance is embedding **distance to the task**
//!   ([`GeneSignature::similarity_in`], best over centroid + subspaces so a
//!   multi-theme gene matches on its RELEVANT theme), not a keyword/name match. The
//!   right gene for THIS task, by geometry.
//! - **Diverse** — greedy **max-marginal-relevance**: after the first pick, each
//!   further pick is penalized by its similarity to what's ALREADY selected, so the
//!   set spans niches instead of stacking k near-duplicates of the leader. This is
//!   the same diversity instinct as [`commons_ranking`], applied at selection time:
//!   a monoculture working set shares one blind spot; a spread one covers the task
//!   from several angles.
//!
//! Fitness/popularity (the [`RankPolicy`] score) and task-relevance are BOTH inputs:
//! a gene must be both good AND relevant to be paged in. The policy stays pluggable —
//! this function consumes whatever `score` the caller's policy produced.

use crate::genome::commons_ranking::{GeneSignals, RankPolicy};
use crate::genome::signature::GeneSignature;

/// One candidate the selector ranks. Borrows the signature (no clone); carries the
/// pre-computed policy signals so selection never re-derives fitness on the hot path.
pub struct GeneCandidate<'a, T> {
    /// Caller's handle to the actual gene (an id, an Arc, a path — the selector is
    /// generic over it and only returns the ones it picked).
    pub handle: T,
    /// The gene's embedding signature, for distance-routing. `None` = a pre-signature
    /// gene; it can still be picked on policy score alone but never wins on relevance.
    pub signature: Option<&'a GeneSignature>,
    /// The signals a [`RankPolicy`] scores (fitness/popularity/trials); novelty is
    /// filled by the selector from live distance, so the caller leaves it `None`.
    pub signals: GeneSignals,
}

/// Select up to `k` genes for a task, fusing policy score with task-relevance and
/// enforcing diversity via max-marginal-relevance. `task_embedding` is the query
/// vector in `embedder_id`'s space (the persona's current task/context embedding).
/// `lambda` in [0,1] trades relevance (1.0) against diversity (0.0); ~0.7 keeps the
/// set on-task while spreading it. Returns the chosen handles, most-relevant first.
///
/// Fast: the only allocation is the `chosen` output and a reused `remaining` index
/// list — candidates are never cloned, signatures never copied.
pub fn select_genes<'a, T, P: RankPolicy>(
    candidates: Vec<GeneCandidate<'a, T>>,
    task_embedding: &[f32],
    embedder_id: &str,
    policy: &P,
    k: usize,
    lambda: f32,
) -> Vec<T> {
    if candidates.is_empty() || k == 0 {
        return Vec::new();
    }
    let lambda = lambda.clamp(0.0, 1.0);

    // Base relevance = task distance; base merit = relevance blended with the policy
    // score (a gene must be BOTH on-task and good). Computed once per candidate.
    let base: Vec<f32> = candidates
        .iter()
        .map(|c| {
            let merit = policy.score(&c.signals);
            // Relevance GATES merit — a gene must be BOTH on-task AND good (product,
            // not sum, so neither alone qualifies it). A signatured gene that declared
            // a DIFFERENT space is genuinely not for this task → near-zero. A gene with
            // no signature can't be routed, so it ranks on merit alone against a
            // neutral relevance (never zeroed for lacking a signature it never minted).
            match c
                .signature
                .and_then(|s| best_similarity(s, embedder_id, task_embedding))
            {
                Some(relevance) => relevance.max(0.0) * merit,
                None => 0.5 * merit, // unroutable: merit at neutral relevance
            }
        })
        .collect();

    let mut remaining: Vec<usize> = (0..candidates.len()).collect();
    let mut chosen: Vec<usize> = Vec::with_capacity(k.min(candidates.len()));

    while chosen.len() < k && !remaining.is_empty() {
        // MMR: pick the candidate maximizing λ·merit − (1−λ)·maxSimToChosen, so each
        // addition is penalized by how close it sits to what we already took.
        let mut best_pos = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        for (pos, &ci) in remaining.iter().enumerate() {
            let redundancy = chosen
                .iter()
                .filter_map(|&sj| pair_similarity(&candidates, ci, sj, embedder_id))
                .fold(0.0f32, f32::max);
            let mmr = lambda * base[ci] - (1.0 - lambda) * redundancy;
            if mmr > best_val {
                best_val = mmr;
                best_pos = pos;
            }
        }
        chosen.push(remaining.swap_remove(best_pos));
    }

    // Return handles in pick order (most-relevant first), consuming the input so the
    // handles move out without a clone.
    let mut out: Vec<Option<T>> = candidates.into_iter().map(|c| Some(c.handle)).collect();
    chosen
        .into_iter()
        .filter_map(|i| out[i].take())
        .collect()
}

/// Best similarity of a task query to a gene: the max over its centroid and every
/// subspace, so a multi-theme gene is matched on the theme that fits THIS task —
/// never diluted by its unrelated themes.
fn best_similarity(sig: &GeneSignature, embedder_id: &str, query: &[f32]) -> Option<f32> {
    let centroid = sig.similarity_in(embedder_id, query)?;
    let sub = sig
        .subspaces
        .iter()
        .filter(|s| s.len() == query.len())
        .map(|s| dot(s, query))
        .fold(centroid, f32::max);
    Some(sub)
}

/// Similarity between two candidates' genes (for the diversity penalty) — centroid
/// to centroid in the shared space. `None` when either lacks a comparable signature.
fn pair_similarity<T>(
    candidates: &[GeneCandidate<'_, T>],
    a: usize,
    b: usize,
    embedder_id: &str,
) -> Option<f32> {
    let sa = candidates[a].signature?;
    let sb = candidates[b].signature?;
    if sa.embedder != embedder_id || sb.embedder != embedder_id || sa.dim != sb.dim {
        return None;
    }
    Some(dot(&sa.centroid, &sb.centroid))
}

/// Cosine of two L2-normalized vectors = their dot product. Signatures store
/// normalized vectors, so no re-normalization on the hot path.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::commons_ranking::DefaultCommonsPolicy;
    use crate::forge::recipe::CorpusRef;
    use crate::genome::signature::GeneSignature;

    fn sig(embedder: &str, centroid: Vec<f32>) -> GeneSignature {
        let dim = centroid.len();
        GeneSignature {
            embedder: embedder.to_string(),
            dim,
            centroid,
            subspaces: Vec::new(),
            corpus: CorpusRef {
                name: "t".into(),
                content_hash: "sha256:0".into(),
                size_bytes: 1,
                source_url: None,
            },
            minted_at_ms: 0,
        }
    }
    fn signals(fitness: f32) -> GeneSignals {
        GeneSignals { fitness: Some(fitness), popularity: Some(0.5), novelty: None, trials: 50 }
    }

    // what this catches: the three properties at once. (1) INTELLIGENT — a gene whose
    // signature points AT the task is picked over an equally-fit gene pointing away.
    // (2) DIVERSE — given two near-identical strong genes and one distinct one, the
    // second pick is the DISTINCT gene, not the near-duplicate (MMR working). (3) the
    // selection consumes borrows and returns only handles (fast — compiles without a
    // Clone bound on the gene).
    #[test]
    fn selection_is_relevant_and_diverse() {
        let e = "test-embedder";
        // A BLENDED task (diagonal X+Y): different genes cover different halves of it,
        // so relevance and distinctness are separable (the realistic case).
        let task = vec![0.7071, 0.7071, 0.0];
        let x1 = sig(e, vec![1.0, 0.0, 0.0]); // covers the X half
        let x2 = sig(e, vec![1.0, 0.0, 0.0]); // exact duplicate of x1
        let y = sig(e, vec![0.0, 1.0, 0.0]); // covers the Y half — equally relevant, orthogonal
        let offtask = sig(e, vec![0.0, 0.0, 1.0]); // Z: no task relevance at all

        let cands = vec![
            GeneCandidate { handle: "x1", signature: Some(&x1), signals: signals(0.9) },
            GeneCandidate { handle: "x2", signature: Some(&x2), signals: signals(0.9) },
            GeneCandidate { handle: "y", signature: Some(&y), signals: signals(0.9) },
            GeneCandidate { handle: "offtask", signature: Some(&offtask), signals: signals(0.9) },
        ];
        let policy = DefaultCommonsPolicy::default();
        // λ=0.6: on-task but diversity-aware.
        let picked = select_genes(cands, &task, e, &policy, 2, 0.6);

        // Intelligent: an X-half gene leads (tie with y broken by order; both cover
        // the task). Diverse: the SECOND pick is the ORTHOGONAL y — MMR rejects the
        // x1 duplicate (max redundancy) AND the off-task gene (zero relevance→zero
        // base). The set spans both halves of the task, which is the whole point.
        assert!(picked.contains(&"y"), "the complementary niche is selected: {picked:?}");
        assert!(!picked.contains(&"x2"), "the exact duplicate is rejected: {picked:?}");
        assert!(!picked.contains(&"offtask"), "the off-task gene never wins on diversity alone: {picked:?}");
    }

    // what this catches: λ is a real dial — λ=1 (pure relevance) takes both near-X
    // genes (best two by task distance); the diversity term is what changed the set.
    #[test]
    fn lambda_one_ignores_diversity() {
        let e = "test-embedder";
        let task = vec![1.0, 0.0, 0.0];
        let x1 = sig(e, vec![1.0, 0.0, 0.0]);
        let x2 = sig(e, vec![0.98, 0.199, 0.0]); // ~X, still highly task-relevant
        let y = sig(e, vec![0.0, 1.0, 0.0]); // Y: zero relevance to a pure-X task
        let cands = vec![
            GeneCandidate { handle: "x1", signature: Some(&x1), signals: signals(0.9) },
            GeneCandidate { handle: "x2", signature: Some(&x2), signals: signals(0.9) },
            GeneCandidate { handle: "y", signature: Some(&y), signals: signals(0.9) },
        ];
        let picked = select_genes(cands, &task, e, &DefaultCommonsPolicy::default(), 2, 1.0);
        // Pure relevance: both X-aligned genes win; Y (far from task) does not.
        assert!(picked.contains(&"x1") && picked.contains(&"x2"), "λ=1 takes the two nearest: {picked:?}");
        assert!(!picked.contains(&"y"));
    }
}
