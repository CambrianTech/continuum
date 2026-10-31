//! Stop-drain S3 (card afaa0a3e): what a start does with the turns the last stop cut.
//!
//! Every open row gets a notice in her notices stream (`pin_fact`, keyed by the row, so a
//! second start re-pins the same notice instead of adding one). A row that recorded no
//! effect is handed back to her as a retained input and retried on the capacity-retry path,
//! reconciling its own row. A row that recorded an effect is never retried: what was begun
//! may have happened, so she is told what, and the row closes after her next completed turn
//! on the cycle carrying the notice. Closing a row is what unpins its notice.

use uuid::Uuid;

use super::{InterruptedReport, Obligations, PersonaObligation, RecordedEffect};
use crate::cognition::conversation::IncomingMessage;
use crate::cognition::workspace::TurnAttention;
use crate::persona::wake_backlog::PendingInput;

/// What a start found open, oldest first, and how it counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Interrupted {
    pub report: InterruptedReport,
    pub rows: Vec<PersonaObligation>,
}

/// The working-memory key of a row's notice: one notice per row, however many starts see it.
pub fn notice_key(row: Uuid) -> String {
    format!("obligation:{row}")
}

/// The notice, rendered from the row: who woke her, how long ago, and what was begun.
pub fn notice(row: &PersonaObligation, now_ms: u64) -> String {
    let from = crate::persona::roster_slot::provisional_sender_name(row.trigger.peer_id);
    let age = ago(now_ms.saturating_sub(row.admitted_at_ms));
    if row.effect_at_ms.is_none() {
        return format!(
            "[interrupted] A stop cut my turn on the message from {from} ({age}) before \
             anything I did reached the world. That message is in front of me again."
        );
    }
    let begun: Vec<String> = row.effects.iter().flatten().map(begun).collect();
    format!(
        "[interrupted] A stop cut my turn on the message from {from} ({age}) after I had \
         begun: {}. Whether that finished is unknown; its receipts say, not my memory.",
        begun.join("; ")
    )
}

fn begun(effect: &RecordedEffect) -> String {
    match effect {
        RecordedEffect::Speech => "posting my reply".into(),
        RecordedEffect::ToolBatch { calls } => calls
            .iter()
            .map(|(call, tool)| format!("{tool} (call {call})"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Ballpark age: minutes are what she can act on.
fn ago(ms: u64) -> String {
    match ms / 60_000 {
        0 => "under a minute ago".into(),
        1 => "a minute ago".into(),
        minutes => format!("{minutes} minutes ago"),
    }
}

impl Obligations {
    /// At loop start: report what the last stop left open, pin a notice for each row, hand
    /// back the untouched ones and hold the uncertain ones. `None` when the store could not
    /// be read (nothing is recovered from an unknown set).
    pub(crate) async fn recover(
        &self,
        now_ms: u64,
        pending: &mut PendingInput,
        attention: impl Fn(&IncomingMessage) -> TurnAttention,
    ) -> Option<InterruptedReport> {
        let Interrupted { report, rows } = self.report_interrupted(now_ms).await?;
        for row in rows {
            let pinned = self.pin_notice(&row, now_ms);
            match row.effect_at_ms {
                None => {
                    let attention = attention(&row.trigger);
                    pending.readmit(row.trigger, row.wake_engram, attention, row.id);
                }
                Some(_) => {
                    continuum_base::probe!(
                        class = "persona.obligation.uncertain",
                        persona = %self.persona_name,
                        turn = %row.id,
                        room_id = %row.room_id,
                        effects = ?row.effects,
                        pinned = pinned,
                        "an interrupted turn had begun an effect: never retried, she is told what was begun"
                    );
                    // Unpinned (no cycle to carry it), the row stays open for the next start.
                    if pinned {
                        pending.hold_uncertain(row.id, row.room_id);
                    }
                }
            }
        }
        Some(report)
    }

    /// Pin the row's notice on the cycle her turns in its room run on. `false` when she has
    /// no such cycle: nothing could carry it.
    fn pin_notice(&self, row: &PersonaObligation, now_ms: u64) -> bool {
        let Some(cycle) = crate::cognition::persona_workspace::global().cycle_for_room(&self.persona_id, row.room_id) else {
            return false;
        };
        let Some(body) = cycle.acting() else { return false };
        body.working_memory.pin_fact(&notice_key(row.id), &notice(row, now_ms));
        self.pinned.lock().insert(row.id, row.room_id);
        true
    }

    /// The row closed: its notice leaves with it.
    pub(super) fn unpin(&self, row: Uuid) {
        let Some(room) = self.pinned.lock().remove(&row) else { return };
        if let Some(body) = crate::cognition::persona_workspace::global()
            .cycle_for_room(&self.persona_id, room)
            .as_ref()
            .and_then(|cycle| cycle.acting())
        {
            body.working_memory.unpin_fact(&notice_key(row));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::persona::obligation::{open, record, test_store_executor};

    fn trigger(room: Uuid, text: &str) -> IncomingMessage {
        IncomingMessage {
            event_id: Uuid::new_v4(),
            lamport: 3,
            peer_id: Uuid::new_v4(),
            text: text.into(),
            media: Vec::new(),
            room_id: room,
            work: None,
        }
    }

    /// A resident with hands, so her live cycle has the working memory notices are pinned in.
    fn resident(persona: Uuid) -> Arc<crate::cognition::workspace::WorkspaceCycle> {
        resident_with(persona, crate::ai::HeuristicInferenceAdapter::new())
    }

    fn resident_with(persona: Uuid, adapter: crate::ai::HeuristicInferenceAdapter) -> Arc<crate::cognition::workspace::WorkspaceCycle> {
        let cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
            persona_id: persona,
            persona_name: "Rhea".into(),
            system_prompt: "You are Rhea.".into(),
            admission: Arc::new(crate::cognition::admission_state::AdmissionState::new(Arc::new(
                crate::cognition::recall_metadata::RecallMetadataRegistry::new(),
            ))),
            adapter: Arc::new(adapter),
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: Some(Arc::new(crate::cognition::act_observe::test_support::ScriptedExecutor::new(["ok"]))),
            experience: None,
            // Roomy enough for framing + tools + a reply: a refused prompt is not what these test.
            context_window: 65_536,
            defer_recall: false,
            defer_grounding: false,
            suppress_recall: false,
            forked: false,
            read_receipt: None,
        };
        crate::cognition::persona_workspace::global().register_from_cfg(cfg).unwrap() // unwrap: test fixture
    }

    fn interrupted_notices(cycle: &crate::cognition::workspace::WorkspaceCycle) -> Vec<String> {
        let body = cycle.acting().unwrap(); // unwrap: the fixture registers her hands
        body.working_memory.pinned_facts().into_iter().filter(|fact| fact.starts_with("[interrupted]")).collect()
    }

    // what this catches (stop-drain S3): an interrupted turn lost, or re-run twice, or told twice.
    // Two untouched rows are each handed back once, with one notice each, and a second start
    // before she settles them still owes exactly those two rows with the same two notices.
    #[tokio::test]
    async fn untouched_turns_are_handed_back_once_and_a_second_start_re_pins_not_adds() {
        let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let executor = test_store_executor();
        let (persona, room, name) = (Uuid::new_v4(), Uuid::new_v4(), "recover-untouched");
        let cycle = resident(persona);
        let first = trigger(room, "first");
        let second = trigger(room, "second");
        let rows = [
            PersonaObligation::admitted(persona, room, &first, Some(Uuid::new_v4()), 10),
            PersonaObligation::admitted(persona, room, &second, None, 20),
        ];
        let seed = Obligations::new(&executor, persona, name);
        assert!(seed.report_interrupted(0).await.is_some(), "the store is readable");
        for row in &rows {
            record(seed.conn.as_ref(), name, row).await.unwrap(); // unwrap: the store under test
        }

        for start in [100, 200] {
            let obligations = Obligations::new(&executor, persona, name);
            let mut pending = PendingInput::for_persona(persona);
            let report = obligations.recover(start, &mut pending, |_| TurnAttention::default()).await;
            assert_eq!(report.map(|r| r.open), Some(2), "both turns are still owed at start {start}");
            assert_eq!(pending.obligation_for(&first), Some(rows[0].id), "handed back with its own row");
            assert_eq!(pending.wake_engram_for(&first), rows[0].wake_engram, "and its original engram");
            assert_eq!(pending.obligation_for(&second), Some(rows[1].id));
            assert!(!pending.permits_self_tick(), "an owed turn comes before her own musing");
            assert_eq!(interrupted_notices(&cycle).len(), 2, "one notice per row, however many starts");
        }

        // The retried turn finishes: its row closes and its notice leaves with it.
        let obligations = Obligations::new(&executor, persona, name);
        let mut pending = PendingInput::for_persona(persona);
        obligations.recover(300, &mut pending, |_| TurnAttention::default()).await;
        obligations.close(Some(rows[0].id)).await;
        assert_eq!(interrupted_notices(&cycle).len(), 1, "the closed row's notice is gone");
        let still = open(obligations.conn.as_ref(), name, persona).await.unwrap(); // unwrap: the store under test
        assert_eq!(still.iter().map(|r| r.id).collect::<Vec<_>>(), vec![rows[1].id]);
    }

    // what this catches (stop-drain S3): a turn that began an effect being run a second time, or
    // closed before she was told. Speech and a tool batch are never handed back; the notice names
    // what was begun (with the batch's call ids); a turn whose inference failed before any
    // generation was served does not answer it, and a restart then still owes it with the same
    // notice (Codex on #5497); the rows close only after a served generation on her live cycle.
    #[tokio::test]
    async fn begun_effects_are_never_retried_and_close_after_a_completed_turn() {
        let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let executor = test_store_executor();
        let (persona, room, name) = (Uuid::new_v4(), Uuid::new_v4(), "recover-uncertain");
        let cycle = resident(persona);
        let spoke = trigger(room, "answer me");
        let acted = trigger(room, "fix the build");
        let rows = [
            PersonaObligation {
                effect_at_ms: Some(11),
                effects: Some(vec![RecordedEffect::Speech]),
                ..PersonaObligation::admitted(persona, room, &spoke, None, 10)
            },
            PersonaObligation {
                effect_at_ms: Some(21),
                effects: Some(vec![RecordedEffect::ToolBatch { calls: vec![("toolu_1".into(), "code/edit".into())] }]),
                ..PersonaObligation::admitted(persona, room, &acted, None, 20)
            },
        ];
        let obligations = Obligations::new(&executor, persona, name);
        assert!(obligations.report_interrupted(0).await.is_some(), "the store is readable");
        for row in &rows {
            record(obligations.conn.as_ref(), name, row).await.unwrap(); // unwrap: the store under test
        }

        let mut pending = PendingInput::for_persona(persona);
        obligations.recover(60_000 + 20, &mut pending, |_| TurnAttention::default()).await;
        assert_eq!(pending.obligation_for(&spoke), None, "a turn that began speaking is never retried");
        assert_eq!(pending.obligation_for(&acted), None, "nor one that began a tool batch");
        let notices = interrupted_notices(&cycle);
        assert!(notices.iter().any(|n| n.contains("posting my reply")), "{notices:?}");
        assert!(notices.iter().any(|n| n.contains("code/edit (call toolu_1)") && n.contains("a minute ago")), "{notices:?}");

        use crate::persona::training_producer::test_support::{receipt, served_receipt};
        pending.notices_read(room, &[receipt("failed before generation")]);
        assert!(pending.take_answered().is_empty(), "a failed inference read nothing");

        // A restart before any generation was served: the rows are still owed, notices still one each.
        let restarted = Obligations::new(&executor, persona, name);
        let mut pending = PendingInput::for_persona(persona);
        let report = restarted.recover(120_000, &mut pending, |_| TurnAttention::default()).await;
        assert_eq!(report.map(|r| r.open), Some(2), "nothing was closed by the failed turn");
        assert_eq!(interrupted_notices(&cycle).len(), 2, "re-pinned, not added");
        let obligations = restarted;

        pending.notices_read(room, &[served_receipt("live", "qwen")]);
        let answered = pending.take_answered();
        assert_eq!(answered.len(), 2, "a completed turn on her live cycle answers both");
        for row in answered {
            obligations.close(Some(row)).await;
        }
        assert!(interrupted_notices(&cycle).is_empty(), "closed rows take their notices");
        assert!(open(obligations.conn.as_ref(), name, persona).await.unwrap().is_empty()); // unwrap: the store under test
    }

    // what this catches (Codex on #5497): a row answered by a generation whose prompt did not
    // carry its notice. One real turn on her live cycle after recovery: the request the engine
    // served contains the notice (with the call id), and that turn's served receipt answers it.
    #[tokio::test]
    async fn the_served_turn_that_answers_a_row_carried_its_notice() {
        use crate::cognition::act_observe::{drive_to_settle_with_input, SettleStep};
        use crate::cognition::conversation::PersonaConversation;
        let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let executor = test_store_executor();
        let (persona, room, name) = (Uuid::new_v4(), Uuid::new_v4(), "recover-served");
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let cycle = resident_with(
            persona,
            crate::ai::HeuristicInferenceAdapter::new().with_canned_response("PASS: noted").with_request_recorder(requests.clone()),
        );
        let row = PersonaObligation {
            effect_at_ms: Some(2),
            effects: Some(vec![RecordedEffect::ToolBatch { calls: vec![("toolu_9".into(), "code/shell".into())] }]),
            ..PersonaObligation::admitted(persona, room, &trigger(room, "run the tests"), None, 1)
        };
        let obligations = Obligations::new(&executor, persona, name);
        assert!(obligations.report_interrupted(0).await.is_some(), "the store is readable");
        record(obligations.conn.as_ref(), name, &row).await.unwrap(); // unwrap: the store under test
        let mut pending = PendingInput::for_persona(persona);
        obligations.recover(3, &mut pending, |_| TurnAttention::default()).await;

        let mut conversation = crate::cognition::conversation::scripted_conversation::ScriptedConversation::new();
        conversation.prime().await.unwrap(); // unwrap: test fixture
        let burst = crate::cognition::workspace::Burst::raw_in(
            crate::identity::ActivityRoom::from_uuid(room).unwrap(), // unwrap: a fresh uuid is a valid room
            "a new line in the room".into(),
        );
        let framing = crate::cognition::workspace::TurnFraming::message(TurnAttention::default());
        let settled = drive_to_settle_with_input(&cycle, burst, 8, framing, &mut conversation).await;
        let (_, _, receipts) = SettleStep::from_settled(settled);
        let served: Vec<String> = requests
            .lock()
            .unwrap() // unwrap: test fixture
            .iter()
            .map(|request| serde_json::to_string(request).unwrap()) // unwrap: test fixture
            .collect();
        assert!(!served.is_empty(), "the turn reached the engine");
        assert!(
            served.iter().all(|request| request.contains("[interrupted]") && request.contains("code/shell (call toolu_9)")),
            "every prompt this turn served carried the notice"
        );
        pending.notices_read(room, &receipts);
        assert_eq!(pending.take_answered(), vec![row.id], "and its served receipt answers the row");
    }

    // what this catches: a row whose close failed losing its notice while it stays owed. The
    // notice stays until the row is gone, so the next start re-pins the same one.
    #[tokio::test]
    async fn a_failed_close_keeps_the_notice() {
        let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let persona = Uuid::new_v4();
        let cycle = resident(persona);
        let room = Uuid::new_v4();
        let row = PersonaObligation::admitted(persona, room, &trigger(room, "owed"), None, 1);
        let no_store = Arc::new(crate::runtime::CommandExecutor::new(Arc::new(crate::runtime::ModuleRegistry::new())));
        let obligations = Obligations::new(&no_store, persona, "recover-no-store");
        assert!(obligations.pin_notice(&row, 2), "her live cycle carries it");
        obligations.close(Some(row.id)).await;
        assert_eq!(interrupted_notices(&cycle).len(), 1, "the row may still be on disk: the notice stays");
    }
}
