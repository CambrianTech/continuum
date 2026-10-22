use super::*;

// ─────────────────────────── work/heartbeat ──────────────────────
/// Extend this persona's claim lease on a card during long work.
pub struct WorkHeartbeat {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkHeartbeatParams {
    /// The card id (UUID) whose claim to extend.
    pub card_id: String,
    /// The claim_id from work/claim.
    pub claim_id: String,
    /// New lease length in ms. Defaults to 30 min.
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkHeartbeatResult {
    pub extended: bool,
}

#[async_trait]
impl ActionCommand for WorkHeartbeat {
    const NAME: &'static str = "work/heartbeat";
    const ALIASES: &'static [&'static str] = &["heartbeat_task", "extend_claim"];
    // core room workflow — STAYING ON IT. Real work outruns the 30-min lease; without
    // this the card is reclaimed out from under a citizen who is still working it, which
    // we watched happen (card 0c69c0f0 through three holders in one evening). (#339)
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Extend your claim lease on a card so it doesn't go stale during long work (pass card_id + \
         claim_id; short 8-char ids from the board are accepted for both).";
    type Params = WorkHeartbeatParams;
    type Output = WorkHeartbeatResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkHeartbeatParams,
    ) -> Result<WorkHeartbeatResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let claim_id = resolve_claim_id(&airc, &p.claim_id).await?;
        let request = HeartbeatWorkClaim {
            card_id,
            claim_id,
            ttl_ms: p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS),
        };
        // THE LEASE IS RENEWED WHERE THE CARD LIVES, not where she stands (card e2aea0e6).
        // `resolve_card_id` already found the card across every board she is subscribed
        // to; the current-room verb then re-checked the CURRENT room's board and refused
        // (`WorkCardNotInCurrentRoom`) whenever she was coordinating in one room while
        // holding a card in another — Benchy's `work/heartbeat ✗` on d33e928a from the
        // fleet room, 2026-09-21. The automatic renewal (airc_runtime) already uses the
        // room-scoped form; the manual verb now does the same, and never moves her room.
        match room_holding_card(&airc, card_id).await {
            Some(room) => airc.heartbeat_work_claim_in(&room, request).await,
            None => airc.heartbeat_work_claim(request).await,
        }
        .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(WorkHeartbeatResult { extended: true })
    }
}
