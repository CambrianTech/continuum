//! A persona's obligations: the turns she was admitted into and has not finished (card 920619c5).
//!
//! A turn's trigger is marked read when its context is composed (`airc_source` advances the
//! read cursor on delivery), and after a restart `prime()` skips everything already in the
//! room. So a turn the stop cuts loses its trigger for good, which is why the stop waits up
//! to `turn_ingress::TURN_SETTLE_CAP` for turns in flight (enrico 2026-10-26: four stops,
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
use crate::runtime::{CommandExecutor, LateBound};
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
}

impl PersonaObligation {
    pub fn admitted(persona_id: Uuid, room_id: Uuid, trigger: &IncomingMessage, wake_engram: Option<Uuid>, now_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            persona_id,
            room_id,
            trigger: trigger.clone(),
            wake_engram,
            admitted_at_ms: now_ms,
            interrupted_at_ms: None,
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
    if listed.items.len() != listed.total as usize {
        return Err(ClientError::Refused {
            command: "data/list".into(),
            outcome: None,
            reason: format!(
                "{}: {} of {} open obligations returned; a partial read cannot say which turns were interrupted",
                PersonaObligation::COLLECTION,
                listed.items.len(),
                listed.total
            ),
        });
    }
    listed
        .items
        .into_iter()
        .map(|item| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(item)?; // ORM boundary
            Ok(serde_json::from_value(record.data)?) // ORM boundary: the row as written
        })
        .collect()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterruptedReport {
    pub open: usize,
    pub newly_interrupted: usize,
}

/// The open rows no earlier start has stamped: what THIS start is the first to find
/// interrupted, so a restart's count names its own stop and not every earlier one.
pub fn newly_interrupted(rows: &[PersonaObligation]) -> impl Iterator<Item = &PersonaObligation> {
    rows.iter().filter(|row| row.interrupted_at_ms.is_none())
}

/// The service loop's late-bound executor, installed at boot beside the training producer's.
/// Before installation the loop runs exactly as without obligations, and says so once.
static EXECUTOR: LateBound<CommandExecutor> = LateBound::new("persona::obligation::executor");

pub fn install_executor(executor: Arc<CommandExecutor>) {
    EXECUTOR.install(executor);
}

/// The loop's handle on its obligations: the persona, and the substrate connection when the
/// executor is installed. Every failure is a probe, never a failed turn (slice 1 measures).
pub struct Obligations {
    persona_id: Uuid,
    persona_name: String,
    conn: Option<Connection<crate::runtime::InProcessTransport>>,
}

impl Obligations {
    pub fn for_persona(persona_id: Uuid, persona_name: &str) -> Self {
        let conn = EXECUTOR.cloned().map(|executor| substrate_connection(&executor));
        if conn.is_none() {
            continuum_base::probe!(
                class = "persona.obligation.unwired",
                persona = %persona_name,
                "no executor installed: this loop's turns are not recorded, so a stop cannot name what it interrupts"
            );
        }
        Self { persona_id, persona_name: persona_name.to_string(), conn }
    }

    /// At loop start: ensure the collection, then report and stamp what the last stop left open.
    pub async fn report_interrupted(&self, now_ms: u64) -> InterruptedReport {
        let Some(conn) = &self.conn else { return InterruptedReport::default() };
        let rows = match ensure_storage(conn, &self.persona_name).await {
            Ok(()) => open(conn, &self.persona_name, self.persona_id).await,
            Err(error) => Err(error),
        };
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                self.fault("read", None, &error);
                return InterruptedReport::default();
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
        report
    }

    /// At admission, before her context is composed. `None` when unrecorded (named by a probe).
    pub async fn admit(&self, room_id: Uuid, trigger: &IncomingMessage, wake_engram: Option<Uuid>, now_ms: u64) -> Option<Uuid> {
        let conn = self.conn.as_ref()?;
        let row = PersonaObligation::admitted(self.persona_id, room_id, trigger, wake_engram, now_ms);
        match record(conn, &self.persona_name, &row).await {
            Ok(()) => Some(row.id),
            Err(error) => {
                self.fault("record", Some(row.id), &error);
                None
            }
        }
    }

    /// When the turn ends, however it ended.
    pub async fn close(&self, turn: Option<Uuid>) {
        let (Some(conn), Some(turn)) = (&self.conn, turn) else { return };
        if let Err(error) = close(conn, &self.persona_name, turn).await {
            self.fault("close", Some(turn), &error);
        }
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
