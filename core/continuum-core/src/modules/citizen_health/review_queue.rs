//! The boards' review queue at the tick: every room a resident stands in, each board read once,
//! counted by the board's own predicates (`review_queue`). `None` when no board could be read: an
//! unread board is not an empty queue.

use crate::cognition::health_ledger::ReviewQueue;

pub(super) async fn read() -> Option<ReviewQueue> {
    let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()?;
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = ReviewQueue::default();
    let mut read_any = false;
    for persona in registry.live_personas() {
        let Some(runtime) = registry.get(persona) else { continue };
        let airc = runtime.airc();
        let Ok(subscriptions) = airc.subscription_set().await else { continue };
        for room in subscriptions.all().map(|sub| sub.as_room()) {
            let room_id = room.channel.as_uuid();
            if !seen.insert(room_id) {
                continue;
            }
            let board = match crate::persona::room_board_source::RoomBoardReader::work_board(airc.as_ref(), Some(room_id)).await {
                Ok(board) => board,
                Err(error) => {
                    continuum_base::probe!(
                        class = "citizen.health.board_unreadable",
                        room = %room_id,
                        error = %error,
                        "a board did not read at the health tick: its review queue is not counted"
                    );
                    continue;
                }
            };
            read_any = true;
            queue.awaiting_reviewer += crate::cognition::review_queue::awaiting_reviewer(&board).count() as u64;
            queue.unsubmitted_pull_requests += crate::cognition::review_queue::unsubmitted_pull_requests(&board).count() as u64;
        }
    }
    read_any.then_some(queue)
}
