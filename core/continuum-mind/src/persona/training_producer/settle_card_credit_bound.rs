use super::*;

pub async fn settle_card_credit_bound(
    card_id: Uuid,
    passed: bool,
    binding: Option<&graded::BenchmarkCreditAttribution>,
) -> Result<(), String> {
    let Some(executor) = EXECUTOR.cloned() else {
        return Err("credit executor is not ready".into());
    };
    let Some(registry) = crate::persona::PersonaAircRuntimeRegistry::try_global() else {
        return Err("persona registry is not ready".into());
    };
    for (persona_name, persona_id) in registry.roster_snapshot() {
        if binding.is_some_and(|b| b.evidence.owner != persona_id) {
            continue;
        }
        let handle = format!("@persona:{persona_name}");
        let conn = Connection::new(InProcessTransport::new(
            executor.clone(),
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona_id),
            )),
        ));
        // The SUBSTRATE does the bookkeeping on her store (the rejected mark, shedding a
        // transferred row's lived copy); her own identity resolves to Trusted, where
        // data/update is refused (modules/grid/acl.rs; cards fd47a804, 44aee459). Her acts
        // (staging, decisions, submits) stay under her identity above.
        let substrate = substrate_connection(&executor);
        let listed = match conn
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection": StagedCredit::COLLECTION,
                    "dbPath": handle,
                    "filter": { "cardId": card_id.to_string() },
                }),
            )
            .await
        {
            Ok(v) => staged_credit_from_list(v),
            Err(e) => Err(e),
        };
        let listed = match (listed, binding) {
            (Ok(rows), Some(binding)) => graded::bound_rows_retained(&conn, &persona_name, binding, rows).await,
            (listed, _) => listed,
        };
        let rows = match listed {
            Ok(rows) => rows,
            Err(e) => {
                if binding.is_some() {
                    return Err(format!("attributed credit cannot be admitted: {e}"));
                }
                continuum_base::probe!(
                    class = "training.credit.settle_unreadable",
                    persona = %persona_name,
                    card = %card_id,
                    error = %e,
                    "a citizen's staged-credit store could not be read at settle — her credit on this card stands unsettled"
                );
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }
        // VERDICT ORDER (card ad4bfaeb): a failure marks every row staged so far; a pass
        // trains only what was staged after it. The order is her own core's perception of
        // the failure, so no clock on the reviewer's node is compared with hers.
        let verdict = match passed.then(|| pass_admits(&rows)) {
            None => CardVerdict::Failed,
            Some(PassAdmits::All) => CardVerdict::Passed,
            Some(PassAdmits::AfterFailure) => CardVerdict::Corrected,
        };
        let rows = if passed {
            match pass_admits(&rows) {
                PassAdmits::All => rows,
                PassAdmits::AfterFailure => {
                    let (rejected, admitted): (Vec<StagedCredit>, Vec<StagedCredit>) =
                        rows.into_iter().partition(|r| r.rejected_at_ms.is_some());
                    continuum_base::probe!(
                        class = "training.credit.rejected_excluded",
                        persona = %persona_name,
                        card = %card_id,
                        excluded = rejected.len() as u64,
                        admitted = admitted.len() as u64,
                        excluded_ids = %rejected.iter().map(|r| r.id.to_string()).collect::<Vec<_>>().join(","),
                        "a passing verdict after a failure: the rows the failure rejected are not trained as passing; only the correction settles"
                    );
                    admitted
                }
            }
        } else {
            // The SUBSTRATE marks her rows, not she: the mark is settlement bookkeeping on her
            // store, and her own connection resolves to Trusted, where data/update is refused
            // (modules/grid/acl.rs; card fd47a804). Through her identity every mark failed and a
            // later pass trained the rejected rows anyway.
            mark_rejected(&substrate, &persona_name, card_id, &rows).await;
            rows
        };
        let mut submitted = 0usize;
        let mut rows = rows;
        rows.sort_by_key(|r| r.staged_at_ms);
        for (row, dropped) in across_turns(&rows) {
            if let Some(reason) = dropped {
                continuum_base::probe!(
                    class = "training.credit.loop_dropped",
                    persona = %persona_name,
                    card = %card_id,
                    submission = %row.id,
                    reason,
                    "a staged turn that repeats an earlier turn's actions on this card, or took none, is not a training example"
                );
                continue;
            }
            match settle_staged_row(&conn, &substrate, persona_id, &persona_name, row, verdict).await {
                Ok(true) => submitted += 1,
                Ok(false) => {}
                Err(error) => continuum_base::probe!(
                    class = "training.credit.settle_failed",
                    persona = %persona_name,
                    card = %card_id,
                    submission = %row.id,
                    error = %error,
                    "staged revision retained; settlement has no completed ownership transfer"
                ),
            }
        }
        // The card is a boundary for its maker: one bounded consolidation pass, now.
        if let Some(region) = crate::cognition::dream_consolidation::global() {
            let pid = persona_id;
            tokio::spawn(async move {
                let _ = region.consolidate_at_card_boundary(pid).await;
            });
        }
        continuum_base::probe!(
            class = "training.credit.settled",
            persona = %persona_name,
            card = %card_id,
            passed,
            staged = rows.len() as u64,
            submitted = submitted as u64,
            "card verdict checked staged revisions; acknowledged transfers retain inspectable evidence"
        );
        crate::cognition::health_ledger::note_credit_settled();
    }
    Ok(())
}

/// `data/list` returns DataRecord envelopes, not bare entities. Validate the
/// complete response before transferring any row; malformed evidence stays put.
pub fn staged_credit_from_list(listed: serde_json::Value) -> Result<Vec<StagedCredit>, ClientError> {
    let listed: crate::orm::list_result::DataListResult = serde_json::from_value(listed)?; // Decode the Value-native data/list response at its command boundary.
    if listed.items.len() != listed.total as usize {
        return Err(ClientError::Transport(format!(
            "staged-credit list is incomplete: {} of {} rows",
            listed.items.len(),
            listed.total,
        )));
    }
    listed
        .items
        .into_iter()
        .map(|item| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(item)?; // Decode the persisted ORM envelope; move its entity payload without cloning it.
            Ok(serde_json::from_value(record.data)?) // Decode one staged-credit entity from its typed storage envelope.
        })
        .collect()
}

/// Transfer one immutable revision using the destination's existing idempotency
/// key. The accepted receipt proves ownership independently of training dispatch;
/// a refusal, failed outcome, missing claim or incompatible lane keeps evidence.
/// Generation reservations also cover ordinary reviewed submissions, so a later
/// cumulative successor cannot repeat the same acts under an earlier verdict.
pub(super) fn staged_submission_params(
    persona_id: Uuid,
    persona_name: &str,
    row: &StagedCredit,
    verdict: impl Into<CardVerdict>,
) -> Result<serde_json::Value, &'static str> {
    let verdict = verdict.into();
    let passed = verdict != CardVerdict::Failed;
    // Every arm NAMES itself. This used to be one `Option`, and returning `None`
    // was the ONLY exit on the settle path that left no trace anywhere — every
    // other one emits a probe. A row could be skipped forever and the ledger would
    // show a staged credit that simply never moved, with nothing to read (card
    // f1771c83: 728 staged rows, 0 completed transfers).
    let eligible_role = match (passed, row.claim_id, row.owner, row.role) {
        (true, Some(_), Some(owner), Some(role)) if owner == persona_id => role,
        (false, ..) => {
            return Err("card verdict was FAIL — staged credit is discarded, not submitted")
        }
        (_, None, ..) => return Err("row carries no claim_id"),
        (_, _, None, _) => return Err("row carries no owner"),
        (_, _, Some(owner), _) if owner != persona_id => {
            return Err("row's owner is a different persona")
        }
        _ => return Err("row carries no role"),
    };
    // The full receipts are authoritative, including records written before the
    // scalar served summary existed. Never borrow another row's model.
    let served = served_provenance(&row.receipts)
        .filter(|served| !served.model.trim().is_empty() && !served.provider.trim().is_empty())
        .ok_or("no served provenance (model+provider) in this row's receipts")?;
    // Her examples are the calls she lived, never the flat prompt and completion: a row
    // whose capture is incomplete stays staged and named until a read completes it
    // (card ad107e18; Codex's audit, which found the flat path training silently).
    let lived = row
        .lived
        .as_deref()
        .filter(|calls| !calls.is_empty())
        .ok_or("no complete lived record of this turn's calls: it waits for one, never trains as flat text")?;
    let stamp = OutcomeStamp {
        card_id: row.card_id,
        role: eligible_role,
        outcome: passed,
    };
    let plan = plan(
        CLASSIFIER.get_or_init(DomainClassifier::new),
        &row.prompt,
        &row.completion,
        Some(stamp),
    )
    .ok_or("no training plan could be built from this turn's prompt/completion")?;
    let mut params = build_submit_params(
        persona_id,
        persona_name,
        &served.model,
        &plan,
        "card-credit",
    );
    params["examples"] = serde_json::to_value(lived_examples(&plan, lived)).map_err(|_| "lived examples could not be serialized")?; // training-trigger submit boundary: the params ARE the command's wire payload
    params["submissionId"] = json!(row.id);
    crate::cognition::held_out::stamp_examples(&mut params, row.task.as_ref());
    if verdict == CardVerdict::Corrected {
        // The reviewed correction (#4900) is the one labelled source: its own bucket, and one
        // correction trains, through the same /train path at the one default schedule
        // (`native_jobs::DEFAULT_LEARNING_RATE`, 1e-4, measured on the 5090 2026-10-10).
        params["traitKind"] = json!(format!("{}/{CORRECTED_BUCKET}", plan.bucket_key()));
        params["minExamples"] = json!(1);
    }
    Ok(params)
}

/// The row with the calls she lived. A row staged while its capture was incomplete
/// reads them again from its own receipts' cursors, so a capture that has since landed
/// completes it; a read that is still incomplete leaves the row as it was, and the
/// lived rule in [`staged_submission_params`] names it.
pub(super) async fn with_lived<'a>(
    persona_id: Uuid,
    persona_name: &str,
    row: &'a StagedCredit,
) -> std::borrow::Cow<'a, StagedCredit> {
    if row.lived.as_deref().is_some_and(|calls| !calls.is_empty()) {
        return std::borrow::Cow::Borrowed(row);
    }
    match super::speech_discipline_trait::read_lived(persona_id, persona_name, &row.receipts, Vec::new()).await {
        (calls, true) => std::borrow::Cow::Owned(StagedCredit {
            lived: Some(calls),
            ..row.clone()
        }),
        (_, false) => std::borrow::Cow::Borrowed(row),
    }
}

/// One example per call she lived (card ad107e18). Each carries its call whole, so a
/// trainer that renders it trains on the exact request she was served and the exact
/// response she gave, reasoning and tool calls included. The flat `prompt` is the
/// call's last message and `completion` her text, for trainers that take text only.
/// Every example keeps the plan's metadata (card, role, outcome, domain).
pub(super) fn lived_examples(
    plan: &SubmitPlan,
    calls: &[crate::genome::fine_tuning::LivedCall],
) -> Vec<TrainingExample> {
    let metadata = example_metadata(plan, "card-credit");
    let kept = not_a_loop(calls);
    if kept.len() < calls.len() {
        continuum_base::probe!(
            class = "training.example.loop_dropped",
            card = %plan.stamp.map(|s| s.card_id).unwrap_or_default(), // unwrap_or_default: an unstamped plan has no card to name
            calls = calls.len() as u64,
            kept = kept.len() as u64,
            "calls that repeated the previous action unchanged are not training examples"
        );
    }
    kept
        .into_iter()
        .map(|call| TrainingExample {
            prompt: call
                .request
                .messages
                .last()
                .map(|m| m.content_text())
                .unwrap_or_default(),
            completion: call.response.text.clone(),
            metadata: Some(metadata.clone()),
            lived: Some(call.clone()),
        })
        .collect()
}

/// The calls of a turn that are not a LOOP (Kimi's rule, card ad107e18: "61 act batches
/// under one thought, no progress" is a thing she must not learn). A call whose action
/// repeats the previous call's unchanged (the same tool calls with the same arguments,
/// or the same text when it made no call) changed nothing; the first of a run is the
/// action and stays, the repeats go. Order is kept.
pub(super) fn not_a_loop(calls: &[crate::genome::fine_tuning::LivedCall]) -> Vec<&crate::genome::fine_tuning::LivedCall> {
    fn action(call: &crate::genome::fine_tuning::LivedCall) -> (Vec<(&str, String)>, &str) {
        let tools: Vec<(&str, String)> = call
            .response
            .tool_calls
            .iter()
            .flatten()
            .map(|t| (t.name.as_str(), t.input.to_string()))
            .collect();
        let text = if tools.is_empty() { call.response.text.trim() } else { "" };
        (tools, text)
    }
    let mut kept = Vec::with_capacity(calls.len());
    let mut previous = None;
    for call in calls {
        let this = action(call);
        if previous.as_ref() != Some(&this) {
            kept.push(call);
        }
        previous = Some(this);
    }
    kept
}

/// A card's staged turns, in staging order, each with the reason it is NOT an example
/// (Kimi's loop rule across turns, card ad107e18). Her real loops were 58 and 84
/// CONSECUTIVE work turns on one card, each repeating the last with zero acts. A turn
/// whose actions repeat the turn right before it is the loop, and only the first of the
/// run stays. Every turn that called no tool counts as the SAME action, so a run of
/// no-action turns collapses whatever their wording, while a single spoken turn (a
/// deliverable written as speech) stays, and so does a re-read after an edit (the
/// verify step): only CONSECUTIVE repeats are the loop (Cormac on #4452). A turn with no
/// lived record cannot be judged; it breaks the run, and settlement holds it until a read
/// completes its record.
pub(super) fn across_turns(rows: &[StagedCredit]) -> Vec<(&StagedCredit, Option<&'static str>)> {
    let mut previous: Option<Vec<(String, String)>> = None;
    rows.iter()
        .map(|row| {
            let Some(calls) = row.lived.as_deref() else {
                previous = None;
                return (row, None);
            };
            let actions: Vec<(String, String)> = not_a_loop(calls)
                .into_iter()
                .flat_map(|c| c.response.tool_calls.iter().flatten())
                .map(|t| (t.name.clone(), t.input.to_string()))
                .collect();
            let repeat = previous.as_ref() == Some(&actions);
            previous = Some(actions);
            (row, repeat.then_some("repeats_the_previous_turn"))
        })
        .collect()
}

pub async fn settle_staged_row<T: Transport, S: Transport>(
    conn: &Connection<T>,
    substrate: &Connection<S>,
    persona_id: Uuid,
    persona_name: &str,
    row: &StagedCredit,
    verdict: impl Into<CardVerdict>,
) -> Result<bool, ClientError> {
    // An accepted transfer is done: its lived copy may already be shed, and it needs no
    // params, so it never reaches the lived rule below as a false "incomplete".
    if reviewed::transfer_accepted(conn, persona_name, row.id).await? {
        clear_lived(substrate, persona_name, row).await;
        return Ok(false);
    }
    // The params take the calls she lived; the reservation below snapshots the row as
    // stored, so its digest still matches what a binding recorded.
    let resolved = with_lived(persona_id, persona_name, row).await;
    let params = match staged_submission_params(persona_id, persona_name, &*resolved, verdict) {
        Ok(params) => params,
        Err(reason) => {
            // SAY IT. A staged credit that can never settle is a citizen's work
            // that will never reach her curriculum, and the old silent `None`
            // meant the only evidence was a row that sat still.
            continuum_base::probe!(
                class = "training.credit.settle_ineligible",
                persona = %persona_name,
                card = %row.card_id,
                revision = %row.id,
                reason,
                "a staged revision cannot be submitted for settlement — named so a stalled credit is readable instead of invisible"
            );
            return Ok(false);
        }
    };
    match reviewed::reserve_transfer(conn, persona_name, row, None).await {
        Ok(()) => {}
        Err(reviewed::CreditBindingError::Storage(error)) => return Err(error),
        Err(error) => {
            continuum_base::probe!(
                class = "training.credit.settle_overlap",
                persona = %persona_name,
                card = %row.card_id,
                revision = %row.id,
                error = %error,
                "generation evidence is already reserved; revision retained without another submission"
            );
            return Ok(false);
        }
    }
    let receipt = submit_training(conn, params).await?;
    if receipt
        .acceptance
        .as_ref()
        .is_none_or(|accepted| accepted.submission_id != row.id)
    {
        continuum_base::probe!(
            class = "training.credit.settle_deferred",
            persona = %persona_name,
            card = %row.card_id,
            submission = %row.id,
            error_kind = ?receipt.error_kind,
            "training destination did not acknowledge this revision; evidence retained"
        );
        return Ok(false);
    }
    let replayed = receipt
        .acceptance
        .as_ref()
        .is_some_and(|accepted| accepted.replayed);
    let dispatch_success = receipt.success;
    reviewed::accept_transfer(conn, persona_name, row.id, receipt).await?;
    clear_lived(substrate, persona_name, row).await;
    continuum_base::probe!(
        class = "training.credit.transferred",
        persona = %persona_name,
        card = %row.card_id,
        submission = %row.id,
        replayed,
        dispatch_success,
        "destination durably accepted this staged revision; training is a separate outcome"
    );
    Ok(true)
}

/// The bucket suffix a reviewed correction trains under.
pub(crate) const CORRECTED_BUCKET: &str = "corrected";

/// What a card's verdict makes of the rows it settles. A pass after a failure on the same
/// card is the reviewed correction (#4900): the labelled example the examiner's acceptance
/// test is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardVerdict {
    Failed,
    Passed,
    Corrected,
}

impl From<bool> for CardVerdict {
    fn from(passed: bool) -> Self {
        if passed { CardVerdict::Passed } else { CardVerdict::Failed }
    }
}

/// Which staged rows a PASSING verdict may train (card ad4bfaeb).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassAdmits {
    /// No failure on this card: every row is the work that passed.
    All,
    /// Her core observed a failure: only the rows staged after it (unmarked) train.
    AfterFailure,
}

/// PURE: what a pass admits, from the rows' marks.
pub(crate) fn pass_admits(rows: &[StagedCredit]) -> PassAdmits {
    if rows.iter().any(|r| r.rejected_at_ms.is_some()) {
        PassAdmits::AfterFailure
    } else {
        PassAdmits::All
    }
}

/// A FAILED verdict marks every row staged on the card so far as rejected, on her own
/// core's clock. `conn` must be the SUBSTRATE's (no caller identity): a persona
/// identity resolves to Trusted, where data/update is refused. A row already marked keeps its first mark (a second failure doesn't move it).
/// A write that fails is named; that row may then train on a later pass, which is the
/// old behaviour, never a new one.
pub(super) async fn mark_rejected<T: Transport>(conn: &Connection<T>, persona_name: &str, card_id: Uuid, rows: &[StagedCredit]) {
    let now = EpochMs::now().millis();
    for row in rows.iter().filter(|r| r.rejected_at_ms.is_none()) {
        let marked = conn
            .commands()
            .execute_value(
                "data/update",
                json!({
                    "collection": StagedCredit::COLLECTION,
                    "id": row.id,
                    "data": { "rejectedAtMs": now },
                    "dbPath": format!("@persona:{persona_name}"),
                }),
            )
            .await
            .and_then(|result| storage_ok(&result, "data/update", StagedCredit::COLLECTION));
        if let Err(error) = marked {
            continuum_base::probe!(
                class = "training.credit.reject_mark_failed",
                persona = %persona_name,
                card = %card_id,
                submission = %row.id,
                error = %error,
                "a row the failure rejected could not be marked; a later pass may still train it"
            );
        }
    }
}

/// Once the destination holds a row's lived calls as examples, the row keeps its
/// receipts (the provenance) and drops the second copy, so settled rows stay small.
/// A failed clear is named; the next settle pass meets the accepted transfer and
/// clears again.
/// `substrate` must be the SUBSTRATE's connection (no caller identity): this write
/// is bookkeeping on her store, and her own identity resolves to Trusted, where
/// data/update is refused. Measured on the 5090 (2026-10-10, 26 refusals in one pass):
/// through her identity no transferred row ever shed its lived copy, so her staged
/// store grew by a second copy of every call she had already handed to training.
pub async fn clear_lived<S: Transport>(substrate: &Connection<S>, persona_name: &str, row: &StagedCredit) {
    if row.lived.is_none() {
        return;
    }
    let cleared = substrate
        .commands()
        .execute_value(
            "data/update",
            json!({
                "collection": StagedCredit::COLLECTION,
                "id": row.id,
                "data": { "lived": serde_json::Value::Null },
                "dbPath": format!("@persona:{persona_name}"),
            }),
        )
        .await
        .and_then(|result| storage_ok(&result, "data/update", StagedCredit::COLLECTION));
    if let Err(error) = cleared {
        continuum_base::probe!(
            class = "training.credit.lived_retained",
            persona = %persona_name,
            submission = %row.id,
            error = %error,
            "transferred row still holds its lived calls; the next settle pass clears them"
        );
    }
}

/// RECONCILE AT REGISTRATION. A review that arrived while she was not resident (a deploy, a
/// sleeping node) reached no live bridge, so her registration asks every board she is on what
/// the room has already judged on her cards (`submission_reviews_for_card`, airc #1537) and
/// sends each through the SAME exact path the live bridge and `work/review` use, with the same
/// self-review rule: the immutable submission binding and its transfer reservations, never a
/// card-wide settle (Codex root, #5017). Idempotent: a transferred binding returns Accepted.
pub async fn reconcile_reviews(persona_name: &str, airc: &airc_lib::Airc) {
    let owned = match crate::persona::airc_runtime::scoped_board_owned_by(airc).await {
        Ok(owned) => owned,
        Err(error) => {
            continuum_base::probe!(
                class = "training.credit.reconcile_unreadable",
                persona = %persona_name,
                error = %error,
                "her boards could not be walked at registration: reviews already on them reach the exact path on the next live delivery"
            );
            return;
        }
    };
    let observer = airc.peer_id().as_uuid();
    let conn = match reviewed::credit_connection(observer) {
        Ok(conn) => conn,
        Err(error) => {
            continuum_base::probe!(class = "training.credit.reconcile_unreadable", persona = %persona_name, error = %error,
                "no credit connection at registration: reviews already on her boards wait for the next live delivery");
            return;
        }
    };
    let mut boards: std::collections::BTreeMap<Uuid, (airc_work::WorkBoardProjection, reviewed::SelfReview)> =
        std::collections::BTreeMap::new();
    let (mut cards, mut reviews, mut deferred) = (0u64, 0u64, 0u64);
    for (room, card) in owned {
        cards += 1;
        let room_id = room.channel.as_uuid();
        if !boards.contains_key(&room_id) {
            match airc.work_board_in(&room).await {
                Ok(board) => {
                    let admission = reviewed::self_review(airc, &room).await;
                    boards.insert(room_id, (board, admission));
                }
                Err(error) => {
                    continuum_base::probe!(
                        class = "training.credit.reconcile_unreadable",
                        persona = %persona_name,
                        room = %room_id,
                        error = %error,
                        "one board could not be projected at registration: its reviews wait for the next live delivery"
                    );
                    continue;
                }
            }
        }
        let Some((board, admission)) = boards.get(&room_id) else { continue };
        let ids: Vec<airc_work::WorkReviewId> =
            board.submission_reviews_for_card(card.card_id).map(|r| r.review_id).collect();
        for review_id in ids {
            reviews += 1;
            let why = match reviewed::consume_observed_review(&conn, persona_name, observer, room_id, board, review_id, admission).await {
                Ok(Some(credit)) if credit.state == reviewed::ReviewedCreditState::PolicyUnavailable => {
                    "the room's review policy could not be read".to_string()
                }
                Ok(Some(credit)) if credit.state == reviewed::ReviewedCreditState::ProvenanceHeld => {
                    "its submission's provenance is held (not staged yet, or shared)".to_string()
                }
                Ok(_) => continue,
                Err(error) => error.to_string(),
            };
            deferred += 1;
            continuum_base::probe!(
                class = "training.credit.review_deferred",
                persona = %persona_name,
                review = %review_id.as_uuid(),
                why = %why,
                "a review already on her board was not settled at registration: her next registration (each core start) replays it"
            );
        }
    }
    continuum_base::probe!(
        class = "training.credit.reconciled",
        persona = %persona_name,
        cards,
        reviews,
        deferred,
        "registration sent every review already on her boards through the exact path"
    );
}

/// Every card an INSTANCE names (a round's cards for it) settles when its verdict
/// is written — the hook `record_verdict` fires, spawned so a verdict never waits
/// on a curriculum write.
pub fn settle_instance_credit(instance: &str, passed: bool) {
    let cards = crate::cognition::bench_round::cards_for_instance(instance);
    if cards.is_empty() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        continuum_base::probe!(
            class = "training.credit.settle_deferred",
            instance,
            "verdict written outside the runtime — its cards' staged credit settles on the next verdict of this instance"
        );
        return;
    };
    for card in cards {
        handle.spawn(settle_card_credit(card, passed));
    }
}
