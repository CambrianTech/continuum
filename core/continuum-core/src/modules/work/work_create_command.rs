use super::*;

// ─────────────────────────── work/create ─────────────────────────
/// Create a new work card on the shared board.
pub struct WorkCreate {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkCreateParams {
    /// The room whose board gets the card (id or name).
    // Required: a "current room" default put project cards in #general.
    pub room: String,
    /// owner/name; defaults to actionable card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// Card title.
    pub title: String,
    /// Card body.
    #[serde(default)]
    pub body: Option<String>,
    /// p0 (urgent) to p3 (whenever). Defaults to p2.
    #[serde(default)]
    pub priority: Option<CardPriority>,
}

// A card's priority on the wire: a closed set serde refuses anything outside, never a
// string read loosely into a default. (`//`, not `///`: a doc comment ships in her tool schema.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CardPriority {
    P0,
    P1,
    P2,
    P3,
}

impl From<CardPriority> for Priority {
    fn from(priority: CardPriority) -> Self {
        match priority {
            CardPriority::P0 => Priority::P0,
            CardPriority::P1 => Priority::P1,
            CardPriority::P2 => Priority::P2,
            CardPriority::P3 => Priority::P3,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkCreateResult {
    pub card_id: String,
}

#[async_trait]
impl ActionCommand for WorkCreate {
    const NAME: &'static str = "work/create";
    // Making cards toward an activity's goals is every participant's work (Joel,
    // 2026-09-28): a citizen who could claim and move cards but not create them could not
    // break a project into slices. The card is created under HER airc identity.
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Create a card on a room's board for known work (a slice, follow-up, review).";
    type Params = WorkCreateParams;
    type Output = WorkCreateResult;

    async fn run(&self, ctx: &Ctx, p: WorkCreateParams) -> Result<WorkCreateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        Self::create(&airc, p).await
    }
}

/// The repo of the card she is focused on, read from the board (the holds authority
/// her turns use; the roster can be empty after a reboot while the board holds work)
/// and chosen by the same focus rule. A failed read is a read failure, never "you hold
/// no card" (Codex on #4571).
pub(super) async fn held_repo(airc: &Airc) -> Result<Option<RepoId>, CommandError> {
    let held = crate::persona::airc_runtime::board_held_by(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("work/create: could not read the cards you hold: {e}")))?;
    Ok(crate::persona::work_focus::focus_actionable_card(held.iter()).map(|c| c.repo.clone()))
}

/// The repo the ROOM declares: a project activity's recipe binding carries `params.repo`
/// (project.json), bound at `activity/spawn`. `Ok(None)` = the room is not a project or
/// declares none (an org room like cambriantech has no binding at all, and that is right:
/// an org is not one repo). A read failure is a read failure.
pub(super) async fn room_repo(airc: &Airc, room: &airc_lib::Room) -> Result<Option<RepoId>, CommandError> {
    let posts = airc
        .wall_posts_in(room, Some(crate::experience::binding::RECIPE_WALL_CATEGORY))
        .await
        .map_err(|e| CommandError::Internal(format!("work/create: could not read the room's recipe binding: {e}")))?;
    let binding = crate::experience::binding::project_binding(&posts)
        .map_err(|e| CommandError::Internal(format!("work/create: the room's recipe binding is unreadable: {e}")))?;
    let Some(repo) = binding.as_ref().and_then(|b| b.declared_repo().map(str::to_string)) else {
        return Ok(None);
    };
    RepoId::new(repo.clone())
        .map(Some)
        .map_err(|e| CommandError::Internal(format!("work/create: the room declares repo {repo:?}, which is not a repo key: {e:?}")))
}

/// The distinct repos of every card she holds, in any state, for the refusal's hint.
/// Best-effort: an unreadable board names no repos; the refusal still stands.
pub(super) async fn repos_she_holds(airc: &Airc) -> Vec<String> {
    let Ok(held) = crate::persona::airc_runtime::board_held_by(airc).await else {
        return Vec::new();
    };
    let mut repos: Vec<String> = held.iter().map(|c| c.repo.to_string()).collect();
    repos.sort();
    repos.dedup();
    repos
}

/// The words when nothing supplies a repo: her one argument first, with an example
/// she can copy; the room's option second; the org-room fact last.
pub(crate) fn repo_refusal(room: &str, repos_she_holds: &[String]) -> String {
    let example = repos_she_holds.first().map(String::as_str).unwrap_or("owner/name");
    let seen = if repos_she_holds.is_empty() {
        String::new()
    } else {
        format!(" Repos of cards you hold: {}.", repos_she_holds.join(", "))
    };
    format!(
        "work/create: add repo to this call, e.g. repo=\"{example}\".{seen} Room {room:?} declares no repo \
         (a project room can, at activity/spawn --params {{\"repo\":\"owner/name\"}}; an org room never does), \
         and no card you hold is actionable here."
    )
}
