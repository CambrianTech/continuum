//! The dream, review and supersession passes (free functions over a reflector).

use super::*;

pub(super) async fn finish_review_boundaries(
    her: &mut Her,
    persona: Uuid,
    reviews: &[Uuid],
    eligible_clusters: usize,
    store: Option<&Arc<dyn ReviewBoundaryLedger>>,
) {
    if let Some(store) = store {
        if let error @ (Ok(Err(_)) | Err(_)) = tokio::time::timeout(
            REVIEW_COMPLETION_IO_BUDGET,
            store.finish(reviews, eligible_clusters),
        )
        .await
        {
            continuum_base::probe!(class = "dream.review_completion_deferred", persona = %persona, error = ?error,
                "processed boundary remains pending until its durable receipt is acknowledged");
            return;
        }
    }
    for review in reviews {
        if her.boundaries.pending.remove(review) {
            continuum_base::probe!(class = "dream.review_boundary_processed", persona = %persona,
                acceptance = %review, eligible_clusters,
                "eligible episode clusters processed; not a training or adoption receipt");
        }
    }
}

/// The inference-heavy dream pass, awaited by her dream part (one pass of hers at a time: the part
/// takes her next input only after it). Returns whether every selected episode cluster was
/// admitted or deduplicated. The historian and belief-review tails are optional follow-on work;
/// their failures do not undo admitted cluster facts or claim a training result.
pub(super) async fn dream_pass(
    reflector: &PersonaReflector,
    persona_id: Uuid,
    clusters: &[Vec<Engram>],
    fresh: &[Engram],
    her: &mut Her,
) -> bool {
    // #175 budget-at-assembly: the observation budget comes from the window serving reports
    // (`distiller_for`), so a cluster is composed WITHIN her slot and never overflows it into a
    // 500 "Compute error" and a poisoned lane ([[budget-at-assembly-never-clamp-the-prompt]]).
    let distiller = distiller_for(reflector);
    let mut published = 0usize;
    let mut complete = true;
    let mut historian_admitted = false;
    for cluster in clusters {
        // #221 slice 2 — SUPERSESSION REVIEW: alongside the cluster, show
        // the distiller the persona's most related PRIOR beliefs (lexical
        // recall-key retrieval — mechanics; the model judges). Its verdict
        // rides the same single generation, so supersession costs zero
        // extra inference.
        let mut prior_beliefs = reflector.admission.semantic_beliefs_matching(&SemanticDistiller::union_recall_keys(cluster));
        // ROTATING WINDOW (#221 slice 2b): lexical overlap can't reach
        // beliefs that share no tokens with new experience (the stale
        // Rust-era beliefs vs python lessons, glass-boxed live). Each pass
        // therefore ALSO re-examines a few of her oldest not-yet-reviewed
        // beliefs — eventual coverage of the whole belief store, a few
        // beliefs per dream, marked reviewed regardless of verdict.
        {
            let mut already = her.reviewed.clone();
            already.extend(prior_beliefs.iter().map(|e| e.id));
            let rotating = reflector.admission.semantic_beliefs_oldest_excluding(
                &already,
                continuum_base::time::EpochMs::now().millis().saturating_sub(REVIEW_MIN_AGE_MS),
                ROTATING_REVIEW_PER_PASS,
            );
            her.reviewed.extend(rotating.iter().map(|b| b.id));
            prior_beliefs.extend(rotating);
        }
        // Distill the cluster into one durable fact. Fail LOUD per cluster:
        // a distillation error is logged and the cluster's episodics stay
        // un-consolidated (so a future dream retries them), never silently
        // swallowed (`[[fallbacks-are-illegal-fail-loud]]`).
        let fact = match distiller
            .distill_reviewing(LENS_CONSOLIDATOR, Some(persona_id), cluster, &prior_beliefs)
            .await
        {
            Ok(fact) => fact,
            Err(err) => {
                complete = false;
                tracing::warn!(
                    persona = %persona_id,
                    error = %err,
                    "dream: distillation failed; leaving cluster for a future dream"
                );
                continue;
            }
        };

        match reflector.admission.admit_reflection(semantic_engram(&fact)) {
            Ok(AdmissionDecision::Admit { .. }) => {
                published += 1;
                her.mark_consolidated(cluster);
                // Apply the model's supersession verdict (#221 slice 2):
                // the replaced beliefs drop to the salience floor NOW —
                // the new fact out-ranks them in recall immediately, and
                // the decay drain owns them from here. Applied ONLY on a
                // successful admit: if the new fact didn't land, the old
                // beliefs keep their standing (never orphan her knowledge).
                apply_supersessions(reflector, persona_id, &fact.supersedes);
            }
            Ok(AdmissionDecision::Drop { .. }) => {
                // Content-hash dedup already has this fact (e.g. a
                // post-restart re-distillation). Mark the sources
                // consolidated so we stop re-spending inference on them.
                her.mark_consolidated(cluster);
            }
            Ok(AdmissionDecision::Quarantine { .. }) => {
                complete = false;
                // Self-produced facts are SelfTrust and do not route through
                // the quarantine gate; reaching here is a contract change in
                // `admit_reflection`. Surface it rather than hide it.
                tracing::warn!(
                    persona = %persona_id,
                    "dream: self-reflection unexpectedly quarantined"
                );
            }
            Err(err) => {
                complete = false;
                tracing::warn!(
                    persona = %persona_id,
                    error = %err,
                    "dream: admit_reflection failed"
                );
            }
        }
    }

    // The wander pass (#145 outlier A): when the dream actually digested
    // something, the historian takes ONE look across the same fresh window
    // and leaves ONE provenance-tagged thought about the pattern in her own
    // recent history. Gated on `published > 0` so it fires at most once per
    // dreaming tick and never on already-consolidated material (the next
    // tick finds nothing fresh and sleeps) — bounded interiority, not a
    // second automaton.
    if published > 0 {
        match distiller
            .distill_with(LENS_HISTORIAN, Some(persona_id), fresh)
            .await
        {
            Ok(thought) => {
                match reflector
                    .admission
                    .admit_reflection(thought_engram(&thought, LENS_HISTORIAN))
                {
                    Ok(AdmissionDecision::Admit { .. }) => {
                        published += 1;
                        historian_admitted = true;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(
                            persona = %persona_id,
                            error = %err,
                            "wander: admit_reflection failed for historian thought"
                        );
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    persona = %persona_id,
                    error = %err,
                    "wander: historian distillation failed; no thought this dream"
                );
            }
        }
    }

    // BUSY-DREAM review tail (#221 slice 2c'): drain one belief batch per dream
    // even when fresh material kept the dream busy — an active persona never
    // has a quiet day, and the backlog must not wait for one.
    let backlog = reflector.admission.semantic_beliefs_oldest_excluding(
        &her.reviewed,
        continuum_base::time::EpochMs::now().millis().saturating_sub(REVIEW_MIN_AGE_MS),
        REVIEW_ONLY_BATCH,
    );
    if !backlog.is_empty() {
        review_batch(reflector, persona_id, backlog, &mut her.reviewed).await; // an unjudged tail stays for the next pass
    }

    tracing::info!(
        persona = %persona_id,
        published,
        clusters = clusters.len(),
        clusters_processed = complete,
        historian_admitted,
        probe_class = "persona.dream.pass_complete",
        "dream: pass ended; admitted counts and required-cluster outcome recorded"
    );
    complete
}

/// Build the pass distiller with its observation budget derived from the LIVE
/// served window (shared by the consolidation pass and the review-only pass —
/// one budget rule, [[budget-at-assembly-never-clamp-the-prompt]]).
fn distiller_for(reflector: &PersonaReflector) -> SemanticDistiller {
    let live = crate::inference::llama_server::current_serving();
    // The window serving reports for the turn, or none: a lane that exists but is not ready has
    // already deferred the pass (`consolidate.rs`), so an unknown window here is no local lane, whose
    // adapter holds its own window.
    let obs_budget_chars = (live.ready && live.served_context_window > 0).then(|| {
        let window = live.served_context_window;
        let completion_reserve = window / 4;
        (window.saturating_sub(completion_reserve) as usize).saturating_mul(4)
    });
    // The dream asks for the model THE LANE IS SERVING RIGHT NOW — never the
    // persona's configured base id, which goes stale the moment serving
    // re-homes onto a genome-adapter alias. Glass-boxed live 2026-07-22: every
    // distillation erroring "model 'unsloth/Devstral-…' is not the active
    // served model" while a healthy lane served Asha's adapter stack — hours of
    // silent dream failure from one stale binding. Same doctrine as the served
    // context window: the live snapshot is the ONE authority; the configured
    // binding is only the not-ready fallback.
    let model = if live.ready && live.active_model.is_some() {
        live.active_model
    } else {
        reflector.model.clone()
    };
    SemanticDistiller::new(reflector.adapter.clone(), obs_budget_chars).with_model(model)
}

/// Apply the model's supersession verdict: demote each replaced belief to the
/// salience floor (shared by both dream shapes — one consequence rule).
fn apply_supersessions(reflector: &PersonaReflector, persona_id: Uuid, superseded: &[Uuid]) {
    if superseded.is_empty() {
        return;
    }
    let now = continuum_base::time::EpochMs::now().millis();
    for id in superseded {
        reflector
            .admission
            .recall_metadata()
            .demote_to_floor(*id, now);
    }
    continuum_base::probe!(
        class = "hippocampus.supersede",
        persona = %persona_id,
        superseded = superseded.len(),
        "dream: stale beliefs demoted to the salience floor — \
         replaced by a newly consolidated fact (the plastic mind)"
    );
}

/// The belief review, callable from BOTH dream shapes: the review-only pass (#221 slice 2c), which
/// re-examines a batch of her oldest beliefs with no fresh material, and the tail of every busy
/// dream. The distiller sees the beliefs as both material and numbered supersession candidates; its
/// verdict demotes what no longer holds; the restatement admits through the normal dedup gate. A
/// judged batch is marked reviewed so the queue drains; a failed distillation leaves it unreviewed.
/// Returns whether the batch was judged.
/// The busy-dream tail exists because an active persona always has fresh material, so a quiet day
/// never comes for her (glass-boxed 2026-07-22: Atlas chatted all evening and the review-only branch
/// never once fired).
pub(super) async fn review_batch(
    reflector: &PersonaReflector,
    persona_id: Uuid,
    beliefs: Vec<Engram>,
    reviewed: &mut HashSet<Uuid>,
) -> bool {
    let distiller = distiller_for(reflector);
    let mut superseded_n = 0usize;
    match distiller
        .distill_reviewing(LENS_REVIEWER, Some(persona_id), &beliefs, &beliefs)
        .await
    {
        Ok(fact) => {
            match reflector.admission.admit_reflection(semantic_engram(&fact)) {
                Ok(AdmissionDecision::Admit { .. }) | Ok(AdmissionDecision::Drop { .. }) => {
                    // Admit = a refreshed restatement joins her knowledge;
                    // Drop = she already knows it (dedup). Either way the
                    // surviving belief exists, so the verdict is safe to apply.
                    superseded_n = fact.supersedes.len();
                    apply_supersessions(reflector, persona_id, &fact.supersedes);
                }
                Ok(AdmissionDecision::Quarantine { .. }) => {
                    tracing::warn!(
                        persona = %persona_id,
                        "review pass: self-reflection unexpectedly quarantined"
                    );
                }
                Err(err) => {
                    tracing::warn!(persona = %persona_id, error = %err, "review pass: admit failed");
                }
            }
        }
        Err(err) => {
            // INFRA never consumes the queue (the same INFRA≠FAIL doctrine,
            // applied inside her own mind): a failed distillation is a broken
            // lane, not a rendered judgment. The batch stays UNREVIEWED and
            // gets its day in court when inference is healthy. Glass-boxed
            // 2026-07-23: a thrashing lane produced 814 distill errors and the
            // old mark-regardless choice incinerated 200+ review batches
            // without judgment — the queue burned against a dead lane. Spin is
            // bounded: an erroring lane fails fast, one cheap call per dream.
            tracing::warn!(
                persona = %persona_id,
                error = %err,
                "review pass: distillation failed — batch left UNREVIEWED for a healthy dream"
            );
            return false;
        }
    }
    reviewed.extend(beliefs.iter().map(|b| b.id));
    tracing::info!(
        persona = %persona_id,
        reviewed = beliefs.len(),
        superseded = superseded_n,
        probe_class = "persona.dream.review_pass",
        "dream: quiet-day belief review complete"
    );
    true
}

