use super::*;

// ───────────────────────── work/similar · work/duplicates ─────────────────────────
//
// The board's "which of these is the same card" question, answered in the ONE
// embedding space. Measured 2026-09-16: 118 open cards, astropy-12907 open EIGHT
// times, sympy-24152 five — and no surface could show it, so the triage never
// started. Both compose the text-level primitives (`embedding/similar` /
// `embedding/groups`); the only board-specific part is what text stands for a card.
/// What a card SAYS, for similarity: its title plus the head of its body. The title
/// alone catches instance duplicates (the bench cards' titles are identical); the
/// body head catches engineering cards that name the same defect in different words.
pub(super) fn card_text(c: &WorkCard) -> String {
    const BODY_HEAD: usize = 400;
    match c.body.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        Some(body) => {
            let head: String = body.chars().take(BODY_HEAD).collect();
            format!("{}\n{}", c.title, head)
        }
        None => c.title.clone(),
    }
}

pub(super) fn card_candidates(
    cards: &[WorkCard],
    states: &[CardState],
) -> Vec<continuum_embedding::commands::similar::SimilarCandidate> {
    cards
        .iter()
        .filter(|c| states.is_empty() || states.contains(&c.state))
        .map(|c| continuum_embedding::commands::similar::SimilarCandidate {
            id: airc_core::shown_id::shown_form(c.card_id.as_uuid()),
            text: card_text(c),
        })
        .collect()
}

pub(super) fn parse_states(states: &[String]) -> Result<Vec<CardState>, CommandError> {
    states.iter().map(|s| parse_state(s)).collect()
}

pub(super) fn default_similar_k() -> usize {
    10
}

pub(super) fn default_open_states() -> Vec<String> {
    vec!["open".to_string()]
}

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkSimilarParams.ts")]
pub struct WorkSimilarParams {
    /// Room whose board to read (name or id); default: this room.
    #[serde(default)]
    #[ts(optional)]
    pub room: Option<String>,
    /// A card to find the likes of (short id or UUID). Its own text is the query and
    /// it is excluded from the hits. Give this OR `text`.
    #[serde(default)]
    pub card_id: Option<String>,
    /// Free text to find the nearest cards to — a new idea before you file it, a
    /// symptom before you go looking. Give this OR `card_id`.
    #[serde(default)]
    pub text: Option<String>,
    /// How many hits (default 10).
    #[serde(default = "default_similar_k")]
    pub k: usize,
    /// Which columns to search (default `["open"]`). Empty = every card.
    #[serde(default = "default_open_states")]
    pub states: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkSimilarHit.ts")]
pub struct WorkSimilarHit {
    pub id: String,
    pub title: String,
    pub state: String,
    pub similarity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub z: Option<f32>,
    pub significant: bool,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkSimilarResult.ts")]
pub struct WorkSimilarResult {
    /// What was compared against: the card's id, or `text`.
    pub query: String,
    /// How many cards were scored (after the state filter).
    pub searched: usize,
    pub space: String,
    pub gate: String,
    pub hits: Vec<WorkSimilarHit>,
}

pub struct WorkSimilar {
    pub registry: PersonaAircRuntimeRegistry,
    pub embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>,
}


#[async_trait]
impl ActionCommand for WorkSimilar {
    const NAME: &'static str = "work/similar";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Find the cards most like a card (`cardId`) or a text (`text`). Each hit carries its \
         similarity, `z` (standard deviations above the embedder's unrelated-pair mean) and \
         `significant`. Use it BEFORE filing a card (is this already on the board?) and while \
         triaging (is this card a duplicate?). Searches open cards by default; pass `states` \
         to widen.";
    type Params = WorkSimilarParams;
    type Output = WorkSimilarResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkSimilarParams,
    ) -> Result<WorkSimilarResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let states = parse_states(&p.states)?;
        let board = board_to_read(&airc, p.room.as_deref()).await?;
        let (query_label, query_text, exclude) = match (p.card_id.as_deref(), p.text.as_deref())
        {
            (Some(id), None) => {
                let card_id = resolve_card_id(&airc, id).await?;
                let card = board
                    .cards
                    .iter()
                    .find(|c| c.card_id == card_id)
                    .ok_or_else(|| {
                        CommandError::NotFound(format!("card {id} is not on the board"))
                    })?;
                let short = airc_core::shown_id::shown_form(card_id.as_uuid());
                (short.clone(), card_text(card), Some(short))
            }
            (None, Some(text)) if !text.trim().is_empty() => {
                ("text".to_string(), text.to_string(), None)
            }
            _ => {
                return Err(CommandError::Invalid(
                    "give exactly one of `cardId` (find cards like this one) or `text` (find \
                     cards like this text)"
                        .into(),
                ))
            }
        };
        let candidates: Vec<_> = card_candidates(&board.cards, &states)
            .into_iter()
            .filter(|c| exclude.as_deref() != Some(c.id.as_str()))
            .collect();
        let searched = candidates.len();
        let ranked = continuum_embedding::commands::similar::rank_texts(
            &self.embedder,
            &query_text,
            &candidates,
            p.k,
            0.0,
            3.0,
        )
        .await?;
        let by_id: std::collections::HashMap<String, &WorkCard> = board
            .cards
            .iter()
            .map(|c| (airc_core::shown_id::shown_form(c.card_id.as_uuid()), c))
            .collect();
        let hits = ranked
            .results
            .into_iter()
            .filter_map(|h| {
                by_id.get(&h.id).map(|c| WorkSimilarHit {
                    id: h.id,
                    title: c.title.clone(),
                    state: state_str(&c.state).to_string(),
                    similarity: h.similarity,
                    z: h.z,
                    significant: h.significant,
                })
            })
            .collect();
        Ok(WorkSimilarResult {
            query: query_label,
            searched,
            space: ranked.space,
            gate: ranked.gate,
            hits,
        })
    }
}

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkDuplicatesParams.ts")]
pub struct WorkDuplicatesParams {
    /// Room whose board to read (name or id); default: this room.
    #[serde(default)]
    #[ts(optional)]
    pub room: Option<String>,
    /// Which columns to group (default `["open"]`). Empty = every card.
    #[serde(default = "default_open_states")]
    pub states: Vec<String>,
    /// How far above the embedder's unrelated-pair mean two cards must sit to be
    /// called the same card, in standard deviations. Default 6 — deliberately
    /// tighter than `work/similar`'s 3: "related" and "the same" are different
    /// floors. Measured 2026-09-16 on the live board: at 3σ (cosine 0.57 on
    /// qwen3-embedding-0.6b) connected components CHAINED 59 unrelated engineering
    /// cards into one group at cohesion 0.45, while every true duplicate sat at
    /// 1.00. Lower it to widen; the result names the floor it joined at.
    #[serde(default = "default_duplicate_min_z")]
    pub min_z: f32,
}

pub(super) fn default_duplicate_min_z() -> f32 {
    6.0
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkDuplicateGroup.ts")]
pub struct WorkDuplicateGroup {
    /// The member to KEEP first (the group's representative), then the rest.
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    pub cohesion: f32,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/work/WorkDuplicatesResult.ts")]
pub struct WorkDuplicatesResult {
    pub searched: usize,
    /// Cards that sit in some group — how many the board can lose without losing a fact.
    pub in_groups: usize,
    /// `in_groups − group_count`: the cards that are pure excess.
    pub excess: usize,
    pub space: String,
    pub gate: String,
    pub joined_at: f32,
    pub groups: Vec<WorkDuplicateGroup>,
}

pub struct WorkDuplicates {
    pub registry: PersonaAircRuntimeRegistry,
    pub embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>,
}

#[async_trait]
impl ActionCommand for WorkDuplicates {
    const NAME: &'static str = "work/duplicates";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the groups of cards that say the same thing — the board's duplicates — strongest \
         group first, the card to KEEP first within each group. `excess` is how many cards the \
         board can close without losing a fact. A group is a CANDIDATE: confirm with the ids \
         (same instance, same defect) before closing. Groups open cards by default; pass \
         `states` to widen, `minZ` (default 6) to loosen. The triage's first move.";
    type Params = WorkDuplicatesParams;
    type Output = WorkDuplicatesResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkDuplicatesParams,
    ) -> Result<WorkDuplicatesResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let states = parse_states(&p.states)?;
        let board = board_to_read(&airc, p.room.as_deref()).await?;
        let candidates = card_candidates(&board.cards, &states);
        let searched = candidates.len();
        let grouped = continuum_embedding::commands::groups::group_texts(
            &self.embedder,
            &candidates,
            p.min_z,
            0.0, // no borrowed floor: on an uncalibrated embedder the call refuses (Cormac, #4140)
        )
        .await?;
        let title_of: std::collections::HashMap<String, String> = board
            .cards
            .iter()
            .map(|c| (airc_core::shown_id::shown_form(c.card_id.as_uuid()), c.title.clone()))
            .collect();
        let groups: Vec<WorkDuplicateGroup> = grouped
            .groups
            .into_iter()
            .map(|g| WorkDuplicateGroup {
                titles: g
                    .ids
                    .iter()
                    .map(|id| title_of.get(id).cloned().unwrap_or_default())
                    .collect(),
                ids: g.ids,
                cohesion: g.cohesion,
            })
            .collect();
        Ok(WorkDuplicatesResult {
            searched,
            in_groups: grouped.grouped,
            excess: grouped.grouped.saturating_sub(grouped.group_count),
            space: grouped.space,
            gate: grouped.gate,
            joined_at: grouped.joined_at,
            groups,
        })
    }
}

pub struct WorkGet {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkGetParams {
    /// The card id — full UUID or short id from any room you belong to.
    pub card_id: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkGetResult {
    /// Signed reviews of this card (or the linked parent for a review card).
    /// Legacy evidence_text=None means the evidence bytes were not published.
    pub reviews: Vec<submission::WorkReviewResult>,
    pub id: String,
    /// The board that supplied this receipt; never the caller's current focus.
    #[ts(type = "string")]
    pub room_id: airc_core::RoomId,
    pub room: String,
    pub is_self: bool,
    /// Availability uses the same holder projection as work/list.
    pub claimable: bool,
    pub lease: Option<String>,
    pub observed_at_ms: u64,
    pub claim_expires_at_ms: Option<u64>,
    /// Seconds until the claim lapses, read at `observed_at_ms`; absent when there is
    /// no claim or it has already lapsed. So nobody converts epoch ms by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lease_remaining_secs: Option<u64>,
    pub last_heartbeat_at_ms: Option<u64>,
    pub title: String,
    /// The card's full body — the task's requirements/spec, when authored.
    pub body: Option<String>,
    pub state: String,
    pub owner: Option<String>,
    pub claim_id: Option<String>,
    /// The card's ledger — the saved state of the thought (`work/note`) — so a reviewer,
    /// a peer taking the card over, or the human reads what is known before the diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ledger: Option<crate::experience::ledger::CardLedger>,
    /// What happens next once this card has a submission (`submission::next_after_submission`):
    /// after submitting, nothing more on the card; the grade fires on a reviewer's pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next: Option<String>,
}

/// PURE: whole seconds until a claim expiring at `expires_at_ms` lapses, seen at `now_ms`;
/// `None` with no claim or once it has lapsed (card 5d447195: Kimi converted
/// `claim_expires_at_ms` to wall time in her reasoning, twice in one day).
pub(super) fn lease_remaining_secs(expires_at_ms: Option<u64>, now_ms: u64) -> Option<u64> {
    expires_at_ms
        .filter(|&expires| expires > now_ms)
        .map(|expires| (expires - now_ms) / 1000)
}

impl WorkGet {
    /// Read only the caller's subscribed boards; a card lookup neither joins a
    /// room nor moves focus. Resolution and content use the same board view.
    pub(super) async fn read_card(
        airc: &Arc<Airc>,
        requested: &str,
        reader: Uuid,
    ) -> Result<WorkGetResult, CommandError> {
        let exact = match continuum_base::id_resolve::normalize(requested) {
            continuum_base::id_resolve::IdMatch::Full(id) => Some(WorkCardId::from_uuid(id)),
            _ => None,
        };
        let horizon = board_horizon_for_card(airc, exact)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?;
        let card_id = resolve_card_id_in_boards(&horizon, requested)?;
        let (room, board, card) = horizon
            .boards
            .iter()
            .find_map(|(room, board)| board.card(card_id).map(|c| (room, board, c)))
            .ok_or_else(|| horizon.not_found("card", requested))?;
        use crate::experience::ledger::LedgerStore as _;
        // Best effort: an unreadable ledger is an absence on the card, never a refusal of
        // the card itself.
        let ledger = crate::experience::ledger::WallLedgerStore::new(airc.clone())
            .read(room, card_id.as_uuid())
            .await
            .unwrap_or(None); // unwrap_or: an unreadable wall reads as no ledger, named by the store's own probe
        let mut receipt = Self::receipt(room, card, ledger, EpochMs::now().millis(), reader);
        let parent = card.reviews.and_then(|id| board.card(id)).unwrap_or(card);
        receipt.reviews = parent.submissions.iter()
            .flat_map(|s| board.submission_reviews_for(s.submission_id))
            .filter(|r| r.card_id == parent.card_id)
            .map(submission::WorkReviewResult::from).collect();
        // the holder's card only: a review card's next step is its verdict, not a submission
        if card.reviews.is_none() {
            let latest = crate::cognition::card_holder::latest_submission(card).map(|s| s.submission_id);
            let outcomes: Vec<_> = latest
                .map(|id| board.submission_reviews_for(id).filter(|r| r.card_id == card.card_id).map(|r| r.outcome).collect())
                .unwrap_or_default(); // unwrap_or_default: no submission, no outcomes
            receipt.next = crate::persona::work_board::submissions::next_after_submission(latest.map(|id| id.as_uuid()), &outcomes, crate::recipe::benchmark_catalog::graded_at_submit(&card.title), crate::persona::work_board::submissions::submission_review_handoff(board, card.card_id, latest));
        }
        Ok(receipt)
    }

    pub(super) fn receipt(
        room: &airc_lib::Room,
        card: &airc_work::WorkCard,
        ledger: Option<crate::experience::ledger::CardLedger>,
        now_ms: u64,
        reader: Uuid,
    ) -> WorkGetResult {
        let holder = crate::cognition::card_holder::holder(
            card,
            reader,
            now_ms,
            &crate::cognition::card_holder::NoNames,
        );
        WorkGetResult {
            reviews: Vec::new(),
            id: airc_core::shown_id::shown_form(card.card_id.as_uuid()),
            room_id: room.channel,
            room: room.name.clone(),
            is_self: holder.is_self,
            claimable: holder.claimable(),
            lease: holder.lease_word().map(str::to_string),
            observed_at_ms: now_ms,
            claim_expires_at_ms: card.claim_expires_at_ms,
            lease_remaining_secs: lease_remaining_secs(card.claim_expires_at_ms, now_ms),
            last_heartbeat_at_ms: card.last_heartbeat_at_ms,
            title: card.title.clone(),
            body: card.body.clone(),
            state: state_str(&card.state).to_string(),
            owner: card.owner.map(|o| airc_core::shown_id::shown_form(o.as_uuid())),
            claim_id: card.claim_id.map(|c| airc_core::shown_id::shown_form(c.as_uuid())),
            ledger,
            next: None,
        }
    }
}

#[async_trait]
impl ActionCommand for WorkGet {
    const NAME: &'static str = "work/get";
    const ALIASES: &'static [&'static str] = &["get_task"];
    const NATIVE: bool = true; // core room workflow — re-reading a card's spec mid-task must not require asking the room
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Read one work card in full (read-only), including signed review verdicts and published evidence_text (null on legacy hash-only reviews): title, body (the task's requirements), state, \
         board room, claimability, lease expiry/heartbeat, owner and claim id. Accepts a full or short id from any room you belong to, without \
         changing your current room. This is how you re-check a spec mid-task.";
    type Params = WorkGetParams;
    type Output = WorkGetResult;

    async fn run(&self, ctx: &Ctx, p: WorkGetParams) -> Result<WorkGetResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let reader = ctx.caller.as_ref().map(|c| c.peer_id.as_uuid()).unwrap_or_default(); // An absent caller has no self identity; never attribute the claim to the operator.
        Self::read_card(&airc, &p.card_id, reader).await
    }
}

// ─────────────────── one registry: descriptors + objects ─────────────────
crate::register_command!(WorkList);

crate::register_command!(WorkGet);

crate::register_command!(WorkSimilar);

crate::register_command!(WorkDuplicates);

crate::register_command!(WorkClaim);

crate::register_command!(WorkCreate);

crate::register_command!(WorkRelease);

crate::register_command!(WorkState);

crate::register_command!(WorkHeartbeat);

crate::register_command!(WorkNote);

/// The kanban module — holds the persona airc-runtime registry so each work tool
/// can resolve the CALLER's own airc handle and act as that persona.
pub struct WorkModule {
    pub(super) registry: PersonaAircRuntimeRegistry,
    /// The ONE process-wide embedding space, handed in at construction — `work/similar`
    /// and `work/duplicates` score card text in it. Never built here: a second embedder
    /// is a second space, and vectors from two spaces compare as noise.
    pub(super) embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>,
    /// Late-bound substrate executor (the ChatModule pattern): benchmark/dispatch
    /// composes OTHER commands — `data/list` to load a recipe row, `serving/pin`
    /// to re-home the lane — through the universal primitive instead of
    /// cross-module state threading. Installed by `install_executor_on_all`.
    pub(super) executor_slot: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>,
    >,
    /// The benchmark adapter's view of a submission (dataset base, candidate diff, grading a
    /// hand-in), injected by core so the work verbs name no benchmark concept.
    pub(super) benchmark: Arc<dyn submission::BenchmarkSubmissionAdapter>,
    /// Commands core owns that register beside the work verbs (their board is the one the
    /// cards land on); built in core with the same registry and handed in.
    pub(super) extra_commands: Vec<Arc<dyn DynCommand>>,
}

impl WorkModule {
    pub fn new(
        registry: PersonaAircRuntimeRegistry,
        embedder: Arc<dyn crate::ai::embedding::EmbeddingProvider>,
        benchmark: Arc<dyn submission::BenchmarkSubmissionAdapter>,
        extra_commands: Vec<Arc<dyn DynCommand>>,
    ) -> Self {
        Self {
            registry,
            embedder,
            benchmark,
            extra_commands,
            executor_slot: std::sync::Arc::new(crate::runtime::LateBound::new("work::executor")),
        }
    }
}

#[async_trait]
impl ServiceModule for WorkModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "work",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Wire the process-global so `work/state` can PUBLISH the card-transition event.
        // Subscribers then REACT (event-based, no poll — the system law). Idempotent:
        // `set` fails silently on a second call, which is correct for a one-core global.
        let _ = WORK_EVENT_BUS.set((ctx.bus.clone(), ctx.registry.clone()));
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // All work commands are typed objects on the one registry (see `commands`).
        Err(format!(
            "work command '{command}' is a typed object, not prefix-routed"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        let own: Vec<Arc<dyn DynCommand>> = vec![
            Arc::new(WorkList {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkGet {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkSimilar {
                registry: self.registry.clone(),
                embedder: self.embedder.clone(),
            }),
            Arc::new(WorkDuplicates {
                registry: self.registry.clone(),
                embedder: self.embedder.clone(),
            }),
            Arc::new(WorkClaim {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkCreate {
                registry: self.registry.clone(),
            }),
            Arc::new(submission::WorkSubmit {
                registry: self.registry.clone(),
                executor_slot: self.executor_slot.clone(),
                benchmark: self.benchmark.clone(),
            }),
            Arc::new(submission::WorkReview {
                registry: self.registry.clone(),
                executor_slot: self.executor_slot.clone(),
            }),
            Arc::new(submission::WorkSubmission {
                registry: self.registry.clone(),
                executor_slot: self.executor_slot.clone(),
            }),
            Arc::new(WorkRelease {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkState {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkHeartbeat {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkNote {
                registry: self.registry.clone(),
            }),
        ];
        own.into_iter().chain(self.extra_commands.iter().cloned()).collect()
    }

    fn install_executor(
        &self,
        executor: std::sync::Arc<crate::runtime::command_executor::CommandExecutor>,
    ) {
        self.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
