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

use crate::cognition::channel_substrate::{global_channel_bookmarks, global_channel_digest_buffer};
use crate::ipc::positron_nav_source::{global_nav_focus, NAV_CHANGED};
use crate::ipc::positron_source::{AircChatFocused, CHAT_FOCUSED};
use crate::runtime::ready_buffer::ReadyBuffer;
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
        if let Some(bus) = self.bus.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            bus.publish_async_only(NAV_CHANGED, serde_json::json!({ "user_id": user }));
        }
    }

    /// Publish `chat:focused` for `room` so the chat projection refocuses its
    /// single-active-room view (the center pane follows the select), then cue a
    /// presence re-assert so the newly-focused room's roster/name re-fold
    /// (the same #118 cue a restarting projector uses). Built from the REAL
    /// [`AircChatFocused`] wire struct — emitter and consumer agree by
    /// construction, never a hand JSON. Honest no-op without a bus, same as
    /// [`Self::publish_nav_changed`].
    fn publish_chat_focused(&self, room: Uuid) {
        if let Some(bus) = self.bus.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let payload = serde_json::to_value(AircChatFocused { room_id: room })
                .expect("AircChatFocused serializes — wire-struct bug, not a runtime error");
            bus.publish_async_only(CHAT_FOCUSED, payload);
            crate::ipc::positron_presence::request_presence_resync(bus);
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
        vec![
            Arc::new(MarkRead {
                shared: self.shared.clone(),
            }),
            Arc::new(Select {
                shared: self.shared.clone(),
            }),
            Arc::new(Close {
                shared: self.shared.clone(),
            }),
        ]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/MarkReadParams.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/MarkReadResult.ts"
)]
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

/// Params for `nav/select` — switch the calling citizen's current tab to an
/// activity. The `select`/`switchTo` NavIntent verb
/// (NAVIGATION-ACROSS-MODALITIES.md §2); caller identity (`userId`) rides the
/// command envelope, same as `nav/mark-read`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/NavSelectParams.ts"
)]
pub struct NavSelectParams {
    /// The activity to switch to (an airc room id, or a citizen id for a
    /// persona-kind tab).
    #[ts(type = "string")]
    pub target: Uuid,
    /// What the target IS — the tab's activity kind (`room` default / `content`
    /// / `persona`). A persona select opens the citizen's HOME tab (the profile
    /// / brain surface) WITHOUT refocusing the chat projection — the content
    /// dispatch keys off this, the same tabs==rooms==activities semantics.
    /// `serde(default)` so a kind-less older client reads as a room switch.
    /// Schema surfaced as a string (the wire values are the lowercase
    /// `NavTargetKind` names) — schemars has no derive on the positron enum.
    #[serde(default)]
    #[schemars(with = "String")]
    pub kind: continuum_positron::nav::NavTargetKind,
}

/// Result of `nav/select` — the citizen's focus after the switch.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/NavSelectResult.ts"
)]
pub struct NavSelectResult {
    /// The current tab after the select (the target, echoed as the stored ref).
    pub current: String,
    /// The previously-focused tab the citizen left, when there was one — the
    /// room whose read cursor the markRead sibling advanced.
    #[ts(optional)]
    pub previous: Option<String>,
}

/// `nav/select` — switch the calling citizen's current tab. Writes the explicit
/// focus (the `currentTab` nav fact the reader's first-room stand-in
/// anticipated), marks the room being LEFT as read (the markRead sibling), and
/// publishes the two bus signals: `nav:changed` (the citizen's nav projector
/// re-projects, so the rail's active cell moves) and `chat:focused` (the chat
/// projection refocuses, so the center pane follows). Command in → Events out;
/// no client-local nav state anywhere ([[navigation-is-airc-state-one-semantics-many-idioms]]).
struct Select {
    shared: Arc<NavShared>,
}

#[async_trait]
impl ActionCommand for Select {
    const NAME: &'static str = "nav/select";
    const DESCRIPTION: &'static str =
        "Switch the calling citizen's current tab to a room — writes the explicit focus, marks \
         the room being left as read, and signals nav + chat projections on the bus so every \
         surface (and the persona's menu) follows.";
    type Params = NavSelectParams;
    type Output = NavSelectResult;

    async fn run(
        &self,
        ctx: &Ctx,
        params: NavSelectParams,
    ) -> Result<NavSelectResult, CommandError> {
        // WHO is navigating — the authenticated caller. No identity → fail loud
        // (a focus with no owner is meaningless), never a silent default user.
        let user = ctx.user_id.ok_or_else(|| {
            CommandError::Invalid(
                "nav/select requires an authenticated caller (user_id)".to_string(),
            )
        })?;
        use continuum_positron::nav::NavTargetKind;
        let target = params.target.to_string();
        let previous = global_nav_focus().focus(user, target.clone(), params.kind);

        // markRead sibling: a ROOM being LEFT is read. Advance its shared
        // cursor to the staged digest's tip — the lamport-domain "now", the same
        // advance a persona makes after engaging (`ChannelDigest::tip_lamport`).
        // No staged digest → no unread info exists for that room yet → honestly
        // nothing to advance (never a fabricated cursor). Cursor is monotonic,
        // so a re-select can never rewind it. A non-room previous focus (a
        // persona tab) has no read cursor — nothing to advance.
        if let Some((prev, NavTargetKind::Room)) = previous.as_ref().filter(|(p, _)| *p != target) {
            if let Ok(prev_room) = Uuid::parse_str(prev) {
                if let Some(tip) = global_channel_digest_buffer()
                    .peek(&(user, prev_room))
                    .and_then(|d| d.tip_lamport())
                {
                    global_channel_bookmarks().advance(user, prev_room, tip);
                }
            }
        }

        // Command in → Events out on the airc bus: for a ROOM select the chat
        // accumulator pins to the selected room (center pane); a PERSONA/content
        // select deliberately does NOT refocus the chat projection — the room on
        // screen stays put, the persona tab renders by its own purpose (the
        // content dispatch keys off the tab kind, never a room switch). The nav
        // projector re-projects either way (rail's active cell + the persona's
        // menu follow the explicit focus).
        if params.kind == NavTargetKind::Room {
            self.shared.publish_chat_focused(params.target);
        }
        self.shared.publish_nav_changed(user);
        Ok(NavSelectResult {
            current: target,
            previous: previous.map(|(p, _)| p),
        })
    }
}

// Self-register the nav verbs into the auto-discovered command registry — the
// declaration-site line that makes them appear in every generated surface
// (CommandMap for the thin clients, the ACL, the persona's AiSafe tool catalog:
// a persona switching its attention IS the same verb — the RAG menu idiom,
// NAVIGATION-ACROSS-MODALITIES.md §3). Dep-holding (they carry NavShared), so
// runtime construction stays in `NavModule::commands()`; this registers only
// the static descriptor.
/// Params for `nav/close` — close one of the calling citizen's open activity
/// tabs (a persona home today; content tabs when they land). Rooms don't
/// close — the room set is membership, not tab state.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/NavCloseParams.ts"
)]
pub struct NavCloseParams {
    /// The open activity to close (the tab's target ref).
    #[ts(type = "string")]
    pub target: Uuid,
}

/// Result of `nav/close` — the closed target, echoed.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/nav/NavCloseResult.ts"
)]
pub struct NavCloseResult {
    /// The tab that was closed.
    pub closed: String,
}

/// `nav/close` — remove one open activity from the citizen's tab set (the ×
/// on a persona tab). Closing the CURRENT tab clears focus, so the reader's
/// first-room stand-in takes over — the same honest pre-select state a fresh
/// citizen sees. Command in → `nav:changed` out; the projector re-projects and
/// the tab disappears on every surface at once.
struct Close {
    shared: Arc<NavShared>,
}

#[async_trait]
impl ActionCommand for Close {
    const NAME: &'static str = "nav/close";
    const DESCRIPTION: &'static str =
        "Close one of the calling citizen's open activity tabs (persona home / content). The \
         citizen's nav projection re-projects without it; closing the current tab falls back \
         to the room on screen.";
    type Params = NavCloseParams;
    type Output = NavCloseResult;

    async fn run(&self, ctx: &Ctx, params: NavCloseParams) -> Result<NavCloseResult, CommandError> {
        let user = ctx.user_id.ok_or_else(|| {
            CommandError::Invalid(
                "nav/close requires an authenticated caller (user_id)".to_string(),
            )
        })?;
        let target = params.target.to_string();
        global_nav_focus().close(user, &target);
        self.shared.publish_nav_changed(user);
        Ok(NavCloseResult { closed: target })
    }
}

crate::register_command!(MarkRead);
crate::register_command!(Select);
crate::register_command!(Close);

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

    // what this catches: the WRITE half of the nav/select verb, end-to-end
    // through the command's run(): the explicit focus lands in the shared
    // NavFocus store (the row the nav reader surfaces as `current`), the room
    // being LEFT gets its shared read cursor advanced to the staged digest's
    // tip (the markRead sibling — unread badges settle when you leave a room),
    // and the previous focus is reported. Regression here = clicking a room
    // moves nothing, or leaves phantom unread on the room you just left.
    #[tokio::test]
    async fn select_writes_focus_and_marks_the_left_room_read() {
        use crate::cognition::channel_substrate::global_channel_digest_builder;
        use airc_core::{
            Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent,
            TranscriptKind,
        };

        let module = NavModule::new();
        let cmd = Select {
            shared: module.shared.clone(),
        };
        // Unique ids so the process-global stores can't collide with parallel tests.
        let user = Uuid::from_u128(0x5e1e_c701);
        let room_a = Uuid::from_u128(0x5e1e_a00a);
        let room_b = Uuid::from_u128(0x5e1e_b00b);
        let ctx = Ctx {
            user_id: Some(user),
            ..Ctx::default()
        };

        // Stage a real digest for (user, room_a) with unread through lamport 3 —
        // what the nav reader's unread badge reads, and what "leaving the room"
        // must mark read.
        let event = TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::from_uuid(room_a),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_000_003,
            lamport: 3,
            target: MentionTarget::Room(RoomId::from_uuid(room_a)),
            headers: Headers::default(),
            body: Some(Body::text("unread in a")),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        };
        let digest =
            global_channel_digest_builder().build_from_events(user, room_a, vec![event], 0);
        global_channel_digest_buffer().publish((user, room_a), Arc::new(digest));

        // First select: no previous focus, cursor untouched.
        let first = cmd
            .run(
                &ctx,
                NavSelectParams {
                    target: room_a,
                    kind: Default::default(),
                },
            )
            .await
            .expect("select ok");
        assert_eq!(first.current, room_a.to_string());
        assert_eq!(
            first.previous, None,
            "a fresh citizen has no previous focus"
        );
        assert_eq!(
            global_nav_focus().current(user),
            Some((
                room_a.to_string(),
                continuum_positron::nav::NavTargetKind::Room
            )),
            "the explicit focus (target + kind) landed in the shared store the reader surfaces"
        );
        assert_eq!(global_channel_bookmarks().last_read(user, room_a), 0);

        // Second select: leaving room_a advances its cursor to the digest tip.
        let second = cmd
            .run(
                &ctx,
                NavSelectParams {
                    target: room_b,
                    kind: Default::default(),
                },
            )
            .await
            .expect("select ok");
        assert_eq!(second.current, room_b.to_string());
        assert_eq!(second.previous, Some(room_a.to_string()));
        assert_eq!(
            global_channel_bookmarks().last_read(user, room_a),
            3,
            "leaving room_a marked it read up to the staged digest's tip"
        );
    }

    // what this catches: the Command-in → Events-out contract — one nav/select
    // must put BOTH bus signals on the wire: `chat:focused` carrying the real
    // AircChatFocused payload (the chat projection's refocus cue) and
    // `nav:changed` for the caller (the nav projector's re-project cue). A
    // regression dropping either leaves the center pane or the rail frozen.
    #[tokio::test]
    async fn select_publishes_chat_focused_and_nav_changed() {
        let module = NavModule::new();
        let bus = Arc::new(MessageBus::new());
        module.shared.set_bus(Arc::clone(&bus));
        let mut rx = bus.receiver();
        let cmd = Select {
            shared: module.shared.clone(),
        };
        let user = Uuid::from_u128(0x5e1e_c702);
        let room = Uuid::from_u128(0x5e1e_c00c);
        let ctx = Ctx {
            user_id: Some(user),
            ..Ctx::default()
        };
        cmd.run(
            &ctx,
            NavSelectParams {
                target: room,
                kind: Default::default(),
            },
        )
        .await
        .expect("select ok");

        let mut saw_focus = false;
        let mut saw_nav = false;
        // Drain what the select published (plus the presence-resync cue).
        while let Ok(event) = rx.try_recv() {
            if event.name == CHAT_FOCUSED {
                let payload: AircChatFocused =
                    serde_json::from_value(event.payload.clone()).expect("real wire struct");
                assert_eq!(payload.room_id, room);
                saw_focus = true;
            }
            if event.name == NAV_CHANGED {
                assert_eq!(event.payload["user_id"], serde_json::json!(user));
                saw_nav = true;
            }
        }
        assert!(saw_focus, "chat:focused must reach the bus");
        assert!(saw_nav, "nav:changed must reach the bus");
    }

    // what this catches: the persona-tab purity contract — a persona-kind
    // nav/select stores the persona focus (kind and all) but must NOT publish
    // `chat:focused` (the chat projection stays pinned to the room on screen;
    // the persona home renders by tab kind, never by hijacking the room
    // accumulator). `nav:changed` still fires so the rail + tab bar follow.
    #[tokio::test]
    async fn persona_select_stores_focus_without_refocusing_chat() {
        use continuum_positron::nav::NavTargetKind;
        let module = NavModule::new();
        let bus = Arc::new(MessageBus::new());
        module.shared.set_bus(Arc::clone(&bus));
        let mut rx = bus.receiver();
        let cmd = Select {
            shared: module.shared.clone(),
        };
        let user = Uuid::from_u128(0x5e1e_c703);
        let persona = Uuid::from_u128(0x5e1e_d00d);
        let ctx = Ctx {
            user_id: Some(user),
            ..Ctx::default()
        };
        let out = cmd
            .run(
                &ctx,
                NavSelectParams {
                    target: persona,
                    kind: NavTargetKind::Persona,
                },
            )
            .await
            .expect("persona select ok");
        assert_eq!(out.current, persona.to_string());
        assert_eq!(
            global_nav_focus().current(user),
            Some((persona.to_string(), NavTargetKind::Persona)),
            "the persona focus (target + kind) landed in the shared store"
        );
        let mut saw_focus = false;
        let mut saw_nav = false;
        while let Ok(event) = rx.try_recv() {
            if event.name == CHAT_FOCUSED {
                saw_focus = true;
            }
            if event.name == NAV_CHANGED {
                saw_nav = true;
            }
        }
        assert!(
            !saw_focus,
            "a persona select must NOT refocus the chat projection — the room stays put"
        );
        assert!(
            saw_nav,
            "nav:changed still reaches the bus for the rail/tab bar"
        );
    }

    // what this catches: no caller identity → fail loud, never a silent
    // default-user focus write — the select twin of the mark-read guard
    // ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn select_without_caller_fails_loud() {
        let module = NavModule::new();
        let cmd = Select {
            shared: module.shared.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                NavSelectParams {
                    target: Uuid::from_u128(2),
                    kind: Default::default(),
                },
            )
            .await;
        assert!(
            out.is_err(),
            "no user_id must fail, not write a default focus"
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
        assert!(
            out.is_err(),
            "no user_id must fail, not write a default cursor"
        );
    }
}
