use super::*;

/// The silence token, exactly as [`crate::cognition::deliberation_parse`] reads it.
pub const SILENCE_COMPLETION: &str = "PASS";

/// The pure plan for a gated echo: the burst she was handed → the silence token.
pub fn plan_speech_discipline(burst: &str) -> SubmitPlan {
    SubmitPlan {
        trait_kind: SPEECH_DISCIPLINE_TRAIT.to_string(),
        prompt: burst.to_string(),
        completion: SILENCE_COMPLETION.to_string(),
        quality: 1.0,
        stamp: None,
    }
}

/// Stage one gated echo as a speech-discipline example. Best-effort and non-blocking
/// (mirrors [`produce_received`]); quietly a no-op before the executor is installed.
/// `marker` names which gate rule tripped (probe provenance only).
pub fn produce_speech_discipline(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    burst: String,
    marker: &'static str,
) {
    let Some(executor) = EXECUTOR.cloned() else {
        tracing::debug!(
            persona = %persona_id,
            "training_producer: executor not installed yet — skipping speech-discipline capture"
        );
        return;
    };
    continuum_base::probe!(
        class = "training.example.speech_discipline",
        persona = %persona_name,
        marker = marker,
        burst_chars = burst.chars().count() as u64,
        "a gated echo staged as {{burst → PASS}} — the gate becomes a lesson"
    );
    tokio::spawn(async move {
        let plan = plan_speech_discipline(&burst);
        submit_plan(
            persona_id,
            persona_name,
            base_model,
            executor,
            plan,
            SPEECH_DISCIPLINE_SOURCE,
        )
        .await;
    });
}

/// Build the wire params for `genome/training-trigger/submit` from a plan — the ONE
/// payload contract, pure + `pub` so a synchronous caller (`memory/consolidate`
/// dispatching received lessons on demand) produces byte-identical params to the
/// fire-and-forget producers. `source: raw` = unfiltered capture; minExamples omitted →
/// DEFAULT_MIN_EXAMPLES (the trigger auto-fires job-create at the threshold). `evalSet`
/// rides the {trait → gym} edge: the committed gym for `plan.trait_kind` when there is one
/// (omitted when there is none), kept for a manual spot-check. Adoption does not read it:
/// the sentinel opens an in-room gene trial either way. `provenance` is metadata only
/// (live-turn vs received-lesson).
/// The audit metadata every example of a plan carries: source, quality, the bare
/// domain, and the card stamp when there is one.
pub(super) fn example_metadata(plan: &SubmitPlan, provenance: &str) -> serde_json::Value {
    let mut metadata = json!({
        "source": provenance,
        "quality": plan.quality,
        "domain": plan.trait_kind,
    });
    if let (Some(stamp), serde_json::Value::Object(map)) = (plan.stamp, &mut metadata) {
        map.insert("cardId".into(), json!(stamp.card_id));
        map.insert("role".into(), json!(stamp.role.as_str()));
        map.insert("outcome".into(), json!(stamp.outcome));
    }
    metadata
}

pub fn build_submit_params(
    persona_id: Uuid,
    persona_name: &str,
    base_model: &str,
    plan: &SubmitPlan,
    provenance: &str,
) -> serde_json::Value {
    // `domain` stays the BARE domain in metadata even when the bucket key widens:
    // the domain is what the example is ABOUT, the key is where it is filed. Losing
    // the bare domain here would make a stamped example unqueryable alongside its
    // unstamped siblings.
    let example = TrainingExample {
        prompt: plan.prompt.clone(),
        completion: plan.completion.clone(),
        metadata: Some(example_metadata(plan, provenance)),
        lived: None,
    };
    let bucket = plan.bucket_key();
    let mut params = json!({
        "personaId": persona_id,
        "personaName": persona_name,
        "baseModel": base_model,
        "traitKind": bucket,
        "examples": [example],
        "source": "raw",
    });
    // Gym lookup keys on the DOMAIN, not the widened bucket: `code/owner` has no
    // gym of its own, and the trait it measures is still `code`. Passing the bucket
    // here would silently drop the eval set for every stamped example, which is the
    // [[no-fallbacks-ever]] shape — a capability quietly lost, not refused. The test
    // asserts this positively (`gym_for_trait(bucket_key()) == None`) rather than by
    // comparing two `is_some()`s, which is how it managed to guard nothing for so long.
    if let Some(eval_set) = crate::cognition::gym::gym_for_trait(&plan.trait_kind) {
        if let serde_json::Value::Object(ref mut map) = params {
            map.insert(
                "evalSet".into(),
                serde_json::Value::String(eval_set.to_string()),
            );
        }
    }
    params
}

/// The shared submit tail used by BOTH producers ([`produce`], [`produce_received`]):
/// build the persona's identity-bearing `Connection`, package the plan via
/// [`build_submit_params`], and dispatch `genome/training-trigger/submit`. Identical
/// flywheel entry for a live turn and a received lesson — only the `provenance` label
/// differs. One submit path, N experience sources.
pub(super) async fn submit_plan(
    persona_id: Uuid,
    persona_name: String,
    base_model: String,
    executor: Arc<CommandExecutor>,
    plan: SubmitPlan,
    provenance: &'static str,
) {
    // The persona's own hands: a Connection carrying its LocalPersona identity through
    // the wired executor, so the Privileged submit is gated AS the persona
    // ([[persona-is-a-client]]). `persona_id` IS the airc `peer_id` (equal by invariant).
    let conn = Connection::new(InProcessTransport::new(
        executor,
        Some(CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(persona_id),
        )),
    ));
    let params = build_submit_params(persona_id, &persona_name, &base_model, &plan, provenance);
    match submit_training(&conn, params).await {
        Ok(receipt) if receipt.success => {
            continuum_base::probe!(
                class = "training.example.produced",
                persona = %persona_name,
                domain = %plan.trait_kind,
                quality = plan.quality as f64,
                provenance = provenance,
                outcome = %receipt.outcome.as_deref().unwrap_or("unspecified"),
                // THE JOIN KEY, not a count. Without it this probe says an example
                // was accepted but not WHICH submission it became, so a room turn
                // cannot be joined to `training_trigger_submissions` or to dispatch
                // state — and a count can never establish a causal link, only a
                // correlation someone will read as one (Astra, 2026-09-08).
                //
                // Emitted as the destination's OWN id, never minted here: an id we
                // invented would join to nothing and would look exactly like one
                // that did.
                // `Option` rather than a "none" String: an absent acceptance OMITS
                // the field instead of emitting a sentinel that reads like a value.
                // "none" in an id column is exactly the absence-as-data shape this
                // card exists to remove — and it allocated per probe. (Astra
                // verified tracing-core implements Value for Option<T>.)
                submission = receipt
                    .acceptance
                    .as_ref()
                    .map(|a| tracing::field::display(&a.submission_id)),
                // A replay is an acceptance the destination has SEEN BEFORE. Without
                // this, a retry and a first submission are the same row.
                replayed = receipt.acceptance.as_ref().is_some_and(|a| a.replayed),
                "training trigger accepted the example (L2)"
            );
        }
        Ok(receipt) => {
            continuum_base::probe!(
                class = "training.example.rejected",
                persona = %persona_name,
                domain = %plan.trait_kind,
                provenance = provenance,
                error_kind = %receipt.error_kind.as_deref().unwrap_or("unspecified"), // Missing diagnostic stays explicit after the receipt's success:false refusal.
                error = %receipt.error.as_deref().unwrap_or("submit returned success:false"), // Display-only absence label; the typed receipt remains unchanged.
                "training trigger did not report successful submission"
            );
            tracing::warn!(
                persona = %persona_id,
                error_kind = ?receipt.error_kind,
                error = ?receipt.error,
                "training_producer: submit refused (best-effort, turn unaffected)"
            );
        }
        Err(error) => {
            continuum_base::probe!(
                class = "training.example.failed",
                persona = %persona_name,
                domain = %plan.trait_kind,
                provenance = provenance,
                error = %error,
                "training submission has no valid receipt"
            );
            tracing::warn!(
                persona = %persona_id,
                error = %error,
                "training_producer: submit failed (best-effort, turn unaffected)"
            );
        }
    }
}

/// Decode the command's outcome once at the typed boundary. Transport success is
/// not acceptance: InconsistentBucket and DispatchFailed deliberately return Ok
/// with success:false. All producers, including consolidation, inspect this receipt.
/// Persist a card-linked turn as a STAGED credit — the parent row plus one child
/// row per generation — instead of submitting it as training data now.
///
/// Card 0d51573a. A `TrainingExample` is `{prompt, completion}` with no preference
/// schema anywhere in the tree, so submission is ONE-WAY: a turn submitted before
/// its card settles cannot be withdrawn when that card later fails. Staging is what
/// makes the evidence floor reachable — the example waits for a verdict that may
/// never arrive, and never arriving is the correct outcome rather than a leak.
///
/// **Schemas are ensured through the registry, never inferred.** `data/ensure-schema`
/// resolves a collection by NAME from the boot registry, which is why both are
/// registered in `persona::register_substrate_orm_entities`. Skipping it would not
/// fail loudly: sqlite would build both tables from the data's SHAPE and silently
/// omit every declared index, including the unique one that stops a single
/// generation being credited to a staged turn twice.
///
/// The whole write is ONE `data/batch`, which is atomic on both adapters — a parent
/// surviving its rolled-back children would be a credit record asserting provenance
/// it cannot produce.
#[allow(clippy::too_many_arguments)]
pub async fn stage_credit<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    credit: &CapturedCredit,
    generation_receipts: Vec<crate::ai::provenance::GenerationReceipt>,
    prompt: String,
    completion: String,
    lived: Option<Vec<crate::genome::fine_tuning::LivedCall>>,
    submission_id: Uuid,
    replaces: &[Uuid],
) -> Result<Uuid, ClientError> {
    // Her OWN store, not the shared main DB: a citizen's staged credit is her
    // record of her own turns.
    let handle = format!("@persona:{persona_name}");
    // `submission_id` identifies immutable payload, not mutable turn content. A
    // retry replaces only that identical revision; a new revision removes known
    // and uncertain predecessors. Children cascade in the same transaction.

    for collection in [StagedCredit::COLLECTION, StagedCreditGeneration::COLLECTION] {
        let result = conn
            .commands()
            .execute_value(
                "data/ensure-schema",
                // `dbPath`, NOT `handle`. `CommandRequest<P>` flattens the params into the
                // SAME JSON object as its own `handle: Option<HandleRef>`, so a
                // top-level "handle" is claimed by the ENVELOPE and never reaches the
                // params' `handle: Option<String>` — the request fails to deserialize
                // with `invalid type: string, expected struct HandleRef`. `dbPath` is the
                // params' WIRE name for it (card ea28d2f6 made that the rename, not an
                // alias, and the envelope's schema guard refuses the bare name).
                json!({ "collection": collection, "dbPath": handle }),
            )
            .await?;
        storage_ok(&result, "data/ensure-schema", collection)?;
    }

    // The ordered receipts remain authoritative. A single scalar is meaningful
    // only if all usable generations agree on the model AND provider.
    let served = served_provenance(&generation_receipts);
    let parent = StagedCredit {
        id: submission_id,
        card_id: credit.card_id,
        claim_id: credit.claim.as_ref().map(|c| c.claim_id),
        owner: credit.claim.as_ref().map(|c| c.owner.as_uuid()),
        role: credit.claim.as_ref().map(|c| c.role),
        receipts: generation_receipts,
        served,
        prompt,
        completion,
        lived,
        staged_at_ms: continuum_base::time::EpochMs::now().millis(),
        coursework_set: credit.task.coursework_set().map(String::from),
        task: Some(credit.task.clone()),
        rejected_at_ms: None,
    };

    let mut operations = Vec::new();
    for prev in replaces
        .iter()
        .filter(|id| **id != submission_id)
        .chain(std::iter::once(&submission_id))
    {
        operations.push(BatchOperation {
            operation_type: BatchOperationType::Delete,
            collection: StagedCredit::COLLECTION.to_string(),
            id: Some(prev.to_string()),
            data: None,
        });
    }
    operations.push(BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: StagedCredit::COLLECTION.to_string(),
        id: Some(submission_id.to_string()),
        data: Some(serde_json::to_value(&parent)?), // ORM batch boundary: BatchOperation.data is Value by contract
    });
    // One correlation row per generation, faults included: the receipts themselves
    // ride on the parent as an opaque JSON column (the ORM has no collection
    // FieldType), so these child rows ARE the index that column cannot carry.
    for receipt in &parent.receipts {
        let child = StagedCreditGeneration {
            id: Uuid::new_v4(),
            staged_credit_id: submission_id,
            submitted_request_id: receipt.submitted_request_id.clone(),
        };
        operations.push(BatchOperation {
            operation_type: BatchOperationType::Create,
            collection: StagedCreditGeneration::COLLECTION.to_string(),
            id: Some(child.id.to_string()),
            data: Some(serde_json::to_value(&child)?), // ORM batch boundary: BatchOperation.data is Value by contract
        });
    }

    let result = conn
        .commands()
        .execute_value(
            "data/batch",
            // `dbPath` for the same reason as above: a top-level "handle" is eaten by
            // the envelope's HandleRef field.
            json!({ "operations": operations, "dbPath": handle }),
        )
        .await?;
    storage_ok(&result, "data/batch", StagedCredit::COLLECTION)?;
    Ok(submission_id)
}

/// Read every served call of a turn back from her prompt capture (card ad107e18).
///
/// Each receipt that carries a capture cursor and was SERVED is decoded; a faulted
/// call is not something she did. `known` holds calls this turn already read, so a
/// restage every four acts reads only the new ones. Returns every call read (the
/// caller keeps them as its cache) and whether the set is COMPLETE. An incomplete
/// set is never staged: a turn with a hole in it would train as if the hole were
/// not there. Named either way, never silent.
pub(super) async fn read_lived(
    persona_id: Uuid,
    persona_name: &str,
    receipts: &[crate::ai::provenance::GenerationReceipt],
    known: Vec<crate::genome::fine_tuning::LivedCall>,
) -> (Vec<crate::genome::fine_tuning::LivedCall>, bool) {
    use crate::ai::provenance::GenerationOutcome;
    let served: Vec<Option<String>> = receipts
        .iter()
        .filter(|r| matches!(r.outcome, GenerationOutcome::Served { .. }))
        .map(|r| r.capture.clone())
        .collect();
    let read = tokio::task::spawn_blocking(move || {
        let dir = crate::cognition::capture::fixture_dir(crate::cognition::prompt_capture::FIXTURE_DIR);
        read_lived_calls(dir.as_deref(), persona_id, served, known)
    })
    .await;
    let (calls, missing) = match read {
        Ok(read) => read,
        Err(e) => (Vec::new(), Some(format!("capture read task: {e}"))),
    };
    match (&missing, calls.is_empty()) {
        (None, false) => {
            continuum_base::probe!(
                class = "training.credit.lived_read",
                persona = %persona_name,
                calls = calls.len() as u64,
                "the staged turn holds every served call as she lived it"
            );
            (calls, true)
        }
        _ => {
            let reason = missing.unwrap_or_else(|| "the turn has no served call".into());
            continuum_base::probe!(
                class = "training.credit.lived_unavailable",
                persona = %persona_name,
                reason = %reason,
                read = calls.len() as u64,
                "the staged turn holds no lived record yet; settlement holds it until a read completes one"
            );
            (calls, false)
        }
    }
}

/// The blocking read behind [`read_lived`]: every served call's cursor in dispatch
/// order (`None` for a served call no capture recorded), decoded from `dir`. Returns
/// the calls read and the first reason the set is incomplete, if any.
pub(super) fn read_lived_calls(
    dir: Option<&std::path::Path>,
    persona_id: Uuid,
    served: Vec<Option<String>>,
    known: Vec<crate::genome::fine_tuning::LivedCall>,
) -> (Vec<crate::genome::fine_tuning::LivedCall>, Option<String>) {
    let mut known: std::collections::HashMap<String, crate::genome::fine_tuning::LivedCall> =
        known.into_iter().map(|c| (c.capture.clone(), c)).collect();
    let mut calls = Vec::with_capacity(served.len());
    let mut missing: Option<String> = None;
    for cursor in served {
        let Some(cursor) = cursor else {
            missing.get_or_insert_with(|| "a served call carries no capture cursor".into());
            continue;
        };
        if let Some(call) = known.remove(&cursor) {
            calls.push(call);
            continue;
        }
        let Some(dir) = dir else {
            missing.get_or_insert_with(|| "capture directory unavailable".into());
            continue;
        };
        let decoded = crate::cognition::prompt_capture::detail(dir, persona_id, &cursor)
            .map_err(|e| e.to_string())
            .and_then(crate::cognition::prompt_capture::decode)
            .and_then(|call| {
                call.response
                    .map(|response| (call.request, response))
                    .ok_or_else(|| "no terminal response".to_string())
            });
        match decoded {
            Ok((request, response)) => calls.push(crate::genome::fine_tuning::LivedCall {
                capture: cursor,
                request,
                response,
            }),
            Err(e) => {
                missing.get_or_insert_with(|| format!("capture {cursor}: {e}"));
            }
        }
    }
    (calls, missing)
}

/// Summarize a homogeneous served lane using a real representative request. Mixed
/// models/providers and all-faulted turns retain their full receipts without a
/// fabricated scalar base model. Fault receipts never become successful output.
pub(super) fn served_provenance(
    receipts: &[crate::ai::provenance::GenerationReceipt],
) -> Option<ServedProvenance> {
    use crate::ai::provenance::GenerationOutcome;
    let mut first: Option<(&str, &str, &str)> = None;
    for receipt in receipts {
        if let GenerationOutcome::Served {
            model,
            provider,
            provider_request_id,
        } = &receipt.outcome
        {
            if first.is_some_and(|(m, p, _)| m != model || p != provider) {
                return None;
            }
            first.get_or_insert((
                model,
                provider,
                provider_request_id
                    .as_deref()
                    .unwrap_or(&receipt.submitted_request_id), // Equal provider/submitted IDs are stored once in the receipt.
            ));
        }
    }
    first.map(|(model, provider, request_id)| ServedProvenance {
        model: model.to_owned(),
        provider: provider.to_owned(),
        request_id: request_id.to_owned(),
    })
}

/// Fail on a command that SUCCEEDED AS A CALL and FAILED AS AN OPERATION.
///
/// `execute_value` returns `Ok` for anything that made the round trip, and the data
/// commands report adapter failure INSIDE the payload as
/// `StorageResult { success: false, error }` — which `InProcessTransport` forwards
/// unchanged. So `.await?` catches a broken transport and nothing else: a child
/// constraint violation rolls the whole batch back, the call still returns `Ok`, and
/// without this the caller emitted `training.credit.staged` with a submission id for
/// rows that do not exist (Astra/S6 on 799b8fe9). Transport success is not operation
/// success, and a receipt that cannot report failure is not a receipt.
pub fn storage_ok(
    value: &serde_json::Value,
    command: &str,
    collection: &str,
) -> Result<(), ClientError> {
    // Decoded as the TYPED result the data layer actually returns, not by poking at a
    // "success" key: a shape change should break the build here rather than silently
    // read as false and start rejecting every staging attempt.
    let decoded: crate::orm::types::StorageResult<serde_json::Value> =
        serde_json::from_value(value.clone())?; // ORM boundary: decode the data layer's typed StorageResult
    if decoded.success {
        return Ok(());
    }
    // `Refused` is the variant whose own doc says "substrate accepted the command but
    // returned an error" — which is precisely this: the call arrived, the operation
    // did not happen.
    Err(ClientError::Refused {
        command: command.to_string(),
        outcome: None,
        reason: format!(
            "`{collection}`: {}",
            decoded
                .error
                .as_deref()
                // An unsuccessful result with no message is still a failure. Saying so
                // beats inventing a reason or, worse, reading the silence as success.
                .unwrap_or("reported success=false with no error message"),
        ),
    })
}

pub async fn submit_training<T: Transport>(
    conn: &Connection<T>,
    params: serde_json::Value,
) -> Result<SubmitOutcome, ClientError> {
    let receipt = conn
        .commands()
        .execute_value("genome/training-trigger/submit", params)
        .await?;
    Ok(serde_json::from_value(receipt)?) // Decode the Value-native command response once; producers share this typed receipt.
}

/// The COMPLETION a work turn that ended in acts is lifted as: the intent she
/// stated plus the exact calls she made, rendered deterministically. This is the
/// ACTED chain the learning loop never saw (2026-09-13: every work turn ends
/// `Acted`; only the rare spoken report reached the producer, so a day of five
/// coders produced 14 conversation examples and zero code ones). A card that later
/// settles PASS turns each of these into a code example in her curriculum.
pub fn acted_completion(intent: &str, calls: &[crate::ai::types::ToolCall]) -> String {
    let mut out = String::new();
    let intent = intent.trim();
    if !intent.is_empty() {
        out.push_str(intent);
        out.push('\n');
    }
    for c in calls {
        out.push_str(&format!(
            "{}({})\n",
            c.name,
            serde_json::to_string(&c.input).unwrap_or_default() // JUSTIFIED unwrap_or_default: an unserializable arg renders empty; the call NAME still trains the shape
        ));
    }
    out
}

/// The whole turn's act chain, rendered in order — one `acted_completion` per act.
pub fn acted_chain(turn_acts: &[(String, Vec<crate::ai::types::ToolCall>)]) -> String {
    turn_acts
        .iter()
        .map(|(intent, calls)| acted_completion(intent, calls))
        .collect::<Vec<_>>()
        .join("")
}

/// The substrate's own connection to this core: no caller identity, so the gate resolves
/// it to Owner. For the producer's BOOKKEEPING on a resident's store only (the rejected
/// mark, shedding a transferred row's lived copy); never for her acts, which carry her
/// identity so provenance stays hers.
pub fn substrate_connection(executor: &Arc<CommandExecutor>) -> Connection<InProcessTransport> {
    Connection::new(InProcessTransport::new(executor.clone(), None))
}

/// Settle a card's staged credit: eligible claimed turns on `card_id` may transfer
/// after a passing verdict, through the same quality gate as live turns. Failed,
/// unknown and incompatible evidence remains staged. Reads each
/// resident's own staged store, the mirror of `stage_credit`. Best-effort and
/// receipted: a store that cannot be read names itself; nothing here can fail the
/// verdict that triggered it.
pub async fn settle_card_credit(card_id: Uuid, passed: bool) {
    let _ = settle_card_credit_bound(card_id, passed, None).await;
}
