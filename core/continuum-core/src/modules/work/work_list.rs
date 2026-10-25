use super::*;

// ─────────────────────────── work/list + work/get ─────────────────
//
// The READ half the board proved it needed live (#309, 2026-08-03): the
// surface was claim/create/release/state/heartbeat — write-only. Card
// content reached personas ONLY through the perception-side projection,
// so a persona that missed the render (or wanted to re-verify a spec)
// had no tool to fetch it. "I don't have access to the work card" was
// LITERALLY TRUE: the claimant of the bakery card could not re-read its
// own requirements, and the card sat at five plans / zero files. Every
// write-only surface eventually proves it needs its read half.
/// The board's displayed 8-char short form of an id — what every surface
/// shows and what the lifecycle verbs accept back.
/// The peer genuinely ON this card right now, or `None`.
///
/// The one question `work/claim`'s error path is allowed to ask before it tells
/// a citizen someone else has her card. It is NOT `card.owner`: expiry never
/// clears owner/claim_id (airc-work's projection clears them only on release or
/// on a new claim), so a card claimed once carries an owner forever. Keying off
/// the bare field relabelled EVERY refusal — settled work, wrong-room, transport
/// faults — as "held by peer X", and `claim_rejections::record` then burned that
/// fabrication into perception for ten minutes.
///
/// Named and separated from the call site so the rule can be tested without a
/// board, an airc daemon, or a running persona — the call site is where this
/// went wrong, and an inline `match` is not something a test can reach.
///
/// Uses the shared claimability-state and lease predicates. A settled card's
/// retained lease must not override its original claim refusal.
pub(super) fn live_holder(card: &airc_work::WorkCard, now_ms: u64) -> Option<airc_core::PeerId> {
    // A retained lease on settled work is not claim contention. Otherwise a
    // refused re-claim becomes AlreadyYours and select_work_claim replaces the
    // useful settled-state refusal with WorkClaimNotCurrent (Kimi, 2026-09-30).
    if crate::cognition::card_holder::refused_by_claim(card.state) {
        return None;
    }
    match crate::cognition::card_holder::hold_of(card, now_ms) {
        crate::cognition::card_holder::Hold::Held => card.owner,
        // Lapsed or unclaimed: whatever refused the claim, it was not a person.
        crate::cognition::card_holder::Hold::Lapsed
        | crate::cognition::card_holder::Hold::Unclaimed => None,
    }
}

pub(super) fn resumes_held_review(
    card: &airc_work::WorkCard,
    requested: CardState,
    caller: airc_core::PeerId,
    now_ms: u64,
) -> bool {
    requested == CardState::InProgress
        && card.state == CardState::Review
        && card.owner == Some(caller)
        && crate::cognition::card_holder::hold_of(card, now_ms)
            == crate::cognition::card_holder::Hold::Held
}

/// What a refused claim MEANS for the citizen who made it.
///
/// `live_holder` answers "is a person on this card". It does NOT answer "is that
/// person YOU" — and without that second question the refusal path told a citizen
/// a stranger held her own work. Glass-boxed live 2026-08-14: Anon (a20b3ada) held
/// 13 cards and was told `card 17531483 ... is held by peer a20b3ada` — her own
/// peer id, rendered as somebody else. She read it as contention, concluded there
/// was nothing for her to do, and went silent holding thirteen claims. Her prompt
/// was not the problem: `[active-work]` listed every card and `[Working Presence]`
/// told her a quiet room is not a stop sign. The substrate said the work was
/// someone else's, so she believed the substrate.
///
/// The caller's identity was already in scope 20 lines below (`ctx.caller`) and
/// simply never consulted for this decision. This is the [[same-bug-at-two-sites]]
/// shape — `work/list` renders holders through `card_holder`, and the claim path
/// needed the same self-vs-other distinction as a NAMED rule rather than prose.
///
/// Separated from the call site for the reason `live_holder` was: the call site is
/// where this went wrong, and an inline `if` is not something a test can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimRefusal {
    /// The live holder IS the caller — she already owns this card. Not contention,
    /// not a fault: the claim's goal ("this citizen holds this card") is already
    /// true, so the verb is satisfied and must not manufacture a rival.
    AlreadyYours,
    /// A different peer genuinely holds it right now. A normal shared-board outcome.
    HeldByPeer(Uuid),
    /// Nobody holds it; whatever refused the claim was not a person.
    Fault,
}

/// Classify a refusal from the live holder and the caller. Pure — no board, no
/// daemon, no persona.
pub(crate) fn classify_refusal(holder: Option<Uuid>, caller: Option<Uuid>) -> ClaimRefusal {
    match (holder, caller) {
        // Self-comparison FIRST: a caller who holds the card is never contention.
        (Some(h), Some(c)) if h == c => ClaimRefusal::AlreadyYours,
        (Some(h), _) => ClaimRefusal::HeldByPeer(h),
        (None, _) => ClaimRefusal::Fault,
    }
}


/// Browse the live work board (read-only).
pub struct WorkList {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkListParams {
    /// Room whose board to read (name or id); default: this room.
    #[serde(default)]
    #[ts(optional)]
    pub room: Option<String>,
    /// Optional COLUMN filter: open | claimed | in_progress | blocked | review | merged | closed.
    ///
    /// This is the card's column, NOT whether you can take it. `state="open"` means
    /// "sitting in the Open column" — it does NOT include a claimed card whose lease
    /// lapsed, which is takeable work. To ask "what can I pick up", use `claimable`.
    #[serde(default)]
    pub state: Option<String>,

    /// Optional AVAILABILITY filter — the question a citizen looking for work is
    /// actually asking. `true` = only cards you can take right now (open, or a lapsed
    /// hold); `false` = only cards someone is actively on.
    ///
    /// This axis exists because the column one lied by omission (#337, measured
    /// 2026-08-06): every board query the residents made was `work/list(state=open)`
    /// — 84 of them, all returning `{"cards":[]}` — on a board of 61 cards where 59
    /// leases had lapsed and 0 cards sat in the Open column. The grounding block in
    /// the same prompt said "59 claimable". Both were correct; the citizens spent a
    /// day reporting they had no work, and they were reading their tool correctly.
    #[serde(default)]
    pub claimable: Option<bool>,
    /// At most this many cards (default 40): claimable first, then priority, then newest.
    // Not a doc comment: this struct's docs are the tool surface a citizen pays tokens for.
    // Measured 2026-10-16 on the M5: an unbounded list answered with all 776 cards as 198k
    // chars of JSON, past her window's share, and she could read none of it (card ee003c55).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<usize>,
}

/// `work/list`'s default bound on listed cards (see [`WorkListParams::limit`]).
pub const WORK_LIST_DEFAULT_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListCard {
    // FIELD ORDER IS LOAD-BEARING HERE TOO — `claimable` MUST precede `state`/`owner`.
    //
    // serde emits in declaration order, so the reader met `"state":"claimed"` and
    // `"owner":"Anwen"` BEFORE `"claimable":true`. Live 2026-08-07, hours after the
    // lease + claimability fixes let him finally see the board, Benchy read exactly
    // this shape and reported: *"there are several tasks available on the board, but
    // they have already been claimed by others."* Every card he was looking at was
    // his to take. Two loud fields saying "someone owns this" beat one quiet field
    // saying "you can have it".
    //
    // #321 fixed the BOARD rendering so a lapsed claim reads CLAIMABLE. This is the
    // same fact on the TOOL-RESULT surface, which that fix never reached — two
    // surfaces answering one question, drifting independently (the same shape as the
    // read/write claimability split fixed in 59cbbb735). Lead with the answer to
    // "can I take this", then say who had it.
    /// 8-char short id — quote this back to work/get / work/claim / work/state.
    pub id: String,
    pub title: String,
    /// Whether she can take this card RIGHT NOW — the fact the board was hiding.
    ///
    /// A claim carries a LEASE. When it expires the holder has stopped working the card and the
    /// substrate already treats it as reclaimable (`airc work next` lists exactly these). But this
    /// projection rendered only `state` + `owner`, so an expired claim still read as
    /// `Claimed owner=Anwen` — indistinguishable from someone actively on it. A citizen doing the
    /// correct thing (read the board, don't steal a peer's card) concludes there is nothing to do.
    ///
    /// Measured 2026-08-06: 19 cards, 17 with expired leases, 2 open — and `airc work next` offered
    /// EIGHT claimable. Six citizens spent the night announcing they had nothing to work on, and
    /// they were reading the board correctly; the board was lying by omission. This is the
    /// projection half of #321.
    pub claimable: bool,
    /// Human-legible lease state for a claimed card: `expired` when the hold has lapsed (take it),
    /// `held` while someone is genuinely on it. `None` for unclaimed cards.
    pub lease: Option<String>,
    /// The card's column. Declared AFTER `claimable`/`lease` on purpose — see the
    /// field-order note above: `"claimed"` read first defeats `claimable: true` read
    /// fifth, and a lapsed claim IS takeable regardless of this column.
    pub state: String,
    /// Who holds it, when claimed: a published name, `YOU`, or "an unnamed peer". Says
    /// WHO, never "someone" ([[card-holder]]), and never an id inside the words (Joel,
    /// 2026-10-06) — read AFTER whether she can take it, because an owner on a lapsed
    /// lease is history, not an obstacle.
    pub owner: Option<String>,
    /// The holder's peer id, for reaching them (airc DM) when no name is known: the id in
    /// its own typed field, never spelled into `owner`. Absent for her own cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub owner_id: Option<Uuid>,
    /// The card's declared priority (`p0`..`p3`). The model always carried it; the
    /// list did not render it, so a 118-card board read as unordered (2026-09-16).
    pub priority: String,
    /// How long the card has been on the board, in hours. Age is the other half of
    /// order: an open P2 from July and an open P2 from this morning are not equals.
    pub age_hours: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListResult {
    // FIELD ORDER IS LOAD-BEARING — the summary MUST precede `cards`.
    //
    // serde emits in declaration order, and the receipt path truncates a large result
    // (`act_observe::truncate_chars`, capped at `fold_at.min(4096)`). `cards` on a real
    // board is far past that cap, so anything declared AFTER it is cut off in EVERY
    // receipt a citizen actually reads. Measured 2026-08-06: Anwen's prompt carried four
    // live `work/list` receipts and ZERO occurrences of `total_on_board` — the
    // self-explaining fields below were computed, serialized, and then severed by the
    // truncator, which is indistinguishable from never having built them.
    //
    // This is the SAME divisibility law the grounding board block already obeys: the
    // first delivered unit must be a complete statement, because a prefix-take keeps the
    // head and drops the tail ([[divisibility-makes-unit-order-load-bearing-the-first-unit-must-be-a-complete-statement]]).
    // Fixed there this afternoon and reintroduced here the same day; the list is the
    // divisible part, so the list is what gets cut.
    /// How many cards are on the board IN TOTAL, before any filter. Always present.
    ///
    /// An empty `cards` list is ambiguous on its own — "the board is empty" and "your
    /// filter matched nothing" are different facts and a citizen cannot tell them apart
    /// from `{"cards":[]}`. She read it as the first one, correctly by every rule of
    /// reading, and stopped looking for work. Same law the grounding sources follow: a
    /// silent zero is a hole in the glass box, so the receipt states its own scope.
    pub total_on_board: usize,

    /// How many of those cards are claimable RIGHT NOW, board-wide, regardless of the
    /// filter applied. The one number that answers "is there work here for me".
    pub claimable_now: usize,

    /// Present only when the filter emptied a non-empty board — says so plainly and
    /// names the query that would have answered her actual question. Friendly feedback
    /// at the moment of the miss, not a silent zero and not a redefinition of `state`
    /// (the column filter keeps meaning the column).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,

    /// The matched cards. LAST by design — see the field-order note at the top of this
    /// struct. This is the divisible part of the answer, so this is what a truncated
    /// receipt sheds; the counts and the note survive.
    pub cards: Vec<WorkListCard>,
}

#[async_trait]
impl ActionCommand for WorkList {
    const NAME: &'static str = "work/list";
    const ALIASES: &'static [&'static str] = &["list_tasks"];
    const NATIVE: bool = true; // core room workflow — the read half of claim_task; without it the board is write-only
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the work board's cards (read-only): short id, title, state, owner, and whether it is \
         CLAIMABLE right now: work/claim takes any card not in review/merged/closed, even one \
         someone holds (a takeover; they see it). Each line says who holds a card and how long \
         they have been silent; taking it over is your call. Use the short id with work/get for \
         full requirements. To find takeable work pass `claimable: true`: filtering \
         `state: \"open\"` (the column) misses held and lapsed cards. The result always reports \
         `total_on_board` and `claimable_now` so an empty list is never mistaken for an empty \
         board. Boards are per room (`room`).";
    type Params = WorkListParams;
    type Output = WorkListResult;

    async fn run(&self, ctx: &Ctx, p: WorkListParams) -> Result<WorkListResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let filter = p.state.as_deref().map(parse_state).transpose()?;
        let board = board_to_read(&airc, p.room.as_deref()).await?;
        let now_ms = EpochMs::now().millis();
        // Resolve every distinct owner to a published name in ONE pass, then render
        // through the SHARED holder projection (`cognition::card_holder`) — the same
        // answer the [room-kanban] grounding block and the service-loop anchor give.
        // Before this, three surfaces computed "who holds it / is the lease live"
        // separately and a teammate always came back as 8-hex, which Joel called out
        // directly: tell them WHO, or they cannot reach out.
        let me = ctx
            .caller
            .as_ref()
            .map(|c| c.peer_id.as_uuid())
            .unwrap_or_default(); // no subscriptions = empty board; the abort branch below speaks
        let mut owner_peers: Vec<airc_core::PeerId> = Vec::new();
        for c in &board.cards {
            if let Some(o) = c.owner {
                if o.as_uuid() != me && !owner_peers.contains(&o) {
                    owner_peers.push(o);
                }
            }
        }
        let names = crate::persona::room_board_source::RoomBoardReader::peer_names(
            airc.as_ref(),
            &owner_peers,
        )
        .await;
        // Render EVERY card once (holder resolution is the shared projection), then
        // filter on the rendered facts. Filtering before rendering would put the
        // availability question on the raw column again — the exact split this fixes.
        let rendered: Vec<(WorkListCard, CardState)> = board
            .cards
            .iter()
            .map(|c| {
                let holder = crate::cognition::card_holder::holder(c, me, now_ms, &names);
                (
                    WorkListCard {
                        id: short8(c.card_id.as_uuid()),
                        title: c.title.clone(),
                        state: state_str(&c.state).to_string(),
                        // The person, not the hex: a published name when known, "an
                        // unnamed peer" otherwise (its handle in owner_id), `YOU` when hers.
                        owner: holder.owner.map(|_| holder.display.clone()),
                        owner_id: holder.owner.filter(|_| !holder.is_self).map(|o| o.as_uuid()),
                        claimable: holder.claimable(),
                        lease: holder.lease_word().map(str::to_string),
                        priority: priority_str(c.priority).to_string(),
                        age_hours: now_ms.saturating_sub(c.created_at_ms) / 3_600_000,
                    },
                    c.state,
                )
            })
            .collect();

        let total_on_board = rendered.len();
        let claimable_now = rendered.iter().filter(|(r, _)| r.claimable).count();

        let mut cards: Vec<WorkListCard> = rendered
            .into_iter()
            .filter(|(_, state)| filter.map_or(true, |f| *state == f))
            .filter(|(r, _)| p.claimable.map_or(true, |want| r.claimable == want))
            .map(|(r, _)| r)
            .collect();
        let matched = cards.len();
        // claimable first, then priority (p0 before p3), then newest: the board's own order
        // is arrival, the one order a citizen looking for work never wants
        cards.sort_by(|a, b| {
            b.claimable
                .cmp(&a.claimable)
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| a.age_hours.cmp(&b.age_hours))
        });
        cards.truncate(p.limit.unwrap_or(WORK_LIST_DEFAULT_LIMIT).max(1));

        // A zero that explains itself. `{"cards":[]}` on a full board is the receipt
        // that cost the residents a day: it is indistinguishable from an empty board,
        // and they read it the only way it can be read. The filter's answer stays
        // honest — we do not quietly widen it — but the result says what it left out
        // and which query asks the question she meant. [[observability-as-substrate]]
        let note = if matched > cards.len() {
            Some(format!(
                "{} of the {matched} matching cards are listed (claimable first, then priority, then \
                 newest); pass limit for more, or narrow with state / claimable",
                cards.len()
            ))
        } else if cards.is_empty() && total_on_board > 0 {
            Some(if claimable_now > 0 {
                format!(
                    "Your filter matched 0 of {total_on_board} cards — the board is NOT empty. \
                     {claimable_now} card(s) are claimable right now; most sit in the `claimed` \
                     column with a lapsed lease, which a `state` filter does not match. Call \
                     work/list with claimable=true to see them."
                )
            } else {
                format!(
                    "Your filter matched 0 of {total_on_board} cards — the board is NOT empty, but \
                     nothing on it is claimable right now (every card is actively held or done). \
                     Call work/list with no filter to see the whole board."
                )
            })
        } else {
            None
        };

        Ok(WorkListResult {
            cards,
            total_on_board,
            claimable_now,
            note,
        })
    }
}
