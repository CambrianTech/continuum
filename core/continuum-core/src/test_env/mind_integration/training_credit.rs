#![cfg(test)]
//! From `persona::training_producer` (and one `cognition::act_observe` scenario): her
//! training credit staged, reviewed, bound and settled through core's REAL `DataModule` over
//! SQLite under the production `GridTrustAuthPolicy` (surface modules the mind crate cannot
//! name), and the fixtures only these tests use. Each asserts exactly what it asserted
//! before the move.

use std::sync::Arc;

use continuum_client::{ClientError, Connection, Transport};
use serde_json::json;
use uuid::Uuid;

use crate::orm::OrmEntity;
use crate::paths::test_home::HomeGuard;
use crate::persona::training_producer::test_support::*;
use crate::persona::training_producer::*;
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport};

mod automatic;
mod bindings;
mod directed;
mod lived;
mod refused_submit;
mod reviewed_credit;
mod settlement;
mod staging;

/// A real `DataModule` over a real SQLite adapter, reached through the SAME
/// dispatch and authorization path production uses — a local persona must
/// not pass here merely because the fixture omitted the grid trust policy.
pub(super) fn data_runtime() -> Arc<CommandExecutor> {
    let registry = Arc::new(crate::runtime::ModuleRegistry::new());
    registry.register(Arc::new(crate::modules::data_wiring::data_module()));
    let executor = Arc::new(
        CommandExecutor::new(registry.clone())
            .with_policy(Arc::new(crate::modules::grid::GridTrustAuthPolicy::new())),
    );
    registry.install_executor_on_all(executor.clone());
    executor
}

/// Shared real SQLite fixture for the production turn-driver regression.
pub(crate) fn turn_capture(
    persona_id: Uuid,
    persona_name: String,
    prompt: String,
    credit: CapturedCredit,
) -> TurnCreditCapture {
    TurnCreditCapture::with_executor(data_runtime(), persona_id, persona_name, prompt, credit)
}

pub(super) fn substrate_conn(executor: &Arc<CommandExecutor>) -> Connection<InProcessTransport> {
    substrate_connection(executor)
}

pub(super) fn conn_as(executor: Arc<CommandExecutor>, persona: Uuid) -> Connection<InProcessTransport> {
    Connection::new(InProcessTransport::new(
        executor,
        Some(CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(persona),
        )),
    ))
}

/// Storage stays on the real SQLite command route. Only the destination's
/// reply is scripted, with an optional pause at the ownership transfer.
pub(super) struct SettlementTransport {
    pub(super) data: InProcessTransport,
    pub(super) submit: continuum_client::mock::MockTransport,
    pub(super) pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}

#[async_trait::async_trait]
impl Transport for SettlementTransport {
    async fn execute(
        &self,
        command: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        if command == "genome/training-trigger/submit" {
            if let Some((entered, release)) = &self.pause {
                entered.notify_one();
                release.notified().await;
            }
            self.submit.execute(command, params).await
        } else {
            self.data.execute(command, params).await
        }
    }

    async fn subscribe(
        &self,
        class: &str,
    ) -> Result<continuum_client::event::EventStream, ClientError> {
        self.data.subscribe(class).await
    }

    async fn emit(&self, class: &str, payload: serde_json::Value) -> Result<(), ClientError> {
        self.data.emit(class, payload).await
    }

    async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn continuum_client::ServeHandler>,
    ) -> Result<(), ClientError> {
        self.data.provide(command, handler).await
    }

    async fn revoke(&self, command: &str) -> Result<(), ClientError> {
        self.data.revoke(command).await
    }

    async fn close(&self) -> Result<(), ClientError> {
        self.data.close().await
    }
}

pub(super) fn settlement_connection(
    executor: Arc<CommandExecutor>,
    persona: Uuid,
    submit: continuum_client::mock::MockTransport,
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
) -> Connection<SettlementTransport> {
    Connection::new(SettlementTransport {
        data: InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona),
            )),
        ),
        submit,
        pause,
    })
}

pub(super) async fn stored_credit_rows(
    conn: &Connection<InProcessTransport>,
    name: &str,
) -> Vec<StagedCredit> {
    let listed = conn.commands().execute_value(
        "data/list",
        json!({"collection": StagedCredit::COLLECTION, "dbPath": format!("@persona:{name}")}),
    ).await.unwrap();
    staged_credit_from_list(listed).unwrap()
}

pub(super) fn settlement_acts(count: usize) -> Vec<(String, Vec<crate::ai::types::ToolCall>)> {
    (0..count)
        .map(|n| {
            (
                format!(
                    "Inspect source file {n} and verify its behavior against the regression."
                ),
                vec![crate::ai::types::ToolCall {
                    id: format!("read-{n}"),
                    name: "code/read".into(),
                    input: json!({"path":format!("src/file_{n}.rs")}),
                }],
            )
        })
        .collect()
}

/// `receipt`, with its call really captured under `persona` in the process home's capture
/// directory, the way the turn driver leaves a served call: the cursor it carries reads back
/// as the call she lived. Receipts that are not served, or already carry a cursor, pass as-is.
pub(super) fn captured(
    persona: Uuid,
    receipt: crate::ai::provenance::GenerationReceipt,
) -> crate::ai::provenance::GenerationReceipt {
    use crate::cognition::prompt_capture::{CaptureLease, JsonlPromptCaptureSink, PromptCall, FIXTURE_DIR};
    let served = matches!(receipt.outcome, crate::ai::provenance::GenerationOutcome::Served { .. });
    if !served || receipt.capture.is_some() {
        return receipt;
    }
    // One capture session per home and persona, as one turn driver keeps: every store open
    // rotates (only the current and previous segments are kept), so a session per receipt
    // would expire all but the last two captures before she staged them.
    static SINKS: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<(std::path::PathBuf, Uuid), Arc<JsonlPromptCaptureSink>>>,
    > = std::sync::LazyLock::new(Default::default);
    let dir = crate::cognition::capture::fixture_dir(FIXTURE_DIR).expect("the test home resolves"); // JUSTIFIED: test fixture
    let mut sinks = SINKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner); // unwrap_or_else: a panicked test leaves the map usable
    // Home-pinned tests run one at a time (HomeGuard): a sink of another home belongs to a
    // finished test, and its handles go with it so its temp home can be removed.
    sinks.retain(|(home, _), _| *home == dir);
    let sink = sinks
        .entry((dir.clone(), persona))
        .or_insert_with(|| Arc::new(JsonlPromptCaptureSink::open(&dir, persona).expect("capture sink"))) // JUSTIFIED: test fixture
        .clone();
    drop(sinks);
    let call = lived_call(&receipt);
    let prompt = PromptCall {
        request_id: receipt.submitted_request_id.clone(),
        persona_id: persona,
        room_id: Uuid::new_v4(),
        cycle_id: Some(1),
        context_window: Some(32_768),
        cause: "fixture",
        cause_root: None,
        replay_of: None,
    };
    let mut lease = CaptureLease::start(sink, &prompt, &call.request);
    let cursor = lease.finish(Some(&call.response), None).expect("a completed capture entry"); // JUSTIFIED: test fixture
    crate::ai::provenance::GenerationReceipt { capture: Some(cursor), ..receipt }
}

/// A receipt naming a capture cursor that holds no call: a capture lost on the way.
pub(super) fn uncaptured(id: &str, model: &str) -> crate::ai::provenance::GenerationReceipt {
    crate::ai::provenance::GenerationReceipt {
        capture: Some(format!("never-written-{id}")),
        ..served_receipt(id, model)
    }
}

/// Runs `staged` with the process home's capture directory moved aside, so every read
/// back fails as an unfinished capture does, then puts it back: the capture lands late.
pub(super) async fn captures_unreadable<T>(staged: impl std::future::Future<Output = T>) -> T {
    let dir = crate::cognition::capture::fixture_dir(crate::cognition::prompt_capture::FIXTURE_DIR)
        .expect("the test home resolves"); // JUSTIFIED: test fixture
    let aside = dir.with_extension("aside");
    std::fs::rename(&dir, &aside).expect("move the captures aside"); // JUSTIFIED: test fixture
    let out = staged.await;
    std::fs::rename(&aside, &dir).expect("the captures land"); // JUSTIFIED: test fixture
    out
}

pub(super) async fn stage_settlement_fixture(
    executor: Arc<CommandExecutor>,
    persona: Uuid,
    name: &str,
    card: Uuid,
    receipts: Vec<crate::ai::provenance::GenerationReceipt>,
) -> (TurnCreditCapture, StagedCredit) {
    let receipts: Vec<_> = receipts.into_iter().map(|r| captured(persona, r)).collect();
    let mut capture = TurnCreditCapture::with_executor(
        executor.clone(),
        persona,
        name.into(),
        "Inspect the Rust source and report the verified regression behavior.".into(),
        CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: card,
            claim: Some(ClaimReceipt {
                claim_id: Uuid::new_v4(),
                owner: airc_core::PeerId::from_uuid(persona),
                role: CreditRole::Owner,
            }),
        },
    );
    assert!(
        capture
            .record(&settlement_acts(4), &receipts, None, false)
            .await
    );
    let row = stored_credit_rows(&conn_as(executor, persona), name)
        .await
        .into_iter()
        .find(|row| row.receipts == receipts)
        .unwrap();
    (capture, row)
}

pub(super) fn script_settlement_reply(
    submit: &continuum_client::mock::MockTransport,
    row: &StagedCredit,
    reply: Option<serde_json::Value>,
) {
    let id = row.id;
    let model = row.served.as_ref().unwrap().model.clone();
    let captures: Vec<serde_json::Value> = row
        .receipts
        .iter()
        .filter(|r| matches!(r.outcome, crate::ai::provenance::GenerationOutcome::Served { .. }))
        .map(|r| json!(r.capture))
        .collect();
    submit.respond_to("genome/training-trigger/submit", move |params| {
        assert_eq!(
            params["submissionId"],
            json!(id),
            "retry carries this immutable snapshot's identity"
        );
        assert_eq!(
            params["baseModel"],
            json!(model),
            "each row supplies its actual serving model"
        );
        // Exact payload: the examples are this row's own captured calls, in receipt order,
        // each completion the response she gave; never the flat prompt and completion.
        let examples = params["examples"].as_array().cloned().unwrap_or_default(); // unwrap_or_default: a missing list fails the equality below
        let cursors: Vec<serde_json::Value> = examples.iter().map(|e| e["lived"]["capture"].clone()).collect();
        assert_eq!(cursors, captures, "the examples are exactly this row's captured calls");
        for example in &examples {
            assert_eq!(example["completion"], example["lived"]["response"]["text"], "each completion is the response she gave");
        }
        match &reply {
            Some(value) => Ok(value.clone()),
            None => Err(ClientError::Transport(
                "acceptance acknowledgement was lost".into(),
            )),
        }
    });
}

/// The exact-review scenario: her staged row bound to one submission on one card, a
/// reviewer, and the room's work events up to the review. Shared by every exact-path
/// test, so each states only its own case (CLAUDE.md: one fixture per concern).
/// A room's board from these work events, signed by their peers, through airc's own codec and
/// replay boundary: what any observer of the room would project.
pub(super) fn project_events(room: Uuid, events: Vec<(airc_work::WorkEvent, airc_core::PeerId)>) -> airc_work::WorkBoardProjection {
    airc_work::project_transcript_work_events(events.into_iter().enumerate().map(|(n, (event, peer_id))| {
        let (headers, body) = airc_work::encode_work_event(&event).unwrap();
        airc_core::TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: airc_core::RoomId::from_uuid(room),
            peer_id,
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::System,
            occurred_at_ms: event.occurred_at_ms(),
            lamport: n as u64 + 1,
            target: airc_core::MentionTarget::All,
            headers,
            body: Some(body),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }))
    .unwrap()
}

pub(super) struct ExactReview {
    pub(super) _home: (HomeGuard, tempfile::TempDir),
    pub(super) executor: Arc<CommandExecutor>,
    pub(super) persona: Uuid,
    pub(super) name: &'static str,
    pub(super) owner: airc_core::PeerId,
    pub(super) reviewer: airc_core::PeerId,
    pub(super) card_id: airc_work::WorkCardId,
    pub(super) room: Uuid,
    pub(super) receipts: Vec<crate::ai::provenance::GenerationReceipt>,
    pub(super) selected: StagedCredit,
    pub(super) selection: reviewed::SubmissionSelection,
    pub(super) data: Connection<InProcessTransport>,
    pub(super) repo: airc_work::RepoId,
    pub(super) pass: airc_work::WorkSubmissionReview,
    pub(super) prefix: Vec<(airc_work::WorkEvent, airc_core::PeerId)>,
}


impl ExactReview {
    pub(super) fn create(&self, card_id: airc_work::WorkCardId, reviews: Option<airc_work::WorkCardId>) -> airc_work::WorkEvent {
        airc_work::WorkEvent::CardCreated(airc_work::CardCreated {
            card_id,
            repo: self.repo.clone(),
            title: "ordinary work with reviewed deliverable".into(),
            body: None,
            priority: airc_work::Priority::P1,
            lane_id: None,
            created_by: self.owner,
            created_at_ms: 1,
            reviews,
            origin: None,
        })
    }

    pub(super) fn claim(card_id: airc_work::WorkCardId, claim_id: airc_work::ClaimId, holder: airc_core::PeerId) -> airc_work::WorkEvent {
        airc_work::WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
            selected_at_ms: None,
            origin: airc_work::ClaimOrigin::Unknown,
            card_id,
            claim_id,
            owner: holder,
            ttl_ms: 1000,
            claimed_at_ms: 2,
        })
    }

    /// The room's board after the scenario's events plus `tail`, through the existing
    /// codec and replay boundary, including the authoritative transcript peer.
    pub(super) fn project(&self, tail: Vec<(airc_work::WorkEvent, airc_core::PeerId)>) -> airc_work::WorkBoardProjection {
        project_events(self.room, self.prefix.iter().cloned().chain(tail).collect())
    }
}

pub(super) async fn exact_review() -> (ExactReview, TurnCreditCapture) {
    exact_review_bound_by(None).await
}

/// [`exact_review`], with the submission bound the way `work/submit` binds it: explicitly
/// (`None`), or by recording the generation that issued it (`Some`), resolved at review.
pub(super) async fn exact_review_bound_by(issuing: Option<&str>) -> (ExactReview, TurnCreditCapture) {
    exact_review_staged(issuing, |_| vec![served_receipt("reviewed-request", "actual-model")], false).await
}

/// [`exact_review_bound_by`], staging the receipts `receipts` makes for the persona (each
/// served one captured unless it already names a cursor), with the capture directory
/// unreadable while she stages when `unreadable_at_stage`: a capture that lands late.
pub(super) async fn exact_review_staged(
    issuing: Option<&str>,
    receipts: impl FnOnce(Uuid) -> Vec<crate::ai::provenance::GenerationReceipt>,
    unreadable_at_stage: bool,
) -> (ExactReview, TurnCreditCapture) {
    use airc_work::{WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
    let home = tempfile::tempdir().unwrap();
    let guard = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .unwrap();
    let executor = data_runtime();
    let persona = Uuid::new_v4();
    let owner = airc_core::PeerId::from_uuid(persona);
    let reviewer = airc_core::PeerId::new();
    let card_id = airc_work::WorkCardId::new();
    let room = Uuid::new_v4();
    let name = "reviewed-project-work";
    let receipts: Vec<_> = receipts(persona).into_iter().map(|r| captured(persona, r)).collect();
    let (capture, selected) = if unreadable_at_stage {
        captures_unreadable(stage_settlement_fixture(executor.clone(), persona, name, card_id.as_uuid(), receipts)).await
    } else {
        stage_settlement_fixture(executor.clone(), persona, name, card_id.as_uuid(), receipts).await
    };
    let receipts = selected.receipts.clone();
    let selection = reviewed::SubmissionSelection {
        submission_id: Uuid::new_v4(),
        room_id: room,
        card_id: card_id.as_uuid(),
        claim_id: selected.claim_id.unwrap(),
        staged_revision_id: selected.id,
        instance: "ordinary-project-work".into(),
        base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
        artifact: serde_json::from_value(json!({"hash":"b".repeat(64),"size_bytes":12})).unwrap(),
    };
    let data = conn_as(executor.clone(), persona);
    match issuing {
        None => {
            reviewed::bind_submission(&data, name, persona, selection.clone()).await.unwrap();
        }
        Some(generation) => {
            let pending = reviewed::PendingSelection {
                publication: reviewed::Publication {
                    submission_id: selection.submission_id,
                    room_id: room,
                    card_id: selection.card_id,
                    claim_id: selection.claim_id,
                    instance: selection.instance.clone(),
                    base_sha: selection.base_sha.clone(),
                    artifact: selection.artifact.clone(),
                },
                issuing_generation: generation.into(),
            };
            if let Err(e) = reviewed::record_pending(&data, name, persona, pending).await {
                panic!("the fixture records its pending binding: {e}");
            }
        }
    }
    let submitted = airc_work::WorkSubmission {
        submission_id: airc_work::SubmissionId::from_uuid(selection.submission_id),
        card_id,
        claim_id: airc_work::ClaimId::from_uuid(selection.claim_id),
        instance: selection.instance.clone(),
        base_sha: selection.base_sha.clone(),
        artifact: selection.artifact.clone(),
        publisher: owner,
        submitted_at_ms: 3,
    };
    let review_card = airc_work::WorkCardId::new();
    let review_claim = airc_work::ClaimId::new();
    let pass = WorkSubmissionReview {
        review_id: airc_work::WorkReviewId::new(),
        card_id,
        submission_id: submitted.submission_id,
        artifact: submitted.artifact.clone(),
        review_card_id: review_card,
        review_claim_id: review_claim,
        reviewer,
        outcome: WorkReviewOutcome::Passed,
        evidence: submitted.artifact.clone(),
        evidence_text: None,
        reviewed_at_ms: 4,
    };
    let mut fx = ExactReview {
        _home: (guard, home),
        executor,
        persona,
        name,
        owner,
        reviewer,
        card_id,
        room,
        receipts,
        selected,
        selection,
        data,
        repo: airc_work::RepoId::new("acme/ordinary-project").unwrap(),
        pass,
        prefix: Vec::new(),
    };
    fx.prefix = vec![
        (fx.create(card_id, None), owner),
        (ExactReview::claim(card_id, submitted.claim_id, owner), owner),
        (WorkEvent::WorkSubmitted(submitted), owner),
        (fx.create(review_card, Some(card_id)), reviewer),
        (ExactReview::claim(review_card, review_claim, reviewer), reviewer),
    ];
    (fx, capture)
}
