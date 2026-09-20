# Live-call positron controls — one truth, every interface

**Status:** designed + fully diagnosed 2026-08-07, not yet built. Everything below was
verified against the running tree, not inferred. Task #58.

## The layering (Joel, 2026-08-07)

- **airc** carries webrtc / streams. Transport.
- **positron** renders. One `ViewState`, N renderers.
- **continuum** holds the recipe and the live orchestration.
- **Node and Python are NEVER core — they bottleneck.** Node is presentation only, and only
  for the *optional* web desktop. There are iOS, Android and TUI clients too.

## What already exists and is wired

Verified, so nobody re-derives it:

| Piece | Where | State |
|---|---|---|
| airc transport | `airc-transport` crate | exists |
| call room ↔ airc room | `live/transport/call_room.rs` | uses `airc_core::RoomId`, `airc_lib::derive_room_id` |
| "the call room IS the airc room" | `live/transport/call_server.rs:503` | **enforced in Rust, fail-loud** (#193 slice B) |
| call lifecycle | `call_server.rs` | `join_call:517`, `join_call_with_model:587`, `leave_call:667` |
| orchestrator | `live/session/orchestrator.rs` | `session_participants`, `session_contexts` |
| VoiceService | constructed `ipc/mod.rs:1581` | serves `voice/register-session` |
| cognition | `live/session/{cognitive_animation,sentiment}.rs` | exists |

## The two defects

### 1. The wire that was never run

`CallServer` knows a call started, holds the `call_id` (= airc `RoomId`) and the joining
participant. Its references to `orchestrator` / `voice_service` / `register_session`: **zero.**

So every path into `register_session` is a **client calling in** — `ffi/mod.rs:191` for native
clients, the IPC command for the Node desktop, plus tests. Registration is client-driven, so
the orchestration logic (participant mapping, `AIAudioBridge.joinCall`) only ever lived in
`legacy/src/system/voice/server/VoiceOrchestratorRustBridge.ts` — 162 lines of Node,
presentation tier, now retired.

**Consequence:** iOS / Android / TUI citizens are not "buggy on voice," they are
*structurally voiceless* — the bootstrap cannot fire unless each client reimplements core
orchestration. The legacy file names the user-visible symptom in its own comment:

> "Without this, `isInCall()` returns false and AI responses are silently dropped."

That is the reported "personas are static — not animating, talking, hearing, seeing."

### 2. No render layer

positron has **eight** sources — chat, roster, kanban, nav, serving, wall, foundry, metrics —
and **none for live/call**. It is the only subsystem that never got one-truth-N-renderers.

## Design

### `core/continuum-positron/src/live.rs` → `LiveCallViewState`

Follows `ServingViewState` exactly: `positron_core::ViewState`, a `KIND` const (open
self-registration, **no central enum**), ts-rs export to
`protocol/typescript/positron/LiveCallViewState.ts` so every renderer gets the type free.

Project **both sides**, because the divergence between them IS the bug:

- from `CallManager`: the live calls (`calls: RwLock<HashMap<String, Arc<RwLock<Call>>>>`)
- from `VoiceOrchestrator`: the registered sessions (`session_participants`)

Per AI participant carry **`audio_registered: bool`**. Today `isInCall()` returning false
silently drops the response — invisible. As a ViewState field it becomes a *rendered fact* in
web + iOS + Android + TUI simultaneously. A call that is live with **no** registration renders
as exactly that, which makes defect 1 self-evident in every client instead of a mystery.

Honest-absence discipline is already the house style here and applies:
`ServingViewState.header` is `Option` — *"None before the daemon has ever published — honest
unknown, never a fabricated ready"*; `series` is empty — *"absence over fabrication."*

### `core/continuum-core/src/ipc/positron_live_source.rs`

Mirror `positron_serving_source.rs`: `spawn_live_emitter(rt, substrate)` → own task,
`tokio::time::interval`, `StateBuilder::standalone()` (sole writer of its kind), build the
view, `substrate.store(builder.session(view))`. Matches CONCURRENCY-STYLE-GUIDE.

### The wire

`CallServer::join_call` / `join_call_with_model` / `leave_call` notify the orchestrator, so
registration is **core-driven**. All four clients get voice with no per-client logic and the
conventions live in one place.

## Build order (7 pieces)

1. `VoiceOrchestrator` read accessor for registered sessions (none exists)
2. `CallManager` read accessor for active calls (none exists)
3. `continuum-positron/src/live.rs` — `LiveCallViewState` + participant views
4. `ipc/positron_live_source.rs` — the emitter
5. module registration (`continuum-positron/src/lib.rs`, `ipc/mod.rs`)
6. spawn beside the other emitters
7. tests — the load-bearing one: **a live call with zero registrations must render as live +
   unregistered**, never as "no call". That is the regression that keeps defect 1 visible.

Do 1–6 as one slice: a `ViewState` with no source, or a source nobody spawns, is the
silently-unwired anti-pattern this whole subsystem is already suffering from.

## Two wrong framings, recorded so they are not re-derived

- *"`voice/register-session` has zero Rust callers"* — accurate observation, no cause.
- *"A Rust IPC command called from TypeScript is the documented three-layer architecture, so
  port the legacy TS bridge"* — **wrong, and worse.** CLAUDE.md's `Rust IPC → TS mixin → TS
  command` section describes the **Node era** and reads as current. Porting orchestration into
  Node would reproduce the layering violation. **That section should be corrected before it
  catches the next reader.**
