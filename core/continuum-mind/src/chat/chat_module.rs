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
            // Chat doesn't use module event_subscriptions: the durable-transcript writer
            // (#140) is the inbound seam's awaited import (`line_store::import`), never a
            // bus subscription that could lag and drop a line.
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        // #140: the durable-transcript writer is the inbound seam's awaited import through this
        // module (`line_store::import`); `from_slot` shares this module's late-bound executor.
        install_line_importer(
            &ctx.runtime,
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
