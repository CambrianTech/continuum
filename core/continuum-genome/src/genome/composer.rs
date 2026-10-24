//! Her PHENOTYPE for a turn: which of her genes shape it, and at what scale
//! (GENE-REUSE-FORK-MINT §3d, the APPLIED tier). Every gene whose region the turn's perception
//! is inside, several at once, weighted by how deep inside each one is, the way a mixture of
//! experts routes. Joel, 2026-10-24: "Goal was many with weights."
//!
//! The one composer. The turn sites ask [`compose`]; the pure core is [`select`]; an examiner
//! compares `P` with [`Phenotype::without`] one gene (that gene at 0, every other unchanged).
//!
//! # The rule
//!
//! - **Inside.** A gene's floor `f` is its own spread
//!   ([`GeneSignature::region`](crate::genome::signature::GeneSignature): the 10th-percentile
//!   similarity of its own corpus, filed at mint), never below the space's measured unrelated
//!   null + 2σ (a diffuse gene must not ride every turn). A gene with neither, or signed in
//!   another embedding space, cannot be routed by distance: it runs at its trained scale as every
//!   gene did before routing existed (a gene never leaves her head by silence), named unrouted.
//! - **Weight.** `a = clamp((sim - f) / (1 - f), 0, 1)`: 0 at the floor, 1 at the center, so a
//!   broad gene and a narrow one compare on the same footing. The `capacity` heaviest are kept
//!   (the genes the engine holds on the device; the working-set owner says how many), and each
//!   runs at `a / max(1, Σa)`: one gene alone runs at its depth, never above its trained 1.0, and
//!   the deltas of several never sum past one trained adapter's magnitude.
//! - **Hysteresis.** A gene enters at its floor (distance `r = 1 - f`) and stays until the
//!   distance passes `1.5 r` (§3d), so a topic at a region's edge does not switch it every turn.
//! - **Held while the set holds.** The engine clears a slot's KV whenever its adapter scales
//!   change at all (fork `are_lora_equal` compares scales exactly; server-context
//!   `lora_should_clear_cache`), so re-weighting each turn would re-prefill her whole context
//!   every turn. Scales are computed when the routed SET changes (which costs that re-prefill
//!   anyway) and held bit-identical until it changes again. A gene kept only by hysteresis has
//!   `a = 0`, so the next recompute drops it.
//! - **No perception, no change.** Without a perception embedding, routed genes stay as they were.

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

/// The turn's perception, embedded once by the caller (the composer stays pure and sync).
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
    /// Compared by distance: its similarity, the edges it was held to, its depth `a`.
    Distance { similarity: f32, floor: f32, leave: f32, weight: f32, inside: bool },
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

/// A phenotype: the genes a turn runs with and their engine scales, heaviest first. Empty = the
/// base alone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Phenotype {
    pub genes: Vec<ActiveAdapterRequest>,
}

impl Phenotype {
    /// `P ∖ g`: the same phenotype with gene `path` off and every other gene at its scale in `P`
    /// (the examiner's comparison: `g` at its weight vs at 0, nothing else moved).
    pub fn without(&self, path: &str) -> Phenotype {
        Phenotype { genes: self.genes.iter().filter(|g| g.path != path).cloned().collect() }
    }

    /// The scale `path` runs at in this phenotype; 0 when it is not in it.
    pub fn scale_of(&self, path: &str) -> f64 {
        self.genes.iter().find(|g| g.path == path).map_or(0.0, |g| g.scale)
    }
}

/// What [`select`] decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub phenotype: Phenotype,
    pub considered: Vec<Considered>,
    /// The applied set or a scale differs from the previous turn's: the engine clears her
    /// slot's KV and she re-prefills her context.
    pub changed: bool,
}

/// PURE: the floor (enter edge) and leave edge of a signed gene in the perception's space, or
/// `None` when it cannot be routed (no region and no measured null).
fn edges(signature: &GeneSignature, need: &Need<'_>) -> Option<(f32, f32)> {
    let null_edge = need.unrelated_null.map(|(mean, std)| mean + NULL_SIGMAS * std);
    let floor = match (signature.region, null_edge) {
        (Some(region), Some(null)) => region.max(null),
        (Some(region), None) => region,
        (None, Some(null)) => null,
        (None, None) => return None,
    };
    Some((floor, 1.0 - LEAVE_DISTANCE_RATIO * (1.0 - floor)))
}

/// PURE: the phenotype for a turn, given her candidate genes, the turn's perception, how many
/// genes the engine holds (`capacity`), and what she ran last turn (`previous`, with scales).
/// See the module docs for the rule.
pub fn select(
    need: Option<&Need<'_>>,
    candidates: &[Candidate<'_>],
    capacity: usize,
    previous: &[ActiveAdapterRequest],
) -> Selection {
    let was: BTreeMap<&str, f64> = previous
        .iter()
        .filter(|g| g.scale != 0.0)
        .map(|g| (g.path.as_str(), g.scale))
        .collect();
    let routes: Vec<Route> = candidates.iter().map(|c| route(c, need, was.contains_key(c.gene.path.as_str()))).collect();

    // The routed genes in the set: inside (or held by hysteresis), heaviest `capacity` of them.
    let mut kept: Vec<(usize, f32)> = routes
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match *r {
            Route::Distance { weight, inside: true, .. } => Some((i, weight)),
            _ => None,
        })
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| candidates[a.0].gene.path.cmp(&candidates[b.0].gene.path)));
    kept.truncate(capacity);
    let routed_was = routes
        .iter()
        .zip(candidates)
        .filter(|(r, c)| matches!(r, Route::Distance { .. }) && was.contains_key(c.gene.path.as_str()))
        .count();
    let held: Option<Vec<f64>> = kept
        .iter()
        .map(|(i, _)| was.get(candidates[*i].gene.path.as_str()).copied())
        .collect::<Option<Vec<f64>>>()
        .filter(|w| w.len() == routed_was);
    let total: f32 = kept.iter().map(|(_, a)| a).sum();
    let mut scales = vec![0.0f64; candidates.len()];
    for (k, &(i, a)) in kept.iter().enumerate() {
        scales[i] = match &held {
            Some(w) => w[k],
            None => f64::from(a / total.max(1.0)),
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
    Selection { changed, phenotype: Phenotype { genes }, considered }
}

/// PURE: how one candidate is routed this turn (`was_on`: it ran last turn).
fn route(candidate: &Candidate<'_>, need: Option<&Need<'_>>, was_on: bool) -> Route {
    let Some(signature) = candidate.signature else {
        return Route::Unrouted;
    };
    let Some(need) = need else {
        return Route::Kept;
    };
    let (Some(similarity), Some((floor, leave))) =
        (signature.similarity_in(need.embedder_id, need.vector), edges(signature, need))
    else {
        return Route::Unrouted;
    };
    let weight = ((similarity - floor) / (1.0 - floor)).clamp(0.0, 1.0);
    let inside = weight > 0.0 || (was_on && similarity >= leave);
    Route::Distance { similarity, floor, leave, weight, inside }
}

/// What each mind ran last turn: the hysteresis memory and the held weights. Per process:
/// after a restart the first turn recomputes, which the engine's restored slot sees as a
/// change only if the recomputed weights differ.
fn last_applied() -> &'static Mutex<HashMap<Uuid, Vec<ActiveAdapterRequest>>> {
    static LAST: std::sync::OnceLock<Mutex<HashMap<Uuid, Vec<ActiveAdapterRequest>>>> = std::sync::OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Her phenotype for the next turn, composed from `perception` (what her mind is on: the turn's
/// focused line). `None` when her genome cannot be read (no serving base, an unreadable trial
/// file): the caller then keeps the genome she has, and the reason is on the probe stream.
pub async fn compose(persona: Uuid, perception: &str) -> Option<Phenotype> {
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
    let genes = crate::genome::gene_trial::her_genes(&trials, persona, &base);
    if genes.is_empty() {
        last_applied().lock().remove(&persona);
        return Some(Phenotype::default());
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
    let embedded = if candidates.iter().any(|c| c.signature.is_some()) && !perception.trim().is_empty() {
        let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
        Some((embedder.id().to_string(), embedder.embed(perception).await, embedder.unrelated_null()))
    } else {
        None
    };
    let need = embedded.as_ref().map(|(id, vector, null)| Need {
        embedder_id: id,
        vector,
        unrelated_null: *null,
    });
    let previous = last_applied().lock().get(&persona).cloned().unwrap_or_default(); // unwrap_or_default: a mind with no turn yet this process ran no genes
    // Capacity: every gene she has, until the gene working set reports how many the engine holds
    // on the device (genome::working_set, the LOADED tier).
    let selection = select(need.as_ref(), &candidates, candidates.len(), &previous);
    probe_selection(persona, &selection);
    last_applied().lock().insert(persona, selection.phenotype.genes.clone());
    Some(selection.phenotype)
}

/// `genome.select`: every gene considered, its similarity, floor, leave edge, depth and the
/// scale it runs at; and, when the set changed, what the change costs her.
fn probe_selection(persona: Uuid, selection: &Selection) {
    let rows: Vec<String> = selection
        .considered
        .iter()
        .map(|c| match c.route {
            Route::Distance { similarity, floor, leave, weight, .. } => format!(
                "{} sim={similarity:.3} floor={floor:.3} leave={leave:.3} a={weight:.3} scale={:.3}",
                c.name, c.scale
            ),
            Route::Unrouted => format!("{} unrouted scale={:.3}", c.name, c.scale),
            Route::Kept => format!("{} kept scale={:.3}", c.name, c.scale),
        })
        .collect();
    continuum_base::probe!(
        class = "genome.select",
        persona = %persona,
        considered = selection.considered.len() as u64,
        applied = selection.phenotype.genes.len() as u64,
        changed = selection.changed,
        genes = %rows.join("; "),
        "the genes this turn runs with, by distance from what her mind is on"
    );
    if selection.changed {
        continuum_base::probe!(
            class = "genome.select.set_changed",
            persona = %persona,
            applied = selection.phenotype.genes.len() as u64,
            "her gene set or a scale changed: the engine clears her slot's KV and she re-prefills her context, and her slot batches only with minds running the same set"
        );
    }
}

/// The `persona.genome.turn_genes` rendering of a phenotype: each gene and its scale.
pub fn render(phenotype: &Phenotype) -> String {
    phenotype
        .genes
        .iter()
        .map(|g| format!("{}={:.3}", g.name, g.scale))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;
