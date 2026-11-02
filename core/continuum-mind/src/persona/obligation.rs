//! A persona's obligations: the turns she was admitted into and has not finished (card 920619c5).
//!
//! A turn's trigger is marked read once a prompt that rendered it was sent and answered (the
//! deliberation faculty's read receipt), and after a restart `prime()` skips everything already
//! in the room. So a turn the stop cuts after that point loses its trigger for good, which is
//! why the stop waits up to `turn_ingress::TURN_SETTLE_CAP` for turns in flight (enrico 2026-10-26: four stops,
//! 300 s each, 12 of 14 turns cut anyway).
//!
//! This module is the durable half of `wake_backlog::PendingInput`'s custody: one row per
//! admitted turn, written before her context is composed and deleted when the turn ends.
//! A row still open at the next start names a turn that a stop or crash interrupted.
//!
//! Slice 1 records, closes, and reports interrupted rows at start
//! (`persona.obligation.interrupted`); it changes no turn. Re-admitting an interrupted turn
//! waits on pre-effect marks (slice 2), so a turn that already spoke or acted is never run
//! blindly a second time.
//!
//! Bookkeeping on her own store goes through the substrate connection (Owner): her own
//! identity resolves to Trusted, for which `data/update` and `data/delete` are refused.

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::cognition::conversation::IncomingMessage;
use crate::orm::OrmEntity;
use crate::persona::training_producer::{storage_ok, substrate_connection};
use crate::runtime::CommandExecutor;
use continuum_client::{ClientError, Connection, Transport};

/// One admitted turn she has not finished. The id is the turn's own id, minted at admission.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "persona_obligation")]
pub struct PersonaObligation {
    #[entity(primary_key)]
    pub id: Uuid,
    /// Whose turn. Indexed: the start-up read asks for one persona's open rows.
    #[entity(indexed)]
    pub persona_id: Uuid,
    /// The room the turn runs in.
    pub room_id: Uuid,
    /// The message that woke her, whole, so a later slice can hand it back to her as it was.
    #[entity(json)]
    pub trigger: IncomingMessage,
    /// The engram the trigger formed at admission, the root of the turn's causal chain.
    pub wake_engram: Option<Uuid>,
    /// When she was admitted into the turn, on her core's clock.
    pub admitted_at_ms: u64,
    /// Set once, by the first start that found this row open: the turn was interrupted.
    pub interrupted_at_ms: Option<u64>,
    /// Stop-drain S2: when the turn's first effect was recorded, BEFORE it happened. `None`
    /// means nothing it did had reached the world, which is what lets S3 re-admit it.
    pub effect_at_ms: Option<u64>,
    /// Every effect recorded so far, in order: `speech`, or a tool batch's (call id, tool).
    #[entity(json)]
    pub effects: Option<Vec<RecordedEffect>>,
}

/// One effect as recorded before it happened, with what recovery needs to find its receipt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedEffect {
    ToolBatch { calls: Vec<(String, String)> },
    Speech,
}

impl From<&crate::cognition::act_observe::Effect> for RecordedEffect {
    fn from(effect: &crate::cognition::act_observe::Effect) -> Self {
        match effect {
            crate::cognition::act_observe::Effect::ToolBatch { calls } => Self::ToolBatch { calls: calls.clone() },
            crate::cognition::act_observe::Effect::Speech => Self::Speech,
        }
    }
}

impl PersonaObligation {
    pub fn admitted(persona_id: Uuid, room_id: Uuid, trigger: &IncomingMessage, wake_engram: Option<Uuid>, now_ms: u64) -> Self {
        Self::admitted_as(Uuid::new_v4(), persona_id, room_id, trigger, wake_engram, now_ms)
    }

    /// The same row under a turn id already minted: a retry reconciles THIS id, never a new one,
    /// so a create that committed unseen cannot be left open beside a second row (Codex on #5492).
    pub fn admitted_as(id: Uuid, persona_id: Uuid, room_id: Uuid, trigger: &IncomingMessage, wake_engram: Option<Uuid>, now_ms: u64) -> Self {
        Self {
            id,
            persona_id,
            room_id,
            trigger: trigger.clone(),
            wake_engram,
            admitted_at_ms: now_ms,
            interrupted_at_ms: None,
            effect_at_ms: None,
            effects: None,
        }
    }
}

fn handle(persona_name: &str) -> String {
    format!("@persona:{persona_name}")
}

pub async fn ensure_storage<T: Transport>(conn: &Connection<T>, persona_name: &str) -> Result<(), ClientError> {
    let result = conn
        .commands()
        .execute_value(
            "data/ensure-schema",
            json!({ "collection": PersonaObligation::COLLECTION, "dbPath": handle(persona_name) }),
        )
        .await?;
    storage_ok(&result, "data/ensure-schema", PersonaObligation::COLLECTION)
}

pub async fn record<T: Transport>(conn: &Connection<T>, persona_name: &str, row: &PersonaObligation) -> Result<(), ClientError> {
    let result = conn
        .commands()
        .execute_value(
            "data/create",
            json!({
                "collection": PersonaObligation::COLLECTION,
                "id": row.id,
                "data": row,
                "dbPath": handle(persona_name),
            }),
        )
        .await?;
    storage_ok(&result, "data/create", PersonaObligation::COLLECTION)
}

pub async fn close<T: Transport>(conn: &Connection<T>, persona_name: &str, id: Uuid) -> Result<(), ClientError> {
    let result = conn
        .commands()
        .execute_value(
            "data/delete",
            json!({ "collection": PersonaObligation::COLLECTION, "id": id, "dbPath": handle(persona_name) }),
        )
        .await?;
    storage_ok(&result, "data/delete", PersonaObligation::COLLECTION)
}

/// Every open row of one persona, oldest first. A short page is refused rather than read as
/// complete: `total` must equal what came back.
pub async fn open<T: Transport>(conn: &Connection<T>, persona_name: &str, persona_id: Uuid) -> Result<Vec<PersonaObligation>, ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": PersonaObligation::COLLECTION,
                "dbPath": handle(persona_name),
                "filter": { "personaId": persona_id.to_string() },
                "sort": [{ "field": "admittedAtMs", "direction": "asc" }],
            }),
        )
        .await?;
    let listed: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // ORM boundary: the typed list result
    whole_page(listed.items.len(), listed.total as usize)?;
    listed
        .items
        .into_iter()
        .map(|item| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(item)?; // ORM boundary
            Ok(serde_json::from_value(record.data)?) // ORM boundary: the row as written
        })
        .collect()
}

/// A short page is refused, never read as complete: which turns a stop interrupted cannot be
/// answered from part of the rows.
pub fn whole_page(returned: usize, total: usize) -> Result<(), ClientError> {
    if returned == total {
        return Ok(());
    }
    Err(ClientError::Refused {
        command: "data/list".into(),
        outcome: None,
        reason: format!(
            "{}: {returned} of {total} open obligations returned; a partial read cannot say which turns were interrupted",
            PersonaObligation::COLLECTION
        ),
    })
}

/// One row by id, through `data/list` so an absent row is an answer (`Ok(None)`), never
/// confused with a failed read.
pub async fn find<T: Transport>(conn: &Connection<T>, persona_name: &str, id: Uuid) -> Result<Option<PersonaObligation>, ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": PersonaObligation::COLLECTION,
                "dbPath": handle(persona_name),
                "filter": { "id": id.to_string() },
            }),
        )
        .await?;
    let listed: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // ORM boundary: the typed list result
    whole_page(listed.items.len(), listed.total as usize)?;
    match listed.items.into_iter().next() {
        None => Ok(None),
        Some(item) => {
            let record: crate::orm::types::DataRecord = serde_json::from_value(item)?; // ORM boundary
            Ok(Some(serde_json::from_value(record.data)?)) // ORM boundary: the row as written
        }
    }
}

/// Whether a turn's obligation is known to be on disk. Only `Confirmed` may run: a write
/// whose reply was lost may still have committed, so "the write failed" is not "no row"
/// (Codex on S2), and a turn with an unknown row could be re-run by S3 after it acted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Confirmed(Uuid),
    /// Not known to be on disk. Carries the candidate id: the retry must reconcile this same id.
    Unconfirmed(Uuid),
}

/// Record `row`; when the write's answer is lost or refused, read the row back by its id
/// before deciding. Confirmed only when the store says the row is there.
pub async fn admit_reconciled<T: Transport>(conn: &Connection<T>, persona_name: &str, row: &PersonaObligation) -> Admission {
    if record(conn, persona_name, row).await.is_ok() {
        return Admission::Confirmed(row.id);
    }
    match find(conn, persona_name, row.id).await {
        Ok(Some(_)) => Admission::Confirmed(row.id),
        Ok(None) | Err(_) => Admission::Unconfirmed(row.id),
    }
}

/// Record an effect on a turn's row before it happens: the first sets `effectAtMs`, and each
/// appends to `effects`. Replaces the whole list with `all`, the mark's in-memory record.
pub async fn mark_effect<T: Transport>(conn: &Connection<T>, persona_name: &str, id: Uuid, first_at_ms: u64, all: &[RecordedEffect]) -> Result<(), ClientError> {
    let result = conn
        .commands()
        .execute_value(
            "data/update",
            json!({
                "collection": PersonaObligation::COLLECTION,
                "id": id,
                "data": { "effectAtMs": first_at_ms, "effects": all },
                "dbPath": handle(persona_name),
            }),
        )
        .await?;
    storage_ok(&result, "data/update", PersonaObligation::COLLECTION)
}

pub async fn mark_interrupted<T: Transport>(conn: &Connection<T>, persona_name: &str, id: Uuid, now_ms: u64) -> Result<(), ClientError> {
    let result = conn
        .commands()
        .execute_value(
            "data/update",
            json!({
                "collection": PersonaObligation::COLLECTION,
                "id": id,
                "data": { "interruptedAtMs": now_ms },
                "dbPath": handle(persona_name),
            }),
        )
        .await?;
    storage_ok(&result, "data/update", PersonaObligation::COLLECTION)
}

/// What a start found: the rows a stop or crash interrupted, and how many of them this start
/// is the first to see (the rest were reported by an earlier start and are still owed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptedReport {
    pub open: usize,
    pub newly_interrupted: usize,
}

/// The open rows no earlier start has stamped: what THIS start is the first to find
/// interrupted, so a restart's count names its own stop and not every earlier one.
pub fn newly_interrupted(rows: &[PersonaObligation]) -> impl Iterator<Item = &PersonaObligation> {
    rows.iter().filter(|row| row.interrupted_at_ms.is_none())
}

/// The loop's handle on its obligations: the persona and the substrate connection to her
/// store. Built by whoever spawns the loop, from the executor boot wired, so a loop without
/// one cannot exist. Every store failure is a probe, never a failed turn (slice 1 measures).
pub struct Obligations {
    persona_id: Uuid,
    persona_name: String,
    conn: Arc<Connection<crate::runtime::InProcessTransport>>,
    /// Consecutive store failures, shared with the turn marks: the backoff and the health
    /// probe read it, so a store that keeps failing slows her down loudly instead of spinning.
    failures: Arc<effects::Failures>,
    /// The interrupted rows whose notice this start pinned, by row id, with the room whose
    /// cycle carries it: closing a row unpins its notice, and only then (stop-drain S3).
    pinned: parking_lot::Mutex<std::collections::HashMap<Uuid, Uuid>>,
}

/// An executor over the real store's data module: what a loop's obligations write through.
/// Shared by every test that needs a CONFIRMED obligation (cards 920619c5, 68811445, afaa0a3e).
#[cfg(test)]
pub(crate) fn test_store_executor() -> Arc<CommandExecutor> {
    fn substrate_entities() -> Result<usize, String> {
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .map(|()| crate::persona::SUBSTRATE_ORM_COLLECTIONS.len())
            .map_err(|e| e.to_string())
    }
    // Registered before the first command, as boot does: the module only reads the registry.
    substrate_entities().unwrap(); // unwrap: test fixture
    let registry = Arc::new(crate::runtime::ModuleRegistry::new());
    registry.register(Arc::new(continuum_store::DataModule::new(substrate_entities)));
    let executor = Arc::new(CommandExecutor::new(registry.clone()));
    registry.install_executor_on_all(executor.clone());
    executor
}

mod effects;
mod recover;
pub use effects::ObligationMark;
pub use recover::Interrupted;

impl Obligations {
    pub fn new(executor: &Arc<CommandExecutor>, persona_id: Uuid, persona_name: &str) -> Self {
        Self {
            persona_id,
            persona_name: persona_name.to_string(),
            conn: Arc::new(substrate_connection(executor)),
            failures: Arc::default(),
            pinned: parking_lot::Mutex::default(),
        }
    }

    /// The turn's pre-effect mark (stop-drain S2), built from its confirmed obligation and
    /// carried in its burst, so it is dropped with the turn and cannot mark another's effects.
    pub fn gate_for(&self, turn: Uuid) -> crate::cognition::act_observe::EffectGate {
        crate::cognition::act_observe::EffectGate(Arc::new(ObligationMark::new(
            self.conn.clone(),
            &self.persona_name,
            turn,
            self.failures.clone(),
        )))
    }

    /// At loop start: ensure the collection, then report and stamp what the last stop left open,
    /// and hand back the open rows (oldest first) for recovery.
    /// `None` when the store could not be read: unknown, never a count of zero (Codex on #5480).
    pub async fn report_interrupted(&self, now_ms: u64) -> Option<Interrupted> {
        let conn = self.conn.as_ref();
        let rows = match ensure_storage(conn, &self.persona_name).await {
            Ok(()) => open(conn, &self.persona_name, self.persona_id).await,
            Err(error) => Err(error),
        };
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                self.fault("read", None, &error);
                return None;
            }
        };
        let mut report = InterruptedReport { open: rows.len(), newly_interrupted: 0 };
        for row in newly_interrupted(&rows) {
            report.newly_interrupted += 1;
            continuum_base::probe!(
                class = "persona.obligation.interrupted",
                persona = %self.persona_name,
                turn = %row.id,
                room_id = %row.room_id,
                trigger_event = %row.trigger.event_id,
                age_ms = now_ms.saturating_sub(row.admitted_at_ms),
                "a turn she was admitted into did not finish before the last stop: its trigger is owed"
            );
            if let Err(error) = mark_interrupted(conn, &self.persona_name, row.id, now_ms).await {
                self.fault("mark", Some(row.id), &error);
            }
        }
        Some(Interrupted { report, rows })
    }

    /// At admission, before her context is composed. `None` when unrecorded (named by a probe).
    /// At admission, before her context is composed. Only `Confirmed` lets the turn run; an
    /// unconfirmed write starts a backoff (shared with the marks, no sleep) and is probed.
    /// `candidate`: the id an earlier attempt for this same input minted. A retry reconciles it
    /// (re-create, else read back) instead of minting a second row.
    pub async fn admit(&self, room_id: Uuid, trigger: &IncomingMessage, wake_engram: Option<Uuid>, now_ms: u64, candidate: Option<Uuid>) -> Admission {
        let id = candidate.unwrap_or_else(Uuid::new_v4); // unwrap_or_else: a first attempt mints the turn's id
        // While a backoff runs the store is not tried; the input is retained with its id.
        if self.failures.backing_off().is_some() {
            return Admission::Unconfirmed(id);
        }
        let row = PersonaObligation::admitted_as(id, self.persona_id, room_id, trigger, wake_engram, now_ms);
        let started = std::time::Instant::now();
        let admission = admit_reconciled(self.conn.as_ref(), &self.persona_name, &row).await;
        self.timed("admit", started);
        match admission {
            Admission::Confirmed(_) => self.failures.succeeded(),
            Admission::Unconfirmed(_) => {
                self.failures.failed(&self.persona_name, "admit", row.id);
            }
        }
        admission
    }

    /// When the turn ends, however it ended.
    pub async fn close(&self, turn: Option<Uuid>) {
        let Some(turn) = turn else { return };
        let conn = self.conn.as_ref();
        let started = std::time::Instant::now();
        let closed = close(conn, &self.persona_name, turn).await;
        self.timed("close", started);
        match closed {
            // A row that failed to close keeps its notice: the next start finds it and re-pins.
            Ok(()) => self.unpin(turn),
            Err(error) => self.fault("close", Some(turn), &error),
        }
    }

    /// Both writes sit on every turn's path, so their cost is measured, never assumed (Fable on
    /// #5480): one probe per write, read as p50/p90 per node.
    fn timed(&self, step: &'static str, started: std::time::Instant) {
        continuum_base::probe!(
            class = "persona.obligation.write_ms",
            persona = %self.persona_name,
            step = step,
            ms = started.elapsed().as_millis() as u64,
            "an obligation write on the turn's path"
        );
    }

    fn fault(&self, step: &'static str, turn: Option<Uuid>, error: &ClientError) {
        continuum_base::probe!(
            class = "persona.obligation.store_failed",
            persona = %self.persona_name,
            step = step,
            turn = ?turn,
            error = %error,
            "an obligation write or read failed: this turn's interruption cannot be named"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(room: Uuid) -> IncomingMessage {
        IncomingMessage {
            event_id: Uuid::new_v4(),
            lamport: 7,
            peer_id: Uuid::new_v4(),
            text: "please look at the failing build".into(),
            media: Vec::new(),
            room_id: room,
            work: None,
        }
    }

    // what this catches: a start re-reporting what an earlier start already reported, so the
    // count a stop is measured by would grow with every restart instead of naming this one's.
    #[test]
    fn only_rows_no_start_has_seen_are_newly_interrupted() {
        let room = Uuid::new_v4();
        let fresh = PersonaObligation::admitted(Uuid::new_v4(), room, &message(room), None, 10);
        let seen = PersonaObligation { interrupted_at_ms: Some(20), ..fresh.clone() };
        let rows = vec![seen, fresh.clone()];
        let newly: Vec<_> = newly_interrupted(&rows).map(|row| row.id).collect();
        assert_eq!(newly, vec![fresh.id]);
    }

    // what this catches: a partial read counted as the whole set, so a stop's interrupted turns
    // would be undercounted silently instead of refused.
    #[test]
    fn a_short_page_is_refused_and_a_whole_one_passes() {
        assert!(whole_page(3, 3).is_ok());
        assert!(whole_page(0, 0).is_ok());
        assert!(whole_page(2, 3).is_err(), "two of three rows is not an answer");
    }

    // what this catches (Codex on #5480): an unreadable store reported as "nothing interrupted".
    // Unknown is None, never a zero that reads as zero loss.
    #[tokio::test]
    async fn an_unreadable_store_reports_unknown_not_zero() {
        let executor = Arc::new(CommandExecutor::new(Arc::new(crate::runtime::ModuleRegistry::new())));
        let obligations = Obligations::new(&executor, Uuid::new_v4(), "no-store");
        assert!(obligations.report_interrupted(1).await.is_none());
    }

    // what this catches: the trigger losing a field on its way through the store, so a later
    // slice would hand her back a different message than the one that woke her.
    #[test]
    fn the_trigger_round_trips_whole_through_the_row_shape() {
        let room = Uuid::new_v4();
        let row = PersonaObligation::admitted(Uuid::new_v4(), room, &message(room), Some(Uuid::new_v4()), 42);
        let stored = serde_json::to_value(&row).unwrap(); // unwrap: test literal
        assert_eq!(stored["admittedAtMs"], 42);
        let back: PersonaObligation = serde_json::from_value(stored).unwrap(); // unwrap: test literal
        assert_eq!(back, row);
    }
}
