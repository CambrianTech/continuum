//! What her dream holds between passes: hers alone, owned by her dream part, so no other
//! persona's dream reads or locks it.

use super::*;

/// Her dream's state across passes. In memory: a restart dreams over what she lives from then on,
/// and her store's content-hash dedup drops what she already knows (durable dream state is card
/// d56dc734 step 3).
#[derive(Default)]
pub(super) struct Her {
    /// Where her lived experience starts for her dream: her store's admission index when her dream
    /// first read it. Everything she lives from there is her dream's material, none of it cut off
    /// by a window; what came before (a restart's rehydrated store) was dreamed by the life before.
    pub(super) lived_from: Option<usize>,
    /// Episodics already folded into a fact: the clustering input's dedup, so a pass never spends
    /// inference again on what it consolidated.
    pub(super) consolidated: HashSet<Uuid>,
    /// Semantic beliefs the review has already shown the distiller.
    pub(super) reviewed: HashSet<Uuid>,
    /// Accepted reviews waiting on a pass, by their exact acceptance ids.
    pub(super) boundaries: ReviewBoundaries,
    /// The newest shared-lesson timestamp already consolidated (`memory/consolidate`'s
    /// `since_timestamp`), so a received lesson is never trained twice.
    pub(super) watermark: Option<String>,
}

impl Her {
    /// Queue an exact acceptance (the bound submission id) for her next pass. A processed one is
    /// never asked for again: her credit store records it durably and the asker reads that first.
    pub(super) fn queue_boundary(&mut self, persona: Uuid, acceptance: Uuid) -> bool {
        let queued = self.boundaries.pending.insert(acceptance);
        if queued {
            continuum_base::probe!(class = "dream.review_boundary_pending", persona = %persona,
                acceptance = %acceptance, "accepted review queued for her dream's next pass");
        }
        queued
    }

    /// Start reading her store at what she lives from now on: once, at her dream's first pass.
    pub(super) fn begin(&mut self, admission: &AdmissionState) {
        self.lived_from.get_or_insert_with(|| admission.engram_count());
    }

    /// Her episodics lived since her dream started that are not yet folded into a fact.
    pub(super) fn fresh(&self, admission: &AdmissionState) -> Vec<Engram> {
        let Some(from) = self.lived_from else { return Vec::new() }; // her dream has not read her store yet
        admission.episodics_from(from).into_iter().filter(|e| !self.consolidated.contains(&e.id)).collect()
    }

    /// Record a cluster's episodics as consolidated.
    pub(super) fn mark_consolidated(&mut self, cluster: &[Engram]) {
        self.consolidated.extend(cluster.iter().map(|e| e.id));
    }
}
