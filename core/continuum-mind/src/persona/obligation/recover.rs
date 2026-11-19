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

/// The working-memory key of a room's notice: one notice per room, however many rows and starts.
pub fn notice_key(room: Uuid) -> String {
    format!("obligations:{room}")
}

/// The room's one notice, from its open rows: how many turns stops cut before anything reached
/// the world (those messages are in front of her again), and what the newest cut turn that had
/// begun an effect began, with every earlier begun turn tallied by tool. Its size follows the
/// distinct tools her cut turns used, never how many turns a run of stops cut: one notice per
/// row was 977 and 1,120 notices for Kimi and Sahar (BigMama, 2026-11-18), ~73k tokens against a
/// 72,064-token window, so no turn ran and no row, which closes after a turn, could ever close.
/// `None` for no rows.
pub fn notice(rows: &[&PersonaObligation], now_ms: u64) -> Option<String> {
    let from = |row: &PersonaObligation| crate::persona::roster_slot::provisional_sender_name(row.trigger.peer_id);
    let age = |row: &PersonaObligation| ago(now_ms.saturating_sub(row.admitted_at_ms));
    fn newest<'a>(rows: &[&&'a PersonaObligation]) -> Option<&'a PersonaObligation> {
        rows.iter().copied().copied().max_by_key(|row| (row.admitted_at_ms, row.id))
    }
    let (untouched, begun): (Vec<&&PersonaObligation>, Vec<&&PersonaObligation>) = rows.iter().partition(|row| row.effect_at_ms.is_none());
    let mut lines = Vec::new();
    if let Some(row) = newest(&untouched) {
        let count = match untouched.len() {
            1 => "my turn".to_owned(),
            n => format!("{n} of my turns"),
        };
        lines.push(format!(
            "[interrupted] A stop cut {count} before anything I did reached the world (the newest on the \
             message from {}, {}). Those messages are in front of me again.",
            from(row),
            age(row)
        ));
    }
    if let Some(row) = newest(&begun) {
        let mut line = format!(
            "[interrupted] A stop cut my turn on the message from {} ({}) after I had begun: {}. Whether \
             that finished is unknown; its receipts say, not my memory.",
            from(row),
            age(row),
            row.effects.iter().flatten().map(began).collect::<Vec<_>>().join("; ")
        );
        let earlier: Vec<&RecordedEffect> = begun.iter().filter(|other| other.id != row.id).flat_map(|other| other.effects.iter().flatten()).collect();
        if !earlier.is_empty() {
            line.push_str(&format!(" {} earlier cut turns had also begun: {}.", begun.len() - 1, tally(&earlier)));
        }
        lines.push(line);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// What one effect began: a call named by its id when its tool ran once in the batch, a run of
/// one tool by its count and its first and last call ids.
fn began(effect: &RecordedEffect) -> String {
    match effect {
        RecordedEffect::Speech => "posting my reply".into(),
        RecordedEffect::ToolBatch { calls } => {
            let mut runs: Vec<(&str, Vec<&str>)> = Vec::new();
            for (call, tool) in calls {
                match runs.iter_mut().find(|(seen, _)| *seen == tool.as_str()) {
                    Some((_, ids)) => ids.push(call),
                    None => runs.push((tool, vec![call])),
                }
            }
            runs.iter()
                .map(|(tool, ids)| match ids.as_slice() {
                    [only] => format!("{tool} (call {only})"),
                    [first, .., last] => format!("{tool} x{} (calls {first} .. {last})", ids.len()),
                    [] => (*tool).to_owned(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

/// Effects counted by what they began, in first-seen order: `code/shell x340, posting my reply x2`.
fn tally(effects: &[&RecordedEffect]) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for effect in effects {
        let names: Vec<String> = match effect {
            RecordedEffect::Speech => vec!["posting my reply".into()],
            RecordedEffect::ToolBatch { calls } => calls.iter().map(|(_, tool)| tool.clone()).collect(),
        };
        for name in names {
            match counts.iter_mut().find(|(seen, _)| *seen == name) {
                Some((_, count)) => *count += 1,
                None => counts.push((name, 1)),
            }
        }
    }
    counts.iter().map(|(name, count)| format!("{name} x{count}")).collect::<Vec<_>>().join(", ")
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
        let pinned = self.pin_notices(&rows, now_ms);
        for row in rows {
            let pinned = pinned.contains(&row.id);
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

    /// Pin each room's one notice on the cycle her turns in that room run on, each room rendered
    /// once. The ids of the rows a notice now carries; a row whose room has no such cycle is left
    /// out (nothing could carry it).
    fn pin_notices(&self, rows: &[PersonaObligation], now_ms: u64) -> std::collections::HashSet<Uuid> {
        let mut carried = std::collections::HashSet::new();
        let rooms: std::collections::BTreeSet<Uuid> = rows.iter().map(|row| row.room_id).collect();
        for room in rooms {
            let Some(cycle) = crate::cognition::persona_workspace::global().cycle_for_room(&self.persona_id, room) else { continue };
            if cycle.acting().is_none() {
                continue;
            }
            let mut pinned = self.pinned.lock();
            for row in rows.iter().filter(|row| row.room_id == room) {
                // The room's notice re-renders from what it holds on every close, so it keeps its own copy.
                let held = (row.clone(), now_ms);
                pinned.insert(row.id, held);
                carried.insert(row.id);
            }
            drop(pinned);
            self.render(room);
        }
        carried
    }

    /// The row closed: her room's notice no longer names it, and leaves with its last row.
    pub(super) fn unpin(&self, row: Uuid) {
        let Some((closed, _)) = self.pinned.lock().remove(&row) else { return };
        self.render(closed.room_id);
    }

    /// Pin `room`'s notice from the rows still pinned in it, or unpin it when none are.
    fn render(&self, room: Uuid) {
        let cycle = crate::cognition::persona_workspace::global().cycle_for_room(&self.persona_id, room);
        let Some(body) = cycle.as_ref().and_then(|cycle| cycle.acting()) else {
            return;
        };
        let pinned = self.pinned.lock();
        let here: Vec<&(PersonaObligation, u64)> = pinned.values().filter(|(row, _)| row.room_id == room).collect();
        let rows: Vec<&PersonaObligation> = here.iter().map(|(row, _)| row).collect();
        let text = here.iter().map(|(_, at)| *at).max().and_then(|now_ms| notice(&rows, now_ms));
        match text {
            Some(text) => body.working_memory.pin_fact(&notice_key(room), &text),
            None => body.working_memory.unpin_fact(&notice_key(room)),
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

    fn notice_text_of(cycle: &crate::cognition::workspace::WorkspaceCycle) -> String {
        interrupted_notices(cycle).join("\n")
    }

    // what this catches (Kimi and Sahar on BigMama, 2026-11-18: 977 and 1,120 open rows, one
    // notice each, ~73k tokens of required notices against a 72,064-token window, so no turn ran
    // and no row could close): a room's notice growing with how many turns stops cut. A hundred
    // times the rows, untouched and begun, one batch of 500 calls among them, costs no more than
    // a handful of rows, and it still counts them all and names the newest begun call.
    #[test]
    fn a_room_of_cut_turns_is_one_notice_whose_size_does_not_grow_with_them() {
        let (persona, room) = (Uuid::new_v4(), Uuid::new_v4());
        let rows = |n: u64| -> Vec<PersonaObligation> {
            (1..=n)
                .map(|at| {
                    let row = PersonaObligation::admitted(persona, room, &trigger(room, "owed"), None, at);
                    match at % 3 {
                        0 => row,
                        1 => PersonaObligation { effect_at_ms: Some(at), effects: Some(vec![RecordedEffect::Speech]), ..row },
                        _ => {
                            let calls = (0..if at == n - 1 { 500 } else { 2 }).map(|i| (format!("toolu_{at}_{i}"), "code/shell".to_owned())).collect();
                            PersonaObligation { effect_at_ms: Some(at), effects: Some(vec![RecordedEffect::ToolBatch { calls }]), ..row }
                        }
                    }
                })
                .collect()
        };
        let render = |rows: &[PersonaObligation]| notice(&rows.iter().collect::<Vec<_>>(), 10_000_000).unwrap_or_default(); // unwrap_or_default: test fixture
        let (few, many) = (render(&rows(12)), render(&rows(1_200)));
        assert!(many.chars().count() < 2 * few.chars().count(), "1,200 rows render {} chars, 12 render {}", many.len(), few.len());
        assert!(many.contains("400 of my turns"), "every untouched row is counted: {many}");
        assert!(many.contains("code/shell x500 (calls toolu_1199_0 .. toolu_1199_499)"), "the newest begun batch is named: {many}");
        assert!(many.contains("799 earlier cut turns had also begun"), "and every earlier begun row is tallied: {many}");
    }

    // what this catches (stop-drain S3): an interrupted turn lost, or re-run twice, or told twice.
    // Two untouched rows are each handed back once, under their room's one notice, and a second
    // start before she settles them still owes exactly those two rows with that same notice.
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
            assert_eq!(interrupted_notices(&cycle), vec![notice_text_of(&cycle)], "one notice for the room, however many starts");
            assert!(notice_text_of(&cycle).contains("2 of my turns"), "it counts both rows");
        }

        // The retried turn finishes: its row closes and its notice leaves with it.
        let obligations = Obligations::new(&executor, persona, name);
        let mut pending = PendingInput::for_persona(persona);
        obligations.recover(300, &mut pending, |_| TurnAttention::default()).await;
        obligations.close(Some(rows[0].id)).await;
        assert!(notice_text_of(&cycle).contains("cut my turn before"), "the room's notice now counts only the row still owed");
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
        assert_eq!(interrupted_notices(&cycle).len(), 1, "re-pinned, not added");
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
        let settled = drive_to_settle_with_input(&cycle, burst, crate::cognition::act_observe::TurnBudget::Steps(8), framing, &mut conversation).await;
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
        assert!(obligations.pin_notices(std::slice::from_ref(&row), 2).contains(&row.id), "her live cycle carries it");
        obligations.close(Some(row.id)).await;
        assert_eq!(interrupted_notices(&cycle).len(), 1, "the row may still be on disk: the notice stays");
    }
}
