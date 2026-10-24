//! Which of her genes shape THIS turn, and how much (GENE-REUSE-FORK-MINT §3d, the APPLIED
//! tier): every gene whose region the turn's need is inside, several at once, weighted by
//! how deep inside each one is, the way a mixture of experts routes. Joel, 2026-10-24:
//! "Goal was many with weights."
//!
//! The one owner of that decision. The turn sites ask [`live_genes`]; the pure core is
//! [`select`], so the rule is tested without a node.
//!
//! # The rule
//!
//! - **Inside.** A gene's region edge (`enter`) is its own spread
//!   ([`GeneSignature::region`](crate::genome::signature::GeneSignature)), never below the
//!   space's measured unrelated null + 2σ (a diffuse gene must not ride every turn). A gene
//!   with neither, or signed in another embedding space, cannot be routed by distance: it
//!   stays in her head at its trained scale, as every gene did before routing existed
//!   (a gene never leaves her head by silence), and the probe names it unrouted.
//! - **Hysteresis.** Enter at distance `r = 1 - enter`, leave at `1.5 r` (§3d), so a topic
//!   at a region's edge does not switch the gene every turn.
//! - **Weights: a convex mix of each gene's depth inside its own region.** `w_i = d_i / Σ d`,
//!   `d_i = (sim_i - leave_i) / (1 - leave_i)`: 0 at the edge where the gene switches off,
//!   1 at its center, so a broad gene and a narrow one compare on the same footing (a raw
//!   margin would hand every turn to the broadest gene). Chosen over a softmax because (1) a gene's weight falls
//!   continuously to zero at the edge where it switches off, so drifting out of a region
//!   never steps the mix; (2) one gene alone runs at its trained scale 1.0, exactly what
//!   it did before; (3) the deltas never sum past one trained adapter's magnitude. A
//!   softmax needs a temperature, which is a constant or a second use of the null's σ, and
//!   gives a gene that barely entered a large share.
//! - **Held while the set holds.** The engine clears a slot's KV whenever its adapter
//!   scales change at all (fork `are_lora_equal` compares scales exactly; server-context
//!   `lora_should_clear_cache`), so re-weighting each turn would re-prefill her whole
//!   context every turn. The weights are computed when the applied SET changes (which costs
//!   that re-prefill anyway) and held bit-identical until it changes again.
//! - **No need, no change.** Without a need embedding (nothing to embed, no embedder), the
//!   routed genes stay as they were.

use std::collections::{BTreeMap, HashMap};

use parking_lot::Mutex;
use uuid::Uuid;

use crate::ai::types::ActiveAdapterRequest;
use crate::genome::signature::{GeneSignature, SignatureStore};

/// How many σ above the space's unrelated null a region edge must sit: the same 2σ
/// discipline recall gates with (`recall_faculty`'s significance ranker).
const NULL_SIGMAS: f32 = 2.0;

/// The leave edge's distance, as a multiple of the enter edge's (GENE-REUSE-FORK-MINT §3d:
/// "enter at r, leave at 1.5 r").
const LEAVE_DISTANCE_RATIO: f32 = 1.5;

/// The turn's need, embedded once by the caller (selection stays pure and sync).
#[derive(Debug, Clone, Copy)]
pub struct Need<'a> {
    pub embedder_id: &'a str,
    pub vector: &'a [f32],
    /// The space's MEASURED unrelated-cosine null `(mean, std)`, when known.
    pub unrelated_null: Option<(f32, f32)>,
}

/// One gene she could run this turn, with its signature when one was minted.
#[derive(Debug, Clone)]
pub struct Candidate<'a> {
    pub gene: ActiveAdapterRequest,
    pub signature: Option<&'a GeneSignature>,
}

/// How a considered gene was routed this turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Route {
    /// Compared by distance: its similarity and the region edges it was held to.
    Distance { similarity: f32, enter: f32, leave: f32, inside: bool },
    /// No usable signature in the need's space: applied at its trained scale.
    Unrouted,
    /// Signed, but the turn had no need to compare: kept as it was.
    Kept,
}

/// One row of "why was this gene in her head" (the `genome.select` probe).
#[derive(Debug, Clone, PartialEq)]
pub struct Considered {
    pub name: String,
    pub path: String,
    pub route: Route,
    /// The scale it runs at this turn; 0 = off.
    pub scale: f64,
}

/// What [`select`] decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// The genes this turn runs with and their scales, heaviest first.
    pub genes: Vec<ActiveAdapterRequest>,
    pub considered: Vec<Considered>,
    /// The applied set or a scale differs from the previous turn's: the engine clears her
    /// slot's KV and she re-prefills her context.
    pub changed: bool,
}

/// PURE: the region edges of a signed gene in the need's space, or `None` when it cannot
/// be routed (no region and no measured null).
fn edges(signature: &GeneSignature, need: &Need<'_>) -> Option<(f32, f32)> {
    let null_edge = need.unrelated_null.map(|(mean, std)| mean + NULL_SIGMAS * std);
    let enter = match (signature.region, null_edge) {
        (Some(region), Some(null)) => region.max(null),
        (Some(region), None) => region,
        (None, Some(null)) => null,
        (None, None) => return None,
    };
    Some((enter, 1.0 - LEAVE_DISTANCE_RATIO * (1.0 - enter)))
}

/// PURE: the genes a turn runs with, given her candidates, the turn's need, and what she
/// ran last turn (`previous`, with scales). See the module docs for the rule.
pub fn select(need: Option<&Need<'_>>, candidates: &[Candidate<'_>], previous: &[ActiveAdapterRequest]) -> Selection {
    let was: BTreeMap<&str, f64> = previous
        .iter()
        .filter(|g| g.scale != 0.0)
        .map(|g| (g.path.as_str(), g.scale))
        .collect();
    let routes: Vec<Route> = candidates.iter().map(|c| route(c, need, was.contains_key(c.gene.path.as_str()))).collect();

    // The routed genes inside their regions, with their depth inside each.
    let margins: Vec<(usize, f32)> = routes
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match *r {
            Route::Distance { similarity, leave, inside: true, .. } => {
                Some((i, ((similarity - leave) / (1.0 - leave)).max(f32::EPSILON)))
            }
            _ => None,
        })
        .collect();
    let routed_was = routes
        .iter()
        .zip(candidates)
        .filter(|(r, c)| matches!(r, Route::Distance { .. }) && was.contains_key(c.gene.path.as_str()))
        .count();
    let held: Option<Vec<f64>> = margins
        .iter()
        .map(|(i, _)| was.get(candidates[*i].gene.path.as_str()).copied())
        .collect::<Option<Vec<f64>>>()
        .filter(|w| w.len() == routed_was);
    let total: f32 = margins.iter().map(|(_, m)| m).sum();
    let mut scales = vec![0.0f64; candidates.len()];
    for (k, &(i, m)) in margins.iter().enumerate() {
        scales[i] = match &held {
            Some(w) => w[k],
            None => f64::from(m / total),
        };
    }
    for (i, r) in routes.iter().enumerate() {
        match r {
            Route::Unrouted => scales[i] = 1.0,
            Route::Kept => {
                if let Some(&w) = was.get(candidates[i].gene.path.as_str()) {
                    scales[i] = w;
                }
            }
            Route::Distance { .. } => {}
        }
    }

    let mut on: Vec<usize> = (0..candidates.len()).filter(|&i| scales[i] != 0.0).collect();
    on.sort_by(|&a, &b| scales[b].total_cmp(&scales[a]).then_with(|| candidates[a].gene.path.cmp(&candidates[b].gene.path)));
    let genes: Vec<ActiveAdapterRequest> = on
        .into_iter()
        .map(|i| ActiveAdapterRequest { scale: scales[i], ..candidates[i].gene.clone() })
        .collect();
    let now: BTreeMap<&str, f64> = genes.iter().map(|g| (g.path.as_str(), g.scale)).collect();
    let changed = now != was;
    let considered = candidates
        .iter()
        .zip(routes)
        .zip(scales)
        .map(|((c, route), scale)| Considered { name: c.gene.name.clone(), path: c.gene.path.clone(), route, scale })
        .collect();
    Selection { changed, genes, considered }
}

/// PURE: how one candidate is routed this turn (`was_on`: it ran last turn).
fn route(candidate: &Candidate<'_>, need: Option<&Need<'_>>, was_on: bool) -> Route {
    let Some(signature) = candidate.signature else {
        return Route::Unrouted;
    };
    let Some(need) = need else {
        return Route::Kept;
    };
    let (Some(similarity), Some((enter, leave))) =
        (signature.similarity_in(need.embedder_id, need.vector), edges(signature, need))
    else {
        return Route::Unrouted;
    };
    let inside = similarity >= enter || (was_on && similarity >= leave);
    Route::Distance { similarity, enter, leave, inside }
}

/// What each mind ran last turn: the hysteresis memory and the held weights. Per process:
/// after a restart the first turn recomputes, which the engine's restored slot sees as a
/// change only if the recomputed weights differ.
fn last_applied() -> &'static Mutex<HashMap<Uuid, Vec<ActiveAdapterRequest>>> {
    static LAST: std::sync::OnceLock<Mutex<HashMap<Uuid, Vec<ActiveAdapterRequest>>>> = std::sync::OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The genes her next turn runs with, chosen by distance from `need_text` (what her mind is
/// on: the turn's focused line). `None` when her genome cannot be read (no serving base, an
/// unreadable trial file): the caller then keeps the genome she has, and the reason is on
/// the probe stream.
pub async fn live_genes(persona: Uuid, need_text: &str) -> Option<Vec<ActiveAdapterRequest>> {
    let base = crate::inference::llama_server::current_serving().active_model?;
    let store = crate::genome::gene_trial::GeneTrials::default_store()?;
    let trials = match store.load() {
        Ok(trials) => trials,
        Err(error) => {
            continuum_base::probe!(
                class = "genome.trial.unreadable",
                persona = %persona,
                error = error.as_str(),
                "the gene trial file did not read: this turn keeps the genome she has"
            );
            return None;
        }
    };
    let genes = crate::genome::gene_trial::genes_for_turn(&trials, persona, &base);
    if genes.is_empty() {
        last_applied().lock().remove(&persona);
        return Some(genes);
    }
    let signatures = match crate::genome::signature::signature_store_path().and_then(|p| SignatureStore::load_at(&p)) {
        Ok(store) => store,
        Err(error) => {
            continuum_base::probe!(
                class = "genome.select.signatures_unreadable",
                persona = %persona,
                error = error.as_str(),
                "the signature sidecar did not read: her genes run unrouted this turn"
            );
            SignatureStore::default()
        }
    };
    let candidates: Vec<Candidate<'_>> = genes
        .into_iter()
        .map(|gene| {
            let signature = signatures.by_path.get(&gene.path);
            Candidate { gene, signature }
        })
        .collect();
    let embedded = if candidates.iter().any(|c| c.signature.is_some()) && !need_text.trim().is_empty() {
        let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
        Some((embedder.id().to_string(), embedder.embed(need_text).await, embedder.unrelated_null()))
    } else {
        None
    };
    let need = embedded.as_ref().map(|(id, vector, null)| Need {
        embedder_id: id,
        vector,
        unrelated_null: *null,
    });
    let previous = last_applied().lock().get(&persona).cloned().unwrap_or_default(); // unwrap_or_default: a mind with no turn yet this process ran no genes
    let selection = select(need.as_ref(), &candidates, &previous);
    probe_selection(persona, &selection);
    last_applied().lock().insert(persona, selection.genes.clone());
    Some(selection.genes)
}

/// `genome.select`: every gene considered, its similarity and edges, the scale it runs at;
/// and, when the set changed, what the change costs her.
fn probe_selection(persona: Uuid, selection: &Selection) {
    let rows: Vec<String> = selection
        .considered
        .iter()
        .map(|c| match c.route {
            Route::Distance { similarity, enter, leave, .. } => {
                format!("{} sim={similarity:.3} enter={enter:.3} leave={leave:.3} scale={:.3}", c.name, c.scale)
            }
            Route::Unrouted => format!("{} unrouted scale={:.3}", c.name, c.scale),
            Route::Kept => format!("{} kept scale={:.3}", c.name, c.scale),
        })
        .collect();
    continuum_base::probe!(
        class = "genome.select",
        persona = %persona,
        considered = selection.considered.len() as u64,
        applied = selection.genes.len() as u64,
        changed = selection.changed,
        genes = %rows.join("; "),
        "the genes this turn runs with, by distance from what her mind is on"
    );
    if selection.changed {
        continuum_base::probe!(
            class = "genome.select.set_changed",
            persona = %persona,
            applied = selection.genes.len() as u64,
            "her gene set or a scale changed: the engine clears her slot's KV and she re-prefills her context, and her slot batches only with minds running the same set"
        );
    }
}

#[cfg(test)]
mod tests;
