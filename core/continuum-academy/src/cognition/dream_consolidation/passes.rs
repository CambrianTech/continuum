//! The dream, review and supersession passes (free functions over a reflector).

use super::*;

pub(super) async fn finish_review_boundaries(
    all: &Mutex<HashMap<Uuid, ReviewBoundaries>>,
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
    let mut all = all.lock().unwrap(); // JUSTIFIED: completion only updates the owner receipt cache synchronously.
    if let Some(receipts) = all.get_mut(&persona) {
        for review in reviews {
            if receipts.pending.remove(review) {
                if receipts.completed.len() == REVIEW_BOUNDARY_CAPACITY {
                    receipts.completed.pop_front();
                }
                receipts.completed.push_back(*review);
                continuum_base::probe!(class = "dream.review_boundary_processed", persona = %persona,
                    acceptance = %review, eligible_clusters,
                    "eligible episode clusters processed; not a training or adoption receipt");
            }
        }
    }
}

/// The inference-heavy dream pass — runs on ITS OWN tokio task, never inside
/// the governor's timeout-isolated tick (see [`DreamConsolidationRegion::consolidate`]).
/// Returns whether every selected episode cluster was admitted or deduplicated.
/// The existing historian and belief-review tails are optional follow-on work;
/// their failures do not undo admitted cluster facts or claim a training result.
pub(super) async fn dream_pass(
    reflector: PersonaReflector,
    persona_id: Uuid,
    clusters: Vec<Vec<Engram>>,
    fresh: Vec<Engram>,
    consolidated: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    reviewed: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
) -> bool {
    // CAUSAL EARLY-SKIP (restore-economy 1.a, the subscribe() consumer): while a
    // measured solve holds the core, don't even ASSEMBLE the dream — cluster
    // gathering, budget math and prompt composition are wasted work for a
    // generation the adapter seam would only park at the door. Await the RELEASE
    // EVENT instead (watch semantics: a transition cannot be missed), holding
    // nothing while parked — the mind isn't locked, its idle-time work is
    // simply queued behind measured work, which is what "idle-time" means
    // ([[idle-is-self-directed-free-time]]). The adapter gate stays the
    // enforcement (this is an optimization, not the guard); its 4h leak ceiling
    // still bounds the pathological case for any dream that slips past here.
    // #2561: superseded by the ACTIVITY GATE — boredom as substrate. The hold
    // is one input of the gate (hold-active ⇒ Active), so waiting for boredom
    // subsumes the old hold-park AND adds what it never had: in-flight turns and
    // directed-conversation linger. The dream state does not ARISE while the
    // organism is engaged; nothing is minted, nothing queues, nothing holds.

    // #175 budget-at-assembly: derive the observation budget from the LIVE served
    // per-slot window (the single source of truth, same the deliberation clamp reads)
    // so a dream cluster is composed WITHIN the slot and can never overflow it →
    // 500 "Compute error" → poisoned lane. tokens → chars (~4/token) minus a reserve
    // for the lens system prompt + the distilled-fact reply. Unknown/not-ready window
    // (mid-relaunch) → the conservative default. [[budget-at-assembly-never-clamp-the-prompt]]
    // Observation budget DERIVED from the live served window — never a hardcoded cap
    // (a magic constant that crushes a real context window is the "120k clamped to 3k →
    // all models suck" anti-pattern, Joel 2026-07-17 [[no-hardcoded-context-numbers-derive-from-the-live-window]]).
    // The prompt (system + observations) must leave room for the reply so
    // prompt+reply ≤ n_ctx (no-context-shift). The reserve comes from `ContextBudget`'s own
    // `latest_action_chars`, which is where a window-relative CHARS bound belongs.
    //
    // CORRECTED 2026-08-20 — this comment claimed to "reserve the SAME completion fraction
    // the deliberation path uses — `completion_budget_for` = window/4". That was false in
    // three ways and had been for as long as it was written: it named a function this code
    // never called, and the deliberation reserve is neither `/4` nor a bare share any more
    // (it yields to a mandatory prompt floor, and `completion_budget_for` is deleted). The
    // claim SURVIVED only because both fractions happened to be a quarter, so the prose
    // agreed with the arithmetic by coincidence rather than by construction.
    //
    // Deliberately NOT "fixed" by wiring this to the deliberation reserve. These are
    // different budgets for different prompts — a distiller's reply is not a citizen's turn
    // — and inventing a dependency to make a sentence true would couple two things that
    // have no reason to move together. The honest fix is prose that describes what the code
    // does.
    //
    // The rest (×4 chars/token) goes to observations. When the window is
    // unknown (mid-relaunch) fall back to the substrate floor MIN_SERVE_CTX (an established
    // constant, not an invented one), which the next ready tick supersedes.
    let distiller = distiller_for(&reflector);
    let mut published = 0usize;
    let mut complete = true;
    let mut historian_admitted = false;
    for cluster in &clusters {
        // #221 slice 2 — SUPERSESSION REVIEW: alongside the cluster, show
        // the distiller the persona's most related PRIOR beliefs (lexical
        // recall-key retrieval — mechanics; the model judges). Its verdict
        // rides the same single generation, so supersession costs zero
        // extra inference.
        let mut prior_beliefs = reflector.admission.semantic_beliefs_matching(
            &SemanticDistiller::union_recall_keys(cluster),
            SUPERSESSION_REVIEW_LIMIT,
        );
        // ROTATING WINDOW (#221 slice 2b): lexical overlap can't reach
        // beliefs that share no tokens with new experience (the stale
        // Rust-era beliefs vs python lessons, glass-boxed live). Each pass
        // therefore ALSO re-examines a few of her oldest not-yet-reviewed
        // beliefs — eventual coverage of the whole belief store, a few
        // beliefs per dream, marked reviewed regardless of verdict.
        {
            let already: HashSet<Uuid> = {
                let seen = reviewed.lock().unwrap(); // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
                let mut set = seen.get(&persona_id).cloned().unwrap_or_default(); // JUSTIFIED: no entry for this persona yet means nothing reviewed; the empty set is the true value, not a guess.
                set.extend(prior_beliefs.iter().map(|e| e.id));
                set
            };
            let rotating = reflector.admission.semantic_beliefs_oldest_excluding(
                &already,
                continuum_base::time::EpochMs::now().millis().saturating_sub(REVIEW_MIN_AGE_MS),
                ROTATING_REVIEW_PER_PASS,
            );
            let mut seen = reviewed.lock().unwrap(); // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
            let entry = seen.entry(persona_id).or_default();
            for b in &rotating {
                entry.insert(b.id);
            }
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
                mark_consolidated(&consolidated, persona_id, cluster);
                // Apply the model's supersession verdict (#221 slice 2):
                // the replaced beliefs drop to the salience floor NOW —
                // the new fact out-ranks them in recall immediately, and
                // the decay drain owns them from here. Applied ONLY on a
                // successful admit: if the new fact didn't land, the old
                // beliefs keep their standing (never orphan her knowledge).
                apply_supersessions(&reflector, persona_id, &fact.supersedes);
            }
            Ok(AdmissionDecision::Drop { .. }) => {
                // Content-hash dedup already has this fact (e.g. a
                // post-restart re-distillation). Mark the sources
                // consolidated so we stop re-spending inference on them.
                mark_consolidated(&consolidated, persona_id, cluster);
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
            .distill_with(LENS_HISTORIAN, Some(persona_id), &fresh)
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
    let already: HashSet<Uuid> = reviewed
        .lock()
        .unwrap() // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
        .get(&persona_id)
        .cloned()
        .unwrap_or_default(); // JUSTIFIED: no entry for this persona yet means nothing reviewed; the empty set is the true value, not a guess.
    let backlog = reflector.admission.semantic_beliefs_oldest_excluding(
        &already,
        continuum_base::time::EpochMs::now().millis().saturating_sub(REVIEW_MIN_AGE_MS),
        REVIEW_ONLY_BATCH,
    );
    if !backlog.is_empty() {
        review_batch(&reflector, persona_id, backlog, &reviewed).await;
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
    let obs_budget_chars = {
        let window = if live.ready && live.served_context_window > 0 {
            live.served_context_window
        } else {
            crate::inference::planning::MIN_SERVE_CTX
        };
        let completion_reserve = window / 4;
        (window.saturating_sub(completion_reserve) as usize).saturating_mul(4)
    };
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

/// The REVIEW-ONLY dream pass (#221 slice 2c): re-examine a batch of her
/// oldest beliefs with no fresh material required. The distiller sees the
/// beliefs as both material and numbered supersession candidates; its verdict
/// demotes what no longer holds; the (possibly refreshed) restatement admits
/// through the normal dedup gate. Every belief in the batch is marked reviewed
/// regardless of outcome, so the queue drains and a wedged batch cannot spin
/// the dream forever (the in-memory reviewed set resets on restart — a full
/// re-review cycle then is harmless by idempotence).
pub(super) async fn review_pass(
    reflector: PersonaReflector,
    persona_id: Uuid,
    beliefs: Vec<Engram>,
    reviewed: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
) {
    review_batch(&reflector, persona_id, beliefs, &reviewed).await;
}

/// The review core, callable from BOTH dream shapes: the quiet-day pass above,
/// and the tail of every BUSY dream (an active persona always has fresh
/// material, so a quiet day never comes for her — the backlog must drain a
/// batch per dream regardless; glass-boxed 2026-07-22: Atlas chatted all
/// evening and the review-only branch never once fired).
async fn review_batch(
    reflector: &PersonaReflector,
    persona_id: Uuid,
    beliefs: Vec<Engram>,
    reviewed: &Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
) {
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
            return;
        }
    }
    {
        let mut seen = reviewed.lock().unwrap(); // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
        let entry = seen.entry(persona_id).or_default();
        for b in &beliefs {
            entry.insert(b.id);
        }
    }
    tracing::info!(
        persona = %persona_id,
        reviewed = beliefs.len(),
        superseded = superseded_n,
        probe_class = "persona.dream.review_pass",
        "dream: quiet-day belief review complete"
    );
}

/// Parse (and strip) a trailing `SUPERSEDES: 1,3` verdict line, mapping the
/// model's 1-based indices onto the presented prior-belief ids. Tolerant by
/// design: no line / `SUPERSEDES: none` / out-of-range indices → no
/// supersessions from that fragment; the line is removed from the fact text
/// either way when present. Only the LAST line is consulted — a fact whose
/// body legitimately mentions the word "supersedes" is untouched.
///
/// The line is NORMALIZED before matching: surrounding markdown emphasis and
/// backticks are stripped, and the keyword is matched case-insensitively. The
/// verdict is the model's JUDGMENT; how it decorated the line is not part of
/// it. Glass-boxed 2026-08-13 — a live `**SUPERSEDES: 3,6**` matched none of
/// the three exact prefixes, so a real supersession ruling was DISCARDED and
/// the marker stayed baked into the stored belief. Honor the emission idiom
/// structurally, the same lesson as the tool-call formats.
pub(super) fn parse_supersedes_line(raw: &str, prior_beliefs: &[Engram]) -> (String, Vec<Uuid>) {
    /// Markdown decoration a model may wrap the verdict line in.
    const DECORATION: &[char] = &['*', '_', '`', '#', ' ', '\t'];
    const KEYWORD: &str = "SUPERSEDES:";

    let Some(last) = raw.lines().last() else {
        return (raw.trim().to_string(), Vec::new());
    };
    let trimmed = last.trim_matches(|c: char| DECORATION.contains(&c));
    let Some(rest) = trimmed
        .get(..KEYWORD.len())
        .filter(|head| head.eq_ignore_ascii_case(KEYWORD))
        .map(|_| &trimmed[KEYWORD.len()..])
    else {
        return (raw.trim().to_string(), Vec::new());
    };
    let body = raw[..raw.len() - last.len()].trim().to_string();
    let ids = rest
        .split(',')
        .filter_map(|t| t.trim().parse::<usize>().ok())
        .filter_map(|i| i.checked_sub(1).and_then(|i| prior_beliefs.get(i)))
        .map(|e| e.id)
        .collect();
    (body, ids)
}

/// Episodics not yet folded into a fact for this persona.
pub(super) fn fresh_episodics(
    consolidated: &Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    persona_id: Uuid,
    episodics: &[Engram],
) -> Vec<Engram> {
    let consolidated = consolidated.lock().unwrap(); // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
    let seen = consolidated.get(&persona_id);
    episodics
        .iter()
        .filter(|e| seen.map_or(true, |set| !set.contains(&e.id)))
        .cloned()
        .collect()
}

/// Record a cluster's episodics as consolidated (clustering-input dedup).
fn mark_consolidated(
    consolidated: &Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    persona_id: Uuid,
    cluster: &[Engram],
) {
    let mut consolidated = consolidated.lock().unwrap(); // JUSTIFIED: std Mutex poisons only after a panic in another dream task; fail loud rather than read a half-updated ledger.
    let set = consolidated.entry(persona_id).or_default();
    for e in cluster {
        set.insert(e.id);
    }
}
