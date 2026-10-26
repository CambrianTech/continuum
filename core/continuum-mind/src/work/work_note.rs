use super::*;

// ─────────────────────────── work/note ──────────────────────────
/// Write the held card's evidence ledger — the saved state of the thought.
pub struct WorkNote {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkNoteParams {
    /// Card id (8-char short id accepted).
    pub card_id: String,
    /// Established facts: `file:range → what it showed`, `test → assertion`.
    #[serde(default)]
    pub known: Vec<String>,
    /// Competing explanations, each with its settling test.
    #[serde(default)]
    pub hypotheses: Vec<HypothesisParam>,
    /// The one unknown the answer turns on.
    #[serde(default)]
    pub unknown: String,
    /// The next test to run.
    #[serde(default)]
    pub next_test: String,
    /// Decided fix: `file:line` + intent.
    #[serde(default)]
    pub decided_fix: Option<String>,
}

/// A hypothesis as the VERB takes it — snake_case like every other work/* parameter.
/// The stored record ([`crate::experience::ledger::LedgerHypothesis`]) is a wire type in
/// camelCase; this is the adapter between the verb surface and the record, so a caller
/// never has to know the storage casing (2026-09-12: my own call sent `nextTest` and the
/// field was silently dropped).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct HypothesisParam {
    pub claim: String,
    #[serde(default)]
    pub evidence_for: Vec<String>,
    #[serde(default)]
    pub evidence_against: Vec<String>,
    /// The test that settles it.
    #[serde(default)]
    pub test: String,
}

impl From<HypothesisParam> for crate::experience::ledger::LedgerHypothesis {
    fn from(h: HypothesisParam) -> Self {
        Self {
            claim: h.claim,
            evidence_for: h.evidence_for,
            evidence_against: h.evidence_against,
            test: h.test,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkNoteResult {
    pub recorded: bool,
    pub room: String,
}

#[async_trait]
impl ActionCommand for WorkNote {
    const NAME: &'static str = "work/note";
    const ALIASES: &'static [&'static str] = &["ledger", "note_task"];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "End every work turn with this: your card's ledger — known facts (file:range → what it \
         showed), hypotheses each with the test that settles it, the one unknown, the next test. \
         The next turn opens from it.";
    type Params = WorkNoteParams;
    type Output = WorkNoteResult;

    async fn run(&self, ctx: &Ctx, p: WorkNoteParams) -> Result<WorkNoteResult, CommandError> {
        use crate::experience::ledger::LedgerStore as _;
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let Some((room, _card)) = card_in_subscribed_rooms(&airc, card_id).await else {
            return Err(CommandError::NotFound(format!(
                "card {} is on no board of a room you stand in — the ledger lives in the card's room",
                short8(card_id.as_uuid())
            )));
        };
        let ledger = crate::experience::ledger::CardLedger {
            card_id: card_id.as_uuid(),
            known: p.known,
            hypotheses: p.hypotheses.into_iter().map(Into::into).collect(),
            unknown: p.unknown,
            next_test: p.next_test,
            decided_fix: p.decided_fix.filter(|f| !f.trim().is_empty()),
            at_ms: EpochMs::now().millis(),
            by: airc.peer_id().as_uuid(),
        };
        crate::experience::ledger::WallLedgerStore::new(airc.clone())
            .write(&room, &ledger)
            .await
            .map_err(|e| CommandError::Internal(format!("ledger could not be recorded: {e}")))?;
        // Stamped for the seam as written: the handoff record carries her ledger whole
        // without a wall read at stop time (card 49b5e806).
        crate::cognition::handoff::note_ledger(airc.peer_id().as_uuid(), &ledger);
        continuum_base::probe!(
            class = "work.ledger.noted",
            card = %short8(card_id.as_uuid()),
            by = %short8(airc.peer_id().as_uuid()),
            known = ledger.known.len() as u64,
            hypotheses = ledger.hypotheses.len() as u64,
            has_next = !ledger.next_test.trim().is_empty(),
            "the card's ledger was written — the next turn opens from it"
        );
        Ok(WorkNoteResult {
            recorded: true,
            room: room.name.clone(),
        })
    }
}
