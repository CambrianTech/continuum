use super::*;

/// The substrate-wired [`CommandExecutor`] (the one `start_server` builds with the
/// `GridTrustAuthPolicy` + interceptors), installed once at boot via
/// [`install_executor`]. The producer dispatches `genome/training-trigger/submit`
/// through a per-persona [`Connection`] over this executor so the submit is gated
/// AS the persona (`LocalPersona` → `Trusted`, which may run the `Privileged`
/// submit but not Owner-gated ops). Late-bound because the service loop has no
/// executor in scope; absent before boot installs it (tests).
pub(super) static EXECUTOR: LateBound<CommandExecutor> = LateBound::new("training_producer::executor");

/// One shared, stateless-for-classification [`DomainClassifier`]. `classify` is a
/// pure function of the text (the instance only holds keyword tables), so a single
/// shared instance is correct and avoids locking the per-persona
/// `PersonaCognition` on the producer path. Lazily built on first turn.
pub(super) static CLASSIFIER: OnceLock<DomainClassifier> = OnceLock::new();

/// Quality bar below which a live turn is NOT worth training on.
///
/// On a live turn there is no human feedback or task outcome, so
/// `score_interaction_quality` reduces to `0.275 + 0.3 * substance` (+ a small
/// structural bonus for code/list formatting). Trivial one-liners ("ok", "thanks"
/// — substance `0.1` → score ≈ `0.305`) fall below this bar; substantive replies
/// (≥100 chars → substance `0.7` → ≈ `0.485`, or ≥500 chars → ≈ `0.545`) clear it.
/// `0.45` is the clean separator between "acknowledgement" and "real content."
pub(super) const MIN_TRAINING_QUALITY: f32 = 0.45;

/// ONE dispatched generation, as a correlation ROW — the index that
/// [`StagedCredit::receipts`] cannot carry (card 0d51573a).
///
/// The ORM's `FieldType` set has no collection type, so the parent's receipts are an
/// opaque JSON column and nothing inside them is indexable. Playback's correlation
/// runs FROM a generation TO the credit, so that direction needs a real index; these
/// rows are it. They hold REFERENCES only — the response payload stays in the parent
/// and is not duplicated here.
///
/// **`submitted_request_id` IS NOT ASSUMED GLOBALLY UNIQUE** (Astra, 2026-09-08).
/// Nothing in the producer contract proves a request id is unique across every staged
/// credit ever written, and a unique index on it alone would reject a legitimate write
/// the first time that assumption failed — silently turning a provenance record into a
/// dropped one. Uniqueness is asserted on the PAIR, which is the thing that genuinely
/// cannot repeat: one turn does not dispatch the same request id twice.
///
/// The row is a child in the truest sense: `on_delete = "cascade"`, so it cannot
/// outlive the credit it describes and there is no orphan class to reap.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "staged_credit_generation")]
#[entity(index(
    name = "idx_staged_credit_generation_pair",
    fields = ["stagedCreditId", "submittedRequestId"],
    unique = true
))]
pub struct StagedCreditGeneration {
    #[entity(primary_key)]
    pub id: Uuid,

    /// The staged credit this generation belongs to.
    #[entity(indexed)]
    #[entity(foreign_key("staged_credit.id", on_delete = "cascade"))]
    pub staged_credit_id: Uuid,

    /// The SUBMITTED request id — the join key playback holds. Indexed on its own so
    /// the generation-to-credit lookup is a real index hit, not a scan.
    #[entity(indexed)]
    pub submitted_request_id: String,
}

/// ONE card-linked turn, held until its card settles (card 0d51573a).
///
/// A card-linked turn must NOT submit at completion. An unstamped example is an
/// immediate POSITIVE SFT target and submission is one-way — `TrainingExample` is
/// `{prompt, completion}` with no preference schema anywhere in the tree — so a turn
/// submitted before its card settles cannot be withdrawn when that card later fails.
/// Observed live 2026-09-08: a citizen accumulating toward a training threshold on a
/// card still in `Claimed` with no settlement.
///
/// Lives in the citizen's own [`crate::cognition::home::PersonaHome`], alongside
/// `engrams.sqlite` — persistence inside its existing owner, per-citizen, NOT a new
/// global queue ([[source-drain-is-the-universal-pattern]]: the drain is settlement).
///
/// ## Two write-once halves, at two different times
///
/// **SELECTION IS IMMUTABLE.** `card_id`, `claim_id`, `owner`, `role` and
/// `submitted_request_id` are written when the turn begins and never rewritten. A
/// MISSING CLAIM REMAINS MISSING: `claim_id` is `None` for a turn rooted at a card
/// whose `WorkCard::owner`/`claim_id` were absent, and no later settlement may fill
/// it in. That turn still stages (it was card-linked) and can never be stamped.
///
/// **SERVED PROVENANCE IS ATTACHED LATER**, once the generation returns, and only
/// ever added — never overwriting a selection field.
///
/// ## Why the id IS the submission id
///
/// Per the #278 contract the submit gains an optional `submissionId` minted ONCE and
/// reused on every retry, so the destination can recognise a replay. Making it this
/// row's primary key means there is no second identifier to keep in sync, and a retry
/// necessarily reuses it because it is reading the same row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "staged_credit")]
pub struct StagedCredit {
    /// The `submissionId` carried to the training-trigger, minted once at stage time
    /// and reused on every retry. Primary key, so the derive pulls in the BaseEntity
    /// columns and a retry cannot mint a second one for the same batch.
    #[entity(primary_key)]
    pub id: Uuid,

    /// The card this turn is credited to. Indexed: settlement arrives per-card and
    /// asks "what did this citizen stage against it".
    #[entity(indexed)]
    pub card_id: Uuid,

    /// The accepted claim captured at SELECTION. `None` means no receipt was taken
    /// and none may ever be supplied — this row can stage but never stamp.
    #[entity(indexed)]
    pub claim_id: Option<Uuid>,

    /// The claim's owner, as the work store spells it. `None` exactly when
    /// `claim_id` is `None`; the two travel together or not at all.
    pub owner: Option<Uuid>,

    /// Which hand the citizen had in the card. JSON because it is a tagged enum.
    #[entity(json)]
    pub role: Option<CreditRole>,

    /// EVERY generation this turn dispatched, in dispatch order, faults included.
    ///
    /// A cycle makes multiple calls and there is no canonical singular request, so
    /// this replaces the single `submitted_request_id` that used to sit here — a
    /// one-to-one field for a one-to-many domain. Stored as an opaque JSON payload
    /// because the ORM has no collection `FieldType`, which is exactly why the
    /// correlation index cannot live in here and gets its own child rows in
    /// [`StagedCreditGeneration`].
    ///
    /// The FULL receipts stay here; the child rows are references, not copies.
    #[entity(json)]
    pub receipts: Vec<crate::ai::provenance::GenerationReceipt>,

    /// Representative served request when all usable generations agree on their
    /// model and provider. Mixed/all-faulted turns have no scalar summary; the
    /// complete ordered receipts above preserve every lane and fault.
    #[entity(json)]
    pub served: Option<ServedProvenance>,

    /// The stimulus, held verbatim so the staged example is the turn she was
    /// actually handed rather than a re-derived approximation.
    pub prompt: String,

    /// Her reply, likewise verbatim.
    pub completion: String,

    /// Every served call of the turn exactly as it happened (card ad107e18), read from
    /// the prompt capture when the turn staged, because the capture rotates within
    /// hours and a card can settle days later. `None` when no call could be read back
    /// (no capture installed, or the read failed, which is probed at stage time), and
    /// cleared once the destination durably accepts the transfer, so a settled row
    /// does not keep a second copy of what the trainer already holds. A row staged
    /// `None` never trains as flat text: settlement re-reads its own receipts' cursors
    /// and holds it, named, until the record is complete.
    #[entity(json)]
    #[serde(default)]
    pub lived: Option<Vec<crate::genome::fine_tuning::LivedCall>>,

    /// When this turn was staged (epoch ms). Not a settlement clock — a staged row
    /// whose card never settles is never submitted, and that is correct, not a leak.
    pub staged_at_ms: u64,

    /// The coursework lesson set this turn worked (`coursework-<sha12>`, the set's task
    /// identity), when the card was a lesson; `None` for real work. Read from the round
    /// tracker when the turn stages, so a lesson turn is told from real work by DATA: the
    /// settle path keeps lesson verdicts out of gene-trial credit (ONE-RESIDENT §10.2: the
    /// promotion gate judges real work only). Which lesson it was is [`StagedCredit::task`].
    /// `default` reads a row staged before this field as real work.
    #[serde(default)]
    pub coursework_set: Option<String>,
    /// Which graded task this turn worked, captured with the card (goal 8). `None` only on a
    /// row staged before tasks were recorded: its examples then read as unknown, so a gene
    /// trained on it is never reported as judged on a known-clean held-out set.
    #[entity(json)]
    #[serde(default)]
    pub task: Option<crate::cognition::held_out::CardTask>,
    /// When HER core observed a FAILED review of this row's card while the row was
    /// staged: her own clock, never the reviewer's (card ad4bfaeb). A row a review
    /// rejected is the behaviour the reviewer refused, so a later PASS on the same card
    /// must not train it as good; only the rows staged after the failure (the
    /// correction) settle as passing. Attached at settlement and only ever added, like
    /// `served`. The flag is also the record of what a gene did NOT train on, kept as a
    /// negative for later.
    #[serde(default)]
    pub rejected_at_ms: Option<u64>,
}

/// A scored, gated, classified training example ready to submit. The pure product
/// of [`plan`] — it separates the JUDGMENT (is this turn worth training on? which
/// domain bucket?) from the EFFECT (dispatch to the trigger), so the judgment is
/// unit-testable with no executor.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitPlan {
    /// The domain bucket — `DomainClassifier::classify(...).domain`. Becomes the
    /// `traitKind` of the `(persona_id, trait_kind, base_model)` bucket key when
    /// UNSTAMPED; a stamped plan keys on `domain × role` (see
    /// [`SubmitPlan::bucket_key`]). There is no outcome segment — a failed turn
    /// never reaches a bucket at all.
    pub trait_kind: String,
    /// The stimulus (the triggering message text).
    pub prompt: String,
    /// The persona's reply.
    pub completion: String,
    /// The interaction-quality score that cleared [`MIN_TRAINING_QUALITY`]
    /// (carried into example metadata as training provenance).
    pub quality: f32,
    /// The verified outcome, when this turn is credited to a settled card.
    /// `None` for an ordinary live turn — the pre-cc34ac0f behaviour, unchanged.
    pub stamp: Option<OutcomeStamp>,
}

impl SubmitPlan {
    /// The bucket this example trains. UNSTAMPED turns keep the bare domain, so
    /// every existing bucket keeps its name and its history; a stamped turn gets
    /// `domain/role`, so a citizen's "code she wrote that passed" is a different
    /// lesson from "review she did that passed". There is NO outcome segment:
    /// a failed turn is refused by [`plan`] before it can be filed anywhere.
    ///
    /// Deliberately NOT a new field on the params: the bucket key is already
    /// `traitKind`, and widening the key rather than adding a parallel dimension
    /// keeps one bucket concept instead of two ([[logic-deepest-level-thin-on-the-way-out]]).
    pub fn bucket_key(&self) -> String {
        match self.stamp {
            None => self.trait_kind.clone(),
            // No `/passed` | `/failed` segment: a FAILED turn never reaches a
            // bucket at all (see the evidence floor in [`plan`]), so every plan
            // that gets this far already carries a passing verdict. Encoding an
            // outcome that is now constant would advertise a distinction the
            // data no longer has — and would invite a `/failed` sibling to be
            // reintroduced as "just another bucket".
            Some(s) => format!("{}/{}", self.trait_kind, s.role.as_str()),
        }
    }
}

/// Score the turn, gate on quality, classify its domain → `Some(SubmitPlan)`;
/// `None` if gated out (trivial / low-quality). Pure: no I/O, no executor, no
/// spawn — this is the testable judgment half of the producer.
///
/// `classifier` is borrowed (the shared [`DomainClassifier`]). Classification runs
/// on the full `(prompt, completion)` pair so both the ask and the answer inform
/// the bucket.
///
/// `stamp` carries a VERIFIED outcome when the turn is credited to a settled card
/// (cc34ac0f). It feeds `score_interaction_quality`'s `task_outcome` — the argument
/// that has always existed and was always `None` on this path. A live, uncredited
/// turn still passes `None` and scores exactly as it did before.
pub fn plan(
    classifier: &DomainClassifier,
    prompt: &str,
    completion: &str,
    stamp: Option<OutcomeStamp>,
) -> Option<SubmitPlan> {
    // THE EVIDENCE FLOOR. A turn credited to a card that FAILED submits NOTHING.
    //
    // This is deliberately not "file it under `/failed` and let training sort it
    // out", which is what an earlier draft of this change did. The reason is the
    // shape of the corpus, not squeamishness: `TrainingExample` is
    // `{prompt, completion}` (genome::fine_tuning::types) and the writers render
    // exactly those two fields — there is NO `chosen`/`rejected` schema anywhere
    // in the tree. So every example in every bucket is an SFT TARGET. A "failed"
    // bucket does not teach "avoid this"; it teaches the failure, under a name
    // that makes it look handled.
    //
    // Until a preference-pair schema exists, the only honest thing a verified
    // failure can do is keep quiet.
    //
    // AND KEEPING QUIET IS ALL IT DOES — a refused turn leaves NO TRACE. There is
    // no plan, so no metadata, no bucket, and no probe. An earlier version of this
    // comment claimed the stamp "still rides on the plan's metadata, so a failure
    // remains ACCOUNTED FOR"; that was false, because there is no plan to carry it.
    // Whether a refused turn SHOULD leave durable provenance is an open question
    // and belongs at the caller that supplies the stamp, not in this pure function
    // (astra-s6-review: a probe here would pollute pure planning, and a probe is
    // glass-box evidence rather than durable accounting — it can be disabled).
    if stamp.is_some_and(|s| !s.outcome) {
        return None;
    }
    let quality = score_interaction_quality(prompt, completion, None, stamp.map(|s| s.outcome));
    if quality.score < MIN_TRAINING_QUALITY {
        return None;
    }
    let domain = classifier
        .classify(&format!("{prompt}\n{completion}"))
        .domain;
    Some(SubmitPlan {
        trait_kind: domain,
        prompt: prompt.to_string(),
        completion: completion.to_string(),
        quality: quality.score,
        stamp,
    })
}

/// Install the substrate-wired [`CommandExecutor`] so the producer can dispatch
/// the trigger. Called once at boot (`ipc/mod.rs`, right after
/// `install_executor_on_all`). Second install is a silent no-op ([`LateBound`]
/// semantics) — the boot executor wins.
pub fn install_executor(executor: Arc<CommandExecutor>) {
    EXECUTOR.install(executor);
}

/// Persist a continuing work turn at the existing four-act boundary.
pub const STAGE_EVERY_ACTS: usize = 4;

pub fn is_stage_point(acts: usize) -> bool {
    acts > 0 && acts % STAGE_EVERY_ACTS == 0
}

/// The selected card and staged revisions belong to THIS turn, not a persona
/// global. The shared driver borrows this owner while it runs. Writes are awaited
/// serially through the existing async data command; cancelling the driver cannot
/// leave a callback attached to another room or eval fork.
/// An in-flight transaction may finish, but it can only replace this turn's rows.
pub struct TurnCreditCapture {
    pub conn: Connection<InProcessTransport>,
    pub(super) persona_id: Uuid,
    pub(super) persona_name: String,
    pub(super) prompt: String,
    pub(super) credit: CapturedCredit,
    pub(super) snapshot: Option<(Uuid, [u8; 32])>,
    pub(super) superseded: Vec<Uuid>,
    /// Calls of this turn already read back from the capture, so each restage
    /// reads only the calls made since the last one.
    pub(super) lived: Vec<crate::genome::fine_tuning::LivedCall>,
}

impl TurnCreditCapture {
    /// This snapshot may carry the generation a pending submission recorded; a review of it
    /// that held because that generation was not staged yet can bind now. Re-drives through
    /// the one replay registration uses (idempotent), only when a pending generation staged.
    async fn redrive_held_reviews(&self, receipts: &[crate::ai::provenance::GenerationReceipt]) {
        match reviewed::issues_pending(&self.conn, &self.persona_name, self.credit.card_id, receipts).await {
            Ok(false) => {}
            Ok(true) => {
                let Some(runtime) = crate::persona::PersonaAircRuntimeRegistry::try_global()
                    .and_then(|registry| registry.get(self.persona_id))
                else {
                    return; // not resident here: her next registration replays it
                };
                redrive::request(self.persona_id, self.persona_name.clone(), runtime);
            }
            Err(error) => continuum_base::probe!(
                class = "training.credit.redrive_unreadable",
                persona = %self.persona_name,
                card = %self.credit.card_id,
                error = %error,
                "pending submissions on this card could not be read after staging: a held review waits for the next review event or registration"
            ),
        }
    }

    /// Opens a capture for each claim a [`super::ClaimFollowingCapture`] follows, on the live
    /// executor, for this turn's persona and prompt.
    pub fn opener(persona_id: Uuid, persona_name: &str, prompt: &str) -> super::OpenCapture {
        let (persona_name, prompt) = (persona_name.to_owned(), prompt.to_owned());
        Box::new(move |credit| Self::for_turn(persona_id, &persona_name, &prompt, Some(credit)))
    }

    pub(crate) fn for_turn(
        persona_id: Uuid,
        persona_name: &str,
        prompt: &str,
        credit: Option<&CapturedCredit>,
    ) -> Option<Self> {
        let credit = credit.filter(|credit| credit.is_card_linked())?;
        let Some(executor) = EXECUTOR.cloned() else {
            continuum_base::probe!(
                class = "training.credit.stage_unavailable",
                persona = %persona_id,
                card = %credit.card_id,
                "card credit capture unavailable: command executor is not installed"
            );
            return None;
        };
        Some(Self::with_executor(
            executor,
            persona_id,
            persona_name.to_owned(),
            prompt.to_owned(),
            credit.clone(),
        ))
    }

    pub fn with_executor(
        executor: Arc<CommandExecutor>,
        persona_id: Uuid,
        persona_name: String,
        prompt: String,
        credit: CapturedCredit,
    ) -> Self {
        Self {
            conn: Connection::new(InProcessTransport::new(
                executor,
                Some(CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(persona_id),
                )),
            )),
            persona_id,
            persona_name,
            prompt,
            credit,
            snapshot: None,
            superseded: Vec::new(),
            lived: Vec::new(),
        }
    }

    /// Snapshot only at a completed-act boundary, or once at the shared driver's
    /// final return. A faulted final generation still belongs in the receipt list.
    /// A failed replacement leaves the previous whole snapshot intact; the next
    /// boundary retries identical content with the SAME identity. Changed content
    /// gets a new revision so a stale settlement cannot delete its successor.
    pub async fn record(
        &mut self,
        acts: &[(String, Vec<crate::ai::types::ToolCall>)],
        receipts: &[crate::ai::provenance::GenerationReceipt],
        report: Option<&str>,
        final_snapshot: bool,
    ) -> bool {
        if (!final_snapshot && !is_stage_point(acts.len()))
            || (acts.is_empty() && report.is_none() && receipts.is_empty())
        {
            return false;
        }
        let started = std::time::Instant::now();
        let mut completion = acted_chain(acts);
        if let Some(report) = report {
            completion.push_str(report);
        }
        let digest = snapshot_digest(&completion, receipts);
        let submission_id = match self.snapshot {
            Some((id, prior)) if prior == digest => id,
            previous => {
                if let Some((id, _)) = previous {
                    self.superseded.push(id);
                }
                let id = Uuid::new_v4();
                // Record the attempt before awaiting: an acknowledgement can be
                // lost after commit. The next revision removes every possible
                // predecessor in its own atomic write, never a successor.
                self.snapshot = Some((id, digest));
                id
            }
        };
        let (lived, complete) = read_lived(
            self.persona_id,
            &self.persona_name,
            receipts,
            std::mem::take(&mut self.lived),
        )
        .await;
        self.lived = lived;
        let result = stage_credit(
            &self.conn,
            &self.persona_name,
            &self.credit,
            receipts.to_vec(),
            self.prompt.clone(),
            completion,
            complete.then(|| self.lived.clone()),
            submission_id,
            &self.superseded,
        )
        .await;
        match result {
            Ok(_) => {
                self.superseded.clear();
                crate::cognition::health_ledger::note_credit_staged();
                continuum_base::probe!(
                    class = "training.credit.staged",
                    persona = %self.persona_name,
                    card = %self.credit.card_id,
                    submission = %submission_id,
                    acts = acts.len() as u64,
                    generations = receipts.len() as u64,
                    final_snapshot,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "card-linked turn snapshot persisted; outcome remains unverified"
                );
                self.redrive_held_reviews(receipts).await;
                true
            }
            Err(error) => {
                continuum_base::probe!(
                    class = "training.credit.stage_failed",
                    persona = %self.persona_name,
                    card = %self.credit.card_id,
                    submission = %submission_id,
                    error = %error,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "turn snapshot refused; prior staged snapshot retained for retry"
                );
                false
            }
        }
    }
}

/// Compare immutable snapshot payloads without retaining another full copy or
/// serializing them. Prompt/card are fixed on the turn owner; these are its only
/// changing fields. Tags and length framing distinguish optional/adjacent strings.
pub(super) fn snapshot_digest(
    completion: &str,
    receipts: &[crate::ai::provenance::GenerationReceipt],
) -> [u8; 32] {
    use crate::ai::provenance::GenerationOutcome;
    use sha2::{Digest, Sha256};
    fn field(hash: &mut Sha256, value: Option<&str>) {
        hash.update([u8::from(value.is_some())]);
        if let Some(value) = value {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
    }
    let mut hash = Sha256::new();
    field(&mut hash, Some(completion));
    for receipt in receipts {
        field(&mut hash, Some(&receipt.submitted_request_id));
        match &receipt.outcome {
            GenerationOutcome::Served {
                model,
                provider,
                provider_request_id,
            } => {
                hash.update([0]);
                field(&mut hash, Some(model));
                field(&mut hash, Some(provider));
                field(&mut hash, provider_request_id.as_deref());
            }
            GenerationOutcome::Faulted {
                detail,
                model,
                provider,
            } => {
                hash.update([1]);
                field(&mut hash, Some(detail));
                field(&mut hash, model.as_deref());
                field(&mut hash, provider.as_deref());
            }
        }
    }
    hash.finalize().into()
}

/// Turn a completed live turn into a buffered training example. Unlinked speech
/// is scored and submitted by the existing best-effort background operation;
/// linked work is retained for a later verified outcome. Never call from eval.
pub fn produce(
    persona_id: Uuid,
    persona_name: String,
    prompt: String,
    completion: String,
    credit: Option<CapturedCredit>,
    generation_receipts: Vec<crate::ai::provenance::GenerationReceipt>,
) {
    produce_with_id(
        persona_id,
        persona_name,
        prompt,
        completion,
        credit,
        generation_receipts,
        Uuid::new_v4(),
        None,
    );
}

/// [`produce`] with the submission id chosen by the caller and, for a re-stage of the
/// same turn, the partial submission it REPLACES (deleted in the same batch).
#[allow(clippy::too_many_arguments)]
pub fn produce_with_id(
    persona_id: Uuid,
    persona_name: String,
    prompt: String,
    completion: String,
    // The card this turn was rooted at, captured AT SELECTION on her hands.
    // `Some` routes the turn to STAGING (it submits only if the card settles PASS);
    // `None` is ordinary conversation and is NOT a training example (card 8e3dd206).
    credit: Option<CapturedCredit>,
    // Every generation this turn dispatched, in order, faults included.
    generation_receipts: Vec<crate::ai::provenance::GenerationReceipt>,
    submission_id: Uuid,
    replaces: Option<Uuid>,
) {
    // AN UNVERIFIED TURN IS NOT A TRAINING EXAMPLE (card 8e3dd206, 2026-09-27).
    //
    // This path used to submit every unlinked live turn that cleared a quality floor,
    // and that floor is a LENGTH score (`0.275 + 0.3 * substance`): nothing in it knows
    // whether the turn was right. Read on the fleet: the IntelMac's ~10 full buckets were
    // tool-error echoes, refusals, the same failed `cargo test` output a dozen times, all
    // at ~0.5 (Cormac); Kimi's 15 "code" examples were her room posts, with the 120-char
    // stimulus as the whole prompt. Every bucket is an SFT target, so a genome trained on
    // them learns the failure modes and to write status posts. The Python trainer failing
    // is the only reason none was ever trained; the in-engine trainer (#4443) will succeed.
    //
    // What enters a bucket is a turn credited to a card that settles PASS (the staged
    // path below), a lesson another citizen deliberately taught, or a curated/teacher
    // corpus. The whole recorded turn as the example is card ad107e18.
    let Some(credit) = credit.filter(|c| c.is_card_linked()) else {
        continuum_base::probe!(
            class = "training.example.unverified",
            persona = %persona_name,
            prompt_chars = prompt.len() as u64,
            completion_chars = completion.len() as u64,
            "live turn with no verdict — not a training example (only a graded turn, a taught \
             lesson or a curated corpus enters a bucket)"
        );
        return;
    };
    let Some(executor) = EXECUTOR.cloned() else {
        // Expected during tests / before boot installs the executor. Named, not
        // silent — but debug, because a turn before install is normal at startup
        // and this is a side channel, not the answer path.
        tracing::debug!(
            persona = %persona_id,
            "training_producer: executor not installed yet — skipping training capture for this turn"
        );
        return;
    };

    tokio::spawn(async move {
        // CARD-LINKED TURNS STAGE. The quality floor and the domain bucket are NOT
        // decided here: `plan` applies the evidence floor and it needs a verdict, which
        // does not exist until the card settles. Deciding now would bake in a judgment
        // made without the evidence.
        // The SAME identity-bearing connection `submit_plan` builds — not a bare
        // `Connection::new(executor)`, which does not even satisfy `Transport`.
        // The identity is load-bearing here and not merely for gating: the
        // `@persona:{name}` handle these writes target resolves AS this persona,
        // so a connection without her `LocalPersona` would stage her credit
        // somewhere other than her own store.
        let conn = Connection::new(InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona_id),
            )),
        ));
        let (lived, complete) =
            read_lived(persona_id, &persona_name, &generation_receipts, Vec::new()).await;
        let lived = complete.then_some(lived);
        match stage_credit(
            &conn,
            &persona_name,
            &credit,
            generation_receipts,
            prompt,
            completion,
            lived,
            submission_id,
            replaces.as_slice(),
        )
        .await
        {
            Ok(submission_id) => continuum_base::probe!(
                class = "training.credit.staged",
                replaced_partial = replaces.is_some(),
                persona = %persona_name,
                card = %credit.card_id,
                submission = %submission_id,
                stampable = credit.claim.is_some(),
                "card-linked turn staged — it submits only if this card settles PASS"
            ),
            // Best-effort: a staging failure must
            // never touch the turn that already happened. NAMED, not swallowed
            // — a turn that failed to stage is credit that silently vanished.
            Err(e) => continuum_base::probe!(
                class = "training.credit.stage_failed",
                persona = %persona_name,
                card = %credit.card_id,
                error = %e,
                "card-linked turn could NOT be staged — this turn's credit is lost"
            ),
        }
    });
}

/// Plan a RECEIVED lesson (one another agent taught via `memory/share` #2025) for
/// the SAME training flywheel — the efferent wiring for the being-loop's third axis
/// ([[lived-and-eval-experience-are-one-stream-one-being]]). Pure: classify + package,
/// no I/O.
///
/// Unlike [`plan`], there is **no `MIN_TRAINING_QUALITY` gate**: a received lesson's
/// salience is its PROVENANCE — someone deliberately chose to teach it (`ReceivedSalience`,
/// where provenance IS the signal), not a chat-interaction score designed for live
/// `(stimulus → reply)` turns. A terse-but-important lesson must not be filtered by a
/// heuristic built for a different shape. We DO classify the domain, so the lesson buckets
/// into the gym that MEASURES its trait — the sentinel still refuses to page in a gene the
/// benchmark-lift gate (#59) didn't clear, so unmeasurable lessons train but never adopt
/// ([[fallbacks-are-illegal-fail-loud]]). `quality = 1.0`: the deliberate act of sharing.
pub fn plan_received(classifier: &DomainClassifier, topic: &str, lesson: &str) -> SubmitPlan {
    let domain = classifier.classify(&format!("{topic}\n{lesson}")).domain;
    SubmitPlan {
        trait_kind: domain,
        prompt: topic.to_string(),
        completion: lesson.to_string(),
        quality: 1.0,
        // A RECEIVED lesson is somebody else's settled experience arriving as
        // teaching. It has no card in THIS citizen's history to credit, and
        // borrowing the teacher's verdict would credit the student for an outcome
        // she did not produce — the confabulation this card exists to end.
        stamp: None,
    }
}

/// Destination replay key for one persisted lesson, recipient and training base.
/// Nested namespaces avoid delimiter collisions. Content is deliberately excluded:
/// changing an already accepted record must raise SubmissionConflict, not mint credit.
/// The version binds producer semantics too: changed names, classifier output, quality
/// or eval policy conflict under v1. A deliberate migration must define its own replay
/// policy before changing this revision; a deploy must not silently retrain old lessons.
pub fn received_submission_id(persona: Uuid, base_model: &str, record_id: &str) -> Uuid {
    let version = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"continuum:received-lesson:v1");
    let recipient = Uuid::new_v5(&version, persona.as_bytes());
    let base = Uuid::new_v5(&recipient, base_model.as_bytes());
    Uuid::new_v5(&base, record_id.as_bytes())
}

/// Preserve durable source identity through the existing received-lesson producer.
/// Received provenance is not a work verdict or a staged-credit binding.
pub fn received_submission_params(
    persona: Uuid,
    persona_name: &str,
    base_model: &str,
    classifier: &DomainClassifier,
    record: &crate::memory::MemoryRecord,
) -> serde_json::Value {
    let episode = crate::cognition::experience::ExperienceRecord::from_shared_lesson(record);
    let plan = plan_received(classifier, &episode.task.prompt, &episode.answer);
    let mut params =
        build_submit_params(persona, persona_name, base_model, &plan, RECEIVED_LESSON_SOURCE);
    params["submissionId"] = json!(received_submission_id(persona, base_model, &record.id));
    let metadata = &mut params["examples"][0]["metadata"];
    metadata["memoryRecordId"] = json!(record.id);
    metadata["memoryTimestamp"] = json!(record.timestamp);
    for field in ["shared_by", "original_author", "scope", "session"] {
        if let Some(value) = record.context.get(field) {
            metadata[field] = value.clone();
        }
    }
    params
}

#[async_trait::async_trait]
impl crate::cognition::act_observe::TurnCredit for TurnCreditCapture {
    async fn record(
        &mut self,
        acts: &[(String, Vec<crate::ai::types::ToolCall>)],
        receipts: &[crate::ai::provenance::GenerationReceipt],
        report: Option<&str>,
        final_snapshot: bool,
    ) -> bool {
        TurnCreditCapture::record(self, acts, receipts, report, final_snapshot).await
    }
}
