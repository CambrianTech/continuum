//! Workspace write events — "this workspace was just written" as a bus event.
//!
//! # Why this exists (card f860e59c)
//!
//! The bench board needs to know which staged SWE trees hold real, ungraded work. It
//! used to find out by asking git on a clock: every 5 s the board emitter walked every
//! citizen's `workspace/swe/*` and spawned `git rev-parse` + `git diff --binary` per
//! instance — measured at ≥4.4 git processes a second on an IDLE IntelMac, for answers
//! that had not changed since the last tick.
//!
//! The law: never poll to detect. A workspace changes because something WROTE it — a
//! `code/write`, a shell command, a git verb, a staging clone — so the write itself is
//! the event. Every write site calls [`note_written`] after the write landed, and the
//! one consumer that cares ([`crate::commands::workspace_artifacts`]) recomputes only the
//! trees an event named.
//!
//! No bus (bare unit tests, the boot window before wiring) is a silent no-op: nothing
//! is subscribed yet, so there is nobody to tell.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::runtime::message_bus::MessageBus;

/// The bus topic a workspace write publishes on. Payload:
/// `{ "personaId": <writer, may be empty>, "path": <absolute path written> }`.
pub const WORKSPACE_WRITTEN_TOPIC: &str = "workspace:written";

/// Process-global bus handle — set once at boot (ipc wiring), read by write sites that
/// hold no bus of their own (FileEngine, the detached shell task, git verbs). Same shape
/// as `SHELL_COMPLETION_BUS`.
static WORKSPACE_EVENT_BUS: OnceLock<Arc<MessageBus>> = OnceLock::new();

/// Wire the workspace-event bus (boot, once). Idempotent; later calls are no-ops.
pub fn set_workspace_event_bus(bus: Arc<MessageBus>) {
    let _ = WORKSPACE_EVENT_BUS.set(bus);
}

/// Announce that `path` (a file, or a directory a command ran in) was written by
/// `persona_id`. Call AFTER the write happened. Cheap and non-blocking: a broadcast
/// send, no I/O.
pub fn note_written(persona_id: &str, path: &Path) {
    let Some(bus) = WORKSPACE_EVENT_BUS.get() else {
        return; // no bus wired (tests, early boot) — nobody is listening yet
    };
    bus.publish_async_only(
        WORKSPACE_WRITTEN_TOPIC,
        serde_json::json!({
            "personaId": persona_id,
            "path": path.to_string_lossy(),
        }),
    );
}
