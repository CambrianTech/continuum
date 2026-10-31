use super::*;

#[async_trait]
impl ServiceModule for ChatModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "chat",
            priority: ModulePriority::Normal,
            // Both prefixes route to this module — `chat/` is the
            // future-canonical surface, `collaboration/chat/` is the
            // legacy path that TS commands still use today and will
            // keep working through this module while consumers migrate.
            command_prefixes: &["chat/", "collaboration/chat/"],
            // Chat doesn't use module event_subscriptions. The durable-
            // transcript writer (#140) rides `spawn_persist_listener` (a bus
            // receiver task, same shape as cognition::dispatch_listener)
            // because `chat:posted` is published via `publish_async_only`,
            // which feeds ONLY the broadcast channel — module subscriptions
            // are dispatched exclusively by the synchronous `publish(...,
            // registry)` path, so a subscription here would be dead wiring
            // (verified against message_bus.rs before wiring; the
            // "deferred tier" its docs mention is not built).
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        // #140: the durable-transcript writer — persists every `chat:posted`
        // projection (persona say + human chat/send, the one seam both cross).
        // Spawned here (inside the runtime) rather than at registration, which
        // runs on a non-tokio thread. `from_slot` shares this module's
        // late-bound executor, filled by install_executor_on_all before any
        // event can arrive from a live room.
        spawn_persist_listener(
            &ctx.runtime,
            ctx.bus.clone(),
            Arc::new(ChatModule::from_slot(self.executor_slot.clone())),
        );
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let _ = params;
        match command {
            // ── Migrated to the typed object registry ───────────────
            //
            // `chat/poll` + `chat/send` are now typed self-routing
            // `ActionCommand`s (see `crate::chat::commands`) that route
            // via `route_object` — the module contributes them through
            // `commands()`, capturing this module's shared executor slot.
            // Reaching this legacy path means a descriptor failed to
            // register; fail loud naming the canonical command rather
            // than silently re-handling. (Retired wholesale when
            // Registry A's trait default becomes fail-loud — #63.)
            "chat/poll" | "collaboration/chat/poll" | "chat/send"
            | "collaboration/chat/send" => Err(format!(
                "'{command}' is a migrated, typed chat command (chat/poll, chat/send) — it \
                 must route via the object registry (route_object), not the legacy \
                 handle_command path. Reaching here means its descriptor failed to register."
            )),

            // ── Staged migration stubs ──────────────────────────────
            //
            // The remaining commands still own their TS implementations
            // until their own follow-up PRs land. The kernel router
            // currently sees `chat/` claim these names (per
            // `command_prefixes` above) so the chat module's handler
            // returns a typed error with the upstream TS command name —
            // callers that need this surface go through the explicit
            // `CommandExecutor::execute_ts_json` API per
            // [[no-fallbacks-ever]] (task #219). The implicit dispatch
            // chain no longer crosses the bridge silently; the chat
            // module owns the prefix and emits a deterministic error.
            //
            // When each migration PR lands, swap the stub arm for a
            // real handler using the envelope pattern above.

            "chat/analyze" | "collaboration/chat/analyze" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),
            "chat/export" | "collaboration/chat/export" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),

            other => Err(format!(
                "{other}: not handled by chat module — known commands are chat/poll, chat/send, chat/analyze (stub), chat/export (stub)"
            )),
        }
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        // `LateBound::install` silently no-ops when the slot is already
        // filled, so a `with_executor()`-injected test executor is never
        // clobbered — the original install wins, same contract as before.
        self.executor_slot.install(executor);
    }

    /// The migrated `chat/poll` + `chat/send` commands as typed
    /// self-routing objects on the ONE registry. Both capture this
    /// module's shared late-bound executor slot (they reach `data/*` +
    /// `airc/*` through it at run time) and delegate to the canonical
    /// [`ChatModule::poll`] / [`ChatModule::send`] bodies. Their
    /// `CommandSpec` descriptors flow into `command_registry()` → the
    /// persona tool surface + grid ACL. (`chat/analyze` + `chat/export`
    /// still own their TS implementations until their own follow-up PRs
    /// — they are NOT contributed here.)
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::chat::commands::command_objects(self.executor_slot.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── helpers ──────────────────────────────────────────────────────────
/// Extract a single field from the first record in a data-module
/// `data/query` response. The data module returns
/// `{ success, data: [{ id, data: {...} }] }`, where each entry's
/// `data` is the entity payload. Returns the field as a JSON string
/// (which is the shape the TS impl threads downstream) or `None` if
/// the response shape doesn't have it.
pub(super) fn extract_first_record_field(query_result: &Value, field: &str) -> Option<String> {
    let records = query_result.get("data")?.as_array()?;
    let first = records.first()?;
    let data = first.get("data")?;
    let value = data.get(field)?;
    match value {
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Extract message payloads from a data-module `data/query` response.
/// The response shape is `{ success, data: [{ id, data: <entity> }] }`;
/// we lift each `entity` out of its `DataRecord` envelope.
pub(super) fn extract_records_as_data(query_result: &Value) -> Vec<Value> {
    query_result
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|record| record.get("data").cloned())
                .collect()
        })
        .unwrap_or_default()
}
