//! `PersonaServiceModule` — singleton Rust `ServiceModule` for persona
//! work.
//!
//! ## L0-2-prep scope
//!
//! Builds on L0-1's minimum unit (#1457): the slot machinery and
//! `enroll` now open. Each enrolled persona gets a `PersonaSlot` that
//! carries its `PersonaCognition` (the per-persona container for engine
//! + inbox + rate_limiter + sleep_state + adapter_registry + genome +
//! classifier + caches + admission state from `persona::unified`).
//!
//! `tick` is still a no-op in this slice. The TS `PersonaAutonomousLoop`
//! continues to drive the production loop. Wiring `service_once_for` to
//! actually dispatch through `full_evaluate` + `respond` lands in
//! L0-2-dispatch, gated against the slot machinery proven here.
//!
//! See [docs/grid/L0-PERSONA-COGNITION-E2E-PLAN.md] for the full
//! sequencing.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::persona::unified::PersonaCognition;
use crate::rag::RagEngine;
use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::runtime::ModuleContext;

/// Per-persona state inside the singleton service module. One slot per
/// enrolled persona; the slot owns the persona's cognition container
/// and the per-slot circuit-breaker bookkeeping.
///
/// L0-2-prep: cognition is carried; circuit breaker fields are
/// declared but not yet exercised (no dispatch happens in this slice).
/// L0-2-dispatch will read + update them inside `service_once_for`.
pub struct PersonaSlot {
    pub persona_id: Uuid,
    pub display_name: String,
    pub cognition: PersonaCognition,
    /// Unix-ms timestamp at which the per-persona circuit re-closes.
    /// 0 means the circuit is currently closed (healthy).
    pub circuit_open_until_ms: u64,
    /// Consecutive `service_once_for` failures since the last success.
    /// Trips the circuit at `CIRCUIT_BREAKER_MAX_CONSECUTIVE_FAILURES`.
    pub consecutive_failures: u32,
}

impl PersonaSlot {
    fn new(persona_id: Uuid, display_name: String, cognition: PersonaCognition) -> Self {
        Self {
            persona_id,
            display_name,
            cognition,
            circuit_open_until_ms: 0,
            consecutive_failures: 0,
        }
    }
}

/// Singleton owning persona work in-process. Replaces the TS
/// `PersonaAutonomousLoop`; the deletion of `PersonaAutonomousLoop.ts`
/// lands with L0-2-cutover.
pub struct PersonaServiceModule {
    /// Per-persona state, keyed by persona_id. One mutex over the whole
    /// map — for the 15-persona load this is fine. If a future profile
    /// ever shows contention here, split into per-slot `Mutex<Slot>`
    /// inside a dashmap or similar.
    personas: Mutex<HashMap<Uuid, PersonaSlot>>,
    /// Shared `RagEngine` used to construct each persona's cognition.
    /// Held at module level so all personas share a single retrieval
    /// substrate (corpora, indexes, caches).
    rag_engine: Arc<RagEngine>,
}

impl PersonaServiceModule {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self {
            personas: Mutex::new(HashMap::new()),
            rag_engine,
        }
    }

    /// Enroll a persona. Constructs a `PersonaCognition` for it under the
    /// module's shared `RagEngine`, stores the slot. Idempotent: enrolling
    /// the same id with a different display name updates the name; the
    /// existing cognition + circuit-breaker state are preserved (do NOT
    /// reset cognition state silently — that would be a fallback).
    pub fn enroll(&self, persona_id: Uuid, display_name: impl Into<String>) -> Result<(), String> {
        let display_name = display_name.into();
        let mut personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        if let Some(slot) = personas.get_mut(&persona_id) {
            slot.display_name = display_name;
            return Ok(());
        }
        let cognition = PersonaCognition::new(
            persona_id,
            display_name.clone(),
            Arc::clone(&self.rag_engine),
        );
        personas.insert(
            persona_id,
            PersonaSlot::new(persona_id, display_name, cognition),
        );
        Ok(())
    }

    /// Number of currently enrolled personas. Cheap; used by status.
    pub fn enrolled_count(&self) -> Result<usize, String> {
        let personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        Ok(personas.len())
    }

    /// Returns a snapshot of enrolled persona ids + display names, used
    /// by status. Allocates; for hot-path observers, iterate the map
    /// directly via your own lock.
    pub fn enrolled_snapshot(&self) -> Result<Vec<(Uuid, String)>, String> {
        let personas = self
            .personas
            .lock()
            .map_err(|_| "personas lock poisoned".to_string())?;
        Ok(personas
            .values()
            .map(|s| (s.persona_id, s.display_name.clone()))
            .collect())
    }
}

#[async_trait]
impl ServiceModule for PersonaServiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona",
            priority: ModulePriority::High,
            command_prefixes: &["persona/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(Duration::from_millis(250)),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "persona/status" => {
                let snapshot = self.enrolled_snapshot()?;
                let entries: Vec<Value> = snapshot
                    .into_iter()
                    .map(|(id, name)| json!({"persona_id": id.to_string(), "display_name": name}))
                    .collect();
                Ok(CommandResult::Json(json!({
                    "module": "persona",
                    "enrolled": entries.len(),
                    "personas": entries,
                    "scope": "L0-2-prep: enroll opens; dispatch wiring lands in L0-2-dispatch",
                })))
            }
            "persona/enroll" => {
                let persona_id_str = params
                    .get("persona_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "persona/enroll requires persona_id (string)".to_string())?;
                let persona_id = Uuid::parse_str(persona_id_str)
                    .map_err(|e| format!("persona/enroll: invalid persona_id uuid: {e}"))?;
                let display_name = params
                    .get("display_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "persona/enroll requires display_name (string)".to_string())?
                    .to_string();
                self.enroll(persona_id, display_name)?;
                Ok(CommandResult::Json(json!({
                    "enrolled": persona_id.to_string(),
                    "total": self.enrolled_count()?,
                })))
            }
            other => Err(format!("unknown persona command: {other}")),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        // L0-2-prep: enrollment is real, but no dispatch yet. The TS
        // PersonaAutonomousLoop continues to drive production. The Rust
        // dispatch lands in L0-2-dispatch with `service_once_for` and is
        // exercised in unit tests before being made the production
        // driver in L0-2-cutover.
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_module() -> PersonaServiceModule {
        PersonaServiceModule::new(Arc::new(RagEngine::new()))
    }

    #[test]
    fn config_declares_persona_prefix_and_high_priority() {
        let m = fresh_module();
        let cfg = m.config();
        assert_eq!(cfg.name, "persona");
        assert_eq!(cfg.priority, ModulePriority::High);
        assert_eq!(cfg.command_prefixes, &["persona/"]);
        assert_eq!(cfg.tick_interval, Some(Duration::from_millis(250)));
    }

    #[tokio::test]
    async fn status_with_no_enrollments_reports_zero_and_prep_scope() {
        let m = fresh_module();
        let result = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result")
        };
        assert_eq!(v["module"], "persona");
        assert_eq!(v["enrolled"], 0);
        assert_eq!(v["personas"].as_array().unwrap().len(), 0);
        assert!(v["scope"].as_str().unwrap().contains("L0-2-prep"));
    }

    #[tokio::test]
    async fn enroll_constructs_slot_and_status_reflects_it() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let result = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": persona_id.to_string(), "display_name": "Helper"}),
            )
            .await
            .expect("enroll succeeds with valid params");
        let CommandResult::Json(enroll_result) = result else {
            panic!("expected Json result")
        };
        assert_eq!(enroll_result["enrolled"], persona_id.to_string());
        assert_eq!(enroll_result["total"], 1);

        let status = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(s) = status else {
            panic!("expected Json result")
        };
        assert_eq!(s["enrolled"], 1);
        let personas = s["personas"].as_array().unwrap();
        assert_eq!(personas.len(), 1);
        assert_eq!(personas[0]["persona_id"], persona_id.to_string());
        assert_eq!(personas[0]["display_name"], "Helper");
    }

    #[tokio::test]
    async fn enroll_is_idempotent_and_updates_display_name() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "First").expect("first enroll");
        m.enroll(persona_id, "Second").expect("second enroll");
        assert_eq!(m.enrolled_count().unwrap(), 1);
        let snapshot = m.enrolled_snapshot().unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].1, "Second");
    }

    #[tokio::test]
    async fn enroll_two_distinct_personas_keeps_both() {
        let m = fresh_module();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        m.enroll(a, "Alpha").expect("enroll alpha");
        m.enroll(b, "Beta").expect("enroll beta");
        assert_eq!(m.enrolled_count().unwrap(), 2);
    }

    #[tokio::test]
    async fn enroll_missing_persona_id_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/enroll", json!({"display_name": "Helper"}))
            .await
            .expect_err("enroll without persona_id must fail");
        assert!(err.contains("persona_id"), "error names the missing param: {err}");
    }

    #[tokio::test]
    async fn enroll_missing_display_name_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": Uuid::new_v4().to_string()}),
            )
            .await
            .expect_err("enroll without display_name must fail");
        assert!(
            err.contains("display_name"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn enroll_invalid_uuid_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": "not-a-uuid", "display_name": "X"}),
            )
            .await
            .expect_err("enroll with invalid uuid must fail");
        assert!(
            err.contains("uuid") || err.contains("invalid"),
            "error names the parse failure: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_command_returns_clear_error() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/teleport", Value::Null)
            .await
            .expect_err("unknown commands must error");
        assert!(err.contains("persona/teleport"), "error names the command");
    }

    #[tokio::test]
    async fn tick_is_no_op_in_prep_slice() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper").expect("enroll");
        // tick should not error and should not affect enrolled state
        m.tick().await.expect("tick succeeds");
        assert_eq!(m.enrolled_count().unwrap(), 1);
    }
}
