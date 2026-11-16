//! The boards a caller reads, for the hands' merge gate: every room her airc identity is subscribed
//! to, through her board index. The hands cannot hold her identity (it lives here, in the roster),
//! so boot binds this reader into the code module's state, and `code/github/pr-merge` asks it for
//! the cards that may link a pull request before it merges one (card 5a94e88e).

use std::sync::Arc;

use airc_work::BoardSnapshot;
use continuum_hands::commands::code::github::card_verdict::{BoardsUnread, CardBoards};
use continuum_hands::commands::code::github::pr_merge::CodeGithubPrMerge;

use crate::persona::operator_peer::persona_runtime;
use crate::persona::work_board::board_index::boards;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::{ActionCommand, Ctx};

/// The caller's boards, read as her: a resident as herself, an agent session as its self-peer, a
/// caller-less session as the operator (the one resolver, `operator_peer::acting_runtime`).
pub struct CallerCardBoards;

#[async_trait::async_trait]
impl CardBoards for CallerCardBoards {
    async fn boards(&self, ctx: &Ctx) -> Result<Vec<Arc<BoardSnapshot>>, BoardsUnread> {
        let registry = PersonaAircRuntimeRegistry::try_global().ok_or(BoardsUnread::NoRoster)?;
        let runtime = persona_runtime(&registry, ctx, CodeGithubPrMerge::NAME).map_err(BoardsUnread::NoIdentity)?;
        boards(runtime.airc()).await.map_err(BoardsUnread::Board)
    }
}
