//! NavModule — the command surface for a citizen's navigation state (nav slice 3).
//!
//! Holds the `MessageBus` (captured in `initialize` from [`ModuleContext`], the same
//! way `vision` captures it) so its `nav/mark-read` command can publish the
//! `NAV_CHANGED` bus signal after advancing the shared read cursor. Bus-native,
//! every environment: a Command in, an Event out on the airc bus — no DOM events,
//! no parallel event system ([[navigation-is-airc-state-one-semantics-many-idioms]]).
//!
//! `nav/mark-read` is the ACTION half of the dual-consumer atom: it advances the ONE
//! shared `(user, room)` cursor that a human's unread badge AND the persona's RAG
//! grounding both read ([[consolidate-before-concern-shared-elements-via-cache]]).

use std::any::Any;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::cognition::channel_substrate::global_channel_bookmarks;
use crate::ipc::positron_nav_source::NAV_CHANGED;
use crate::runtime::{
    CommandResult, MessageBus, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx, DynCommand};

/// Shared nav state — the bus handle, set at `initialize` and read by the
/// `nav/mark-read` command to publish `NAV_CHANGED`.
#[derive(Default)]
pub struct NavShared {
    bus: RwLock<Option<Arc<MessageBus>>>,
}

impl NavShared {
    fn set_bus(&self, bus: Arc<MessageBus>) {
        *self.bus.write().unwrap_or_else(|e| e.into_inner()) = Some(bus);
    }

    /// Publish `NAV_CHANGED` for `user` on the airc bus so the nav projector
    /// re-reads + re-projects this citizen's view. An honest no-op if the bus is
    /// not yet wired — the cursor advance already persisted to the shared store,
    /// so the write is not lost (only the live re-project is deferred).
    fn publish_nav_changed(&self, user: Uuid) {
        if let Some(bus) = self
            .bus
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            bus.publish_async_only(NAV_CHANGED, serde_json::json!({ "user_id": user }));
        }
    }
}

/// The nav command module. Owns no tick — it's a pure command surface, like
/// `resources`; its whole job is to contribute `nav/*` verbs holding the bus.
pub struct NavModule {
    shared: Arc<NavShared>,
}

impl NavModule {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(NavShared::default()),
        }
    }
}

impl Default for NavModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for NavModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "nav",
            priority: ModulePriority::Normal,
            // Typed path only — `nav/*` verbs route via `route_object` against the
            // objects `commands()` contributes. No prefix arm, no tick.
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the airc bus so mark-read can emit NAV_CHANGED — same seam vision uses.
        self.shared.set_bus(ctx.bus.clone());
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![Arc::new(MarkRead {
            shared: self.shared.clone(),
        })]
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Err(format!(
            "nav: '{command}' is a typed-registry command — it must route via route_object \
             (commands/nav/), not the legacy handle_command path"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Params for `nav/mark-read` — mark a room read up to `lamport` (the newest
/// message the caller has seen). Monotonic: the cursor never moves backward.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/nav/MarkReadParams.ts")]
pub struct MarkReadParams {
    /// The room whose read cursor to advance.
    #[ts(type = "string")]
    pub room: Uuid,
    /// Advance the cursor to this lamport (the newest message the caller has seen).
    #[ts(type = "number")]
    pub lamport: u64,
}

/// Result of `nav/mark-read` — the cursor's new value, the same `(user, room)` row
/// the human unread badge and the persona RAG grounding both read.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/nav/MarkReadResult.ts")]
pub struct MarkReadResult {
    /// The cursor value after the advance (`>=` the requested lamport, monotonic).
    #[ts(type = "number")]
    pub last_read: u64,
}

/// `nav/mark-read` — advance the shared `ChannelBookmarks` cursor for the caller +
/// publish `NAV_CHANGED` on the airc bus. Command in, Event out; the write half of
/// the dual-consumer atom.
struct MarkRead {
    shared: Arc<NavShared>,
}

#[async_trait]
impl ActionCommand for MarkRead {
    const NAME: &'static str = "nav/mark-read";
    const DESCRIPTION: &'static str =
        "Mark a room read up to a lamport for the calling citizen — advances the ONE shared \
         read cursor (the human unread badge and the persona's RAG grounding both read it) and \
         signals nav changed on the bus.";
    type Params = MarkReadParams;
    type Output = MarkReadResult;

    async fn run(&self, ctx: &Ctx, params: MarkReadParams) -> Result<MarkReadResult, CommandError> {
        // WHO is reading — the authenticated caller. No identity → fail loud (a read
        // cursor with no owner is meaningless), never a silent default user.
        let user = ctx.user_id.ok_or_else(|| {
            CommandError::Invalid(
                "nav/mark-read requires an authenticated caller (user_id)".to_string(),
            )
        })?;
        let bookmarks = global_channel_bookmarks();
        bookmarks.advance(user, params.room, params.lamport);
        // Command in → Event out on the airc bus (not a DOM event): the projector
        // re-reads + re-projects, so every surface + the persona see the new cursor.
        self.shared.publish_nav_changed(user);
        Ok(MarkReadResult {
            last_read: bookmarks.last_read(user, params.room),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: mark-read advances the REAL shared cursor for the caller
    // (the dual-consumer row) and reports the monotonic new value — the write half of
    // the atom, exercised end-to-end through the command's run().
    #[tokio::test]
    async fn mark_read_advances_the_shared_cursor_for_the_caller() {
        let module = NavModule::new();
        let cmd = MarkRead {
            shared: module.shared.clone(),
        };
        // Unique ids so the process-global bookmark store can't collide with a
        // parallel test advancing a different (user, room).
        let user = Uuid::from_u128(0xda7a);
        let room = Uuid::from_u128(0xbeef);
        let ctx = Ctx {
            user_id: Some(user),
            ..Ctx::default()
        };
        let out = cmd
            .run(&ctx, MarkReadParams { room, lamport: 77 })
            .await
            .expect("mark-read ok");
        assert_eq!(out.last_read, 77);
        // Monotonic: a lower lamport does not move it backward.
        let out2 = cmd
            .run(&ctx, MarkReadParams { room, lamport: 10 })
            .await
            .expect("ok");
        assert_eq!(out2.last_read, 77, "cursor is monotonic — never rewinds");
        assert_eq!(
            global_channel_bookmarks().last_read(user, room),
            77,
            "the shared store carries the advance (the row the persona grounding reads)"
        );
    }

    // what this catches: no caller identity → fail loud, never a silent default-user
    // cursor write ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn mark_read_without_caller_fails_loud() {
        let module = NavModule::new();
        let cmd = MarkRead {
            shared: module.shared.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                MarkReadParams {
                    room: Uuid::from_u128(1),
                    lamport: 5,
                },
            )
            .await;
        assert!(out.is_err(), "no user_id must fail, not write a default cursor");
    }
}
