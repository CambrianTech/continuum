//! Loop-filler dedup for the persona heartbeat — task #16 (self-echo suppression +
//! multi-hop loop dedup).
//!
//! # The pathology this exists to break
//!
//! Two idle personas sharing a room can fall into a **courtesy resonance**: each
//! self-tick, one emits a content-free pleasantry that names the other by id, the
//! other's heartbeat perceives a NEW airc item, wakes, emits its own pleasantry, and
//! so on — burning a full decode (~40 s on a 14B lane) per turn and flooding the room
//! with poisoned `assistant` precedent ([[false-refusal-anchor-present-but-positionally-defeated]]).
//! Proven live 2026-07-02, room cb2e21a1, personas 90e758b2 + 0d3209a1.
//!
//! # Why the wake gate lets it through
//!
//! `service_loop::burst_fingerprint` hashes each external airc item's content
//! VERBATIM. The captured contexts show the flood is not per-turn rewording but
//! **verbatim template cycling** — a handful of stock courtesy templates repeated
//! (observed 5×/3×/3× in one 11-message window). Each tick appends another COPY of an
//! already-seen template, so the item *list* grows and the fingerprint changes —
//! even though the deduped *set* of distinct turns is unchanged. A grown-but-not-novel
//! burst is exactly what `burst_fingerprint` is documented to filter OUT ("wake on a
//! CHANGE to what I should attend to"), and it fails to, because it keys on the raw
//! item stream rather than on novelty.
//!
//! # The fix (this module)
//!
//! Collapse near-duplicate airc turns to their first occurrence BEFORE the burst is
//! fingerprinted / turned into workspace turns / scanned for an address. Once the set
//! of distinct templates {A, B, C} is present, another copy of A does not change the
//! deduped set → the fingerprint is stable → no wake → no decode → the resonance
//! starves. The FIRST appearance of a genuinely new turn still changes the set → she
//! still wakes on real content.
//!
//! This is SCHEDULING / context hygiene, symmetric to the own-post exclusion already
//! in `burst_fingerprint` — NOT a heuristic steering cognition
//! ([[no-hardcoded-heuristics-to-steer-cognition]]). It never decides what she says;
//! it only refuses to re-present the same turn as if it were news.
//!
//! # Why exact-normalized primary + a conservative fuzzy pass
//!
//! Normalized-exact dedup alone breaks the observed verbatim flood. A trigram-Jaccard
//! pass (≥ [`NEAR_DUP_JACCARD`]) is cheap insurance against the near-verbatim variant —
//! the SAME courtesy template with a swapped addressee id ("Thank you for your
//! readiness, <peer-A>" vs "…<peer-B>"), which is one changed word and lands around
//! 0.72 trigram similarity. Two genuinely distinct substantive messages share almost
//! no trigrams, so even at 0.65 this cannot silently merge real content. HEAVY
//! rewording (a resonance that paraphrases every turn) is NOT reliably catchable
//! lexically — that is the job of the embedding-backed novelty judge (#9/#16), and
//! this module does not pretend to cover it.

use crate::persona::rag_budget::{RagDelivery, RagItem};
use std::collections::{HashSet, VecDeque};

/// airc-chat deliveries are tagged with this source id (mirrors the literal in
/// `service_loop::burst_fingerprint`). Only these items are deduped; curated sources
/// (doctrine, roster, active-work) are unique by construction and pass through.
const AIRC_SOURCE: &str = "airc";

/// Trigram-Jaccard at/above which two airc turns are treated as the SAME
/// contribution. Calibrated so a same-template/swapped-id courtesy (one changed word,
/// ≈0.72 similarity) collapses, while two different questions (near-zero shared
/// trigrams) stay apart. Not higher: normalization already folds punctuation/case, so
/// a stricter bar would leave the fuzzy pass with nothing real to catch.
const NEAR_DUP_JACCARD: f32 = 0.65;

/// How many recently-kept turns each new turn is fuzzy-compared against. Bounded so
/// the pass stays O(n·window), not O(n²), on a long history.
const FUZZY_WINDOW: usize = 16;

/// Collapse whitespace + case so trivial formatting differences don't defeat the
/// exact-match key.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Word trigrams of already-normalized text (falls back to the unigram set for text
/// shorter than 3 words so tiny turns still compare sensibly).
fn trigrams(normalized: &str) -> HashSet<String> {
    let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() < 3 {
        return words.into_iter().map(str::to_string).collect();
    }
    words.windows(3).map(|w| w.join(" ")).collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Order-preserving collapse of near-duplicate airc turns across all deliveries,
/// keeping the FIRST occurrence. Non-airc deliveries and empty-content items pass
/// through untouched. Both self- and peer-authored turns are deduped: repeated OWN
/// courtesy is the same contamination as repeated peer courtesy
/// ([[false-refusal-anchor-present-but-positionally-defeated]]), so feeding five copies
/// of one's own pleasantry as `assistant` precedent is exactly what we must not do.
pub fn dedup_loop_filler(deliveries: &[RagDelivery]) -> Vec<RagDelivery> {
    // Shared dedup state across ALL airc deliveries (chat history may arrive split).
    let mut seen_exact: HashSet<String> = HashSet::new();
    let mut recent_shingles: VecDeque<HashSet<String>> = VecDeque::new();

    let mut out = Vec::with_capacity(deliveries.len());
    for delivery in deliveries {
        if delivery.source_id != AIRC_SOURCE {
            out.push(delivery.clone());
            continue;
        }
        let mut kept: Vec<RagItem> = Vec::with_capacity(delivery.items.len());
        for item in &delivery.items {
            let norm = normalize(&item.content);
            if norm.is_empty() {
                kept.push(item.clone());
                continue;
            }
            if seen_exact.contains(&norm) {
                continue; // verbatim repeat (modulo whitespace/case) — drop
            }
            let shingles = trigrams(&norm);
            let near_dup = recent_shingles
                .iter()
                .any(|prev| jaccard(&shingles, prev) >= NEAR_DUP_JACCARD);
            if near_dup {
                continue; // near-identical rewording — drop
            }
            seen_exact.insert(norm);
            recent_shingles.push_back(shingles);
            if recent_shingles.len() > FUZZY_WINDOW {
                recent_shingles.pop_front();
            }
            kept.push(item.clone());
        }
        out.push(RagDelivery {
            items: kept,
            ..delivery.clone()
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::rag_budget::{ResolutionPreference, RagDelivery, RagItem};
    use serde_json::json;

    fn airc_item(content: &str, peer: &str) -> RagItem {
        RagItem {
            content: content.to_string(),
            tokens: 1,
            metadata: json!({ "peer_id": peer }),
        }
    }

    fn airc_delivery(items: Vec<RagItem>) -> RagDelivery {
        RagDelivery {
            source_id: "airc".to_string(),
            items,
            tokens_used: 0,
            continuation: None,
            resolution_used: ResolutionPreference::Placeholder,
        }
    }

    // what this catches: the courtesy-resonance regression (room cb2e21a1). Verbatim
    // template cycling must collapse to the distinct set so the wake fingerprint over
    // the deduped burst is STABLE when yet another copy is appended — no wake, no
    // 40s decode, the two-persona flood starves.
    #[test]
    fn verbatim_template_cycling_collapses_and_stabilizes() {
        let a = "You're welcome! I'm glad we're both ready for this discussion too!";
        let b = "Thank you for your readiness, 0d3209a1.";
        let c = "Thank you for your enthusiasm, 0d3209a1.";
        // The real captured window: A×5, B×3, C×3, interleaved.
        let items = vec![
            airc_item(a, "p1"),
            airc_item(b, "p2"),
            airc_item(a, "p1"),
            airc_item(c, "p2"),
            airc_item(a, "p1"),
            airc_item(b, "p2"),
            airc_item(a, "p1"),
            airc_item(c, "p2"),
            airc_item(a, "p1"),
            airc_item(b, "p2"),
            airc_item(c, "p2"),
        ];
        let deduped = dedup_loop_filler(&[airc_delivery(items)]);
        assert_eq!(deduped[0].items.len(), 3, "11 turns of 3 templates → 3 kept");

        // The anti-resonance property: append ANOTHER copy of an existing template →
        // the deduped set is unchanged (no new distinct turn to wake on).
        let mut grown: Vec<RagItem> = deduped[0].items.clone();
        // Simulate the next tick's full burst = prior 11 + one more copy of A.
        let next_burst = {
            let mut v: Vec<RagItem> = vec![
                airc_item(a, "p1"),
                airc_item(b, "p2"),
                airc_item(c, "p2"),
            ];
            v.push(airc_item(a, "p1")); // the new tick's repeat
            v
        };
        let deduped_next = dedup_loop_filler(&[airc_delivery(next_burst)]);
        grown.truncate(3);
        assert_eq!(
            deduped_next[0].items.len(),
            3,
            "appending a repeat must not grow the deduped set"
        );
    }

    // what this catches: over-collapse. A genuinely novel turn must survive dedup so
    // she still wakes on real content — the fix must not gag her.
    #[test]
    fn novel_turn_survives() {
        let a = "You're welcome! I'm glad we're both ready.";
        let novel = "Can you run commands/list and tell me how many tools are available?";
        let deduped = dedup_loop_filler(&[airc_delivery(vec![
            airc_item(a, "p1"),
            airc_item(a, "p1"),
            airc_item(novel, "p2"),
        ])]);
        assert_eq!(deduped[0].items.len(), 2, "1 courtesy + 1 novel = 2 kept");
        assert!(
            deduped[0].items.iter().any(|i| i.content == novel),
            "the novel question must survive"
        );
    }

    // what this catches: minor rewording (changed punctuation / one swapped word)
    // still collapses via the fuzzy pass, but two DISTINCT questions do not.
    #[test]
    fn near_identical_collapses_distinct_survives() {
        let base = "Thank you for your readiness, let us keep exploring the topic together";
        let reworded = "Thank you for your readiness, let us keep exploring the topics together"; // 1 word
        let distinct = "What is the time complexity of the recipe loader's dedup pass?";
        let deduped = dedup_loop_filler(&[airc_delivery(vec![
            airc_item(base, "p1"),
            airc_item(reworded, "p2"),
            airc_item(distinct, "p1"),
        ])]);
        assert_eq!(
            deduped[0].items.len(),
            2,
            "near-identical rewording collapses; the distinct question stays"
        );
        assert!(deduped[0].items.iter().any(|i| i.content == distinct));
    }

    // what this catches: non-airc curated sources (doctrine/roster/active-work) must
    // never be touched — only the chat thread has the resonance pathology.
    #[test]
    fn non_airc_sources_pass_through() {
        let repeated = "System doctrine line.";
        let mut d = airc_delivery(vec![]);
        d.source_id = "room-doctrine".to_string();
        d.items = vec![
            RagItem { content: repeated.to_string(), tokens: 1, metadata: json!({}) },
            RagItem { content: repeated.to_string(), tokens: 1, metadata: json!({}) },
        ];
        let deduped = dedup_loop_filler(&[d]);
        assert_eq!(
            deduped[0].items.len(),
            2,
            "curated non-airc items pass through untouched"
        );
    }
}
