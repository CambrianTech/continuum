# Client / SDK / Platform Architecture

> The headless Rust core is the server. Everything that talks to it is a **client**,
> and clients are built in two tiers: **SDKs** (libraries) and **apps** (consumers
> of SDKs). None is privileged; the desktop is just one app. See
> [[headless-core-many-clients]].

## The organizing law: concentrate logic as deep as possible (≈ all Rust)

Push **every bit of logic to the deepest shared layer** — which is almost entirely
Rust. Connection, retry, reconnection, backoff, caching, event demux, auth, error
modeling, state — ALL of it lives in `client/continuum-client` (Rust), so every
platform inherits it for free and behaves identically. **Every client uses the one
Rust lib.**

A binding is NOT "thin business logic" — it holds **zero logic**. The only thing a
per-language SDK adds is *idiomatic surface* (Swift `async/await` · Kotlin `Flow`
· Dart `Stream` · TS `Promise`) over the FFI facade, plus *generated* types. "Two
headers over the xcframework and AAR" (Joel) — and nothing more. If you're tempted
to write logic in Swift/Kotlin/Dart/TS, it belongs in the Rust lib instead.

**The quality bar:** each SDK must **seem coded from the ground up** — a Swift dev
should feel they're holding a first-class, hand-crafted Swift SDK; a Kotlin dev a
real Kotlin SDK — while in reality it's a thin conversion/wrapper layer over the
shared Rust lib. The thinness must NOT show. That's the whole art: maximal native
feel, near-zero native code. Conversions to each language's paradigm are the work;
logic is not.

Web is no exception: its deepest form is **wasm of `client/continuum-client`**, not
a hand-written TS client. The mature hand-written `sdk/typescript` (RustCoreIPC) is
transitional; logic migrates down into the Rust lib, the TS surface thins toward a
wasm/wire shim + generated types. (When the core is remote, the TS client is thin
anyway — the core does the logic over the wire.)

## The two tiers

| Tier | What | Rule |
|------|------|------|
| **SDK** (library) | per-language/per-platform binding to the core — `Connection / CommandClient / EventSubscriber` in idiomatic shape | NO business logic (that's `core/`), NO UI. Thin. |
| **App** (consumer) | an actual product — web, desktop, mobile, cli, vr, ar, mcp | consumes ONE SDK; holds its own platform extensions + UX |

"Platforms" (Joel's word from his RN/Flutter work) **are SDKs** — `sdk/ios`, `sdk/android`, `sdk/web` are platform SDKs.

## The layered SDK stack (NOT flat)

The load-bearing correction (Joel, from shipping RN SDKs with AARs/Maven +
xcframeworks inside): **cross-platform SDKs do not bypass the native SDKs — they
contain and use them.**

```
core/  (the substrate — Rust)
  ▲ airc IPC / WebSocket / cross-grid (same wire family)
client/continuum-client  (Rust)   ← THE Rust SDK. apps/cli links it directly.
  │  ONE FFI facade (generic-free, JSON at the boundary):
  │     execute(cmd: String, params_json: String) -> result_json
  │     subscribe(pattern: String) -> stream<event_json>
  │  uniffi → ONE native binding
  ▼
platform artifacts:   xcframework (Apple)   ·   AAR / Maven (Android)   ·   wasm / napi (web, node)
  ▼
native platform SDKs (idiomatic typed layer, generated types — never hand-written):
   sdk/swift   (Swift async/await, AsyncStream — over the xcframework)
   sdk/kotlin  (Kotlin suspend, Flow<T>      — over the AAR)
   sdk/typescript (over wasm / WebSocket     — web/desktop/mcp)
  ▼
cross-platform SDKs (BUNDLE the native artifacts inside and USE them):
   sdk/flutter      (Dart plugin: ships the xcframework + AAR inside; Stream<T>)
   sdk/react-native (same pattern, if/when)
  ▼
apps (consumers; own their platform extensions):
   apps/web      → sdk/typescript
   apps/desktop  → sdk/typescript (Tauri)         — next to web
   apps/mobile   → sdk/flutter   (one codebase, iOS+Android; push, deep links, background, sensors)
   apps/cli      → client/continuum-client (Rust direct; the `ctm` binary)
   apps/vr       → EXTENDS the native platform SDK (visionOS rides sdk/swift; Quest rides sdk/kotlin)
   apps/ar       → EXTENDS the native platform SDK (same)
   apps/mcp      → sdk/typescript (or Rust)
```

### Why this layering (the compression)

ONE native binding (uniffi → xcframework + AAR) is consumed by **native iOS, native
Android, AND Flutter/RN**. There is NOT a separate `flutter_rust_bridge` binding
competing with the native SDKs — Flutter wraps the same artifacts the native apps
use. One Rust crate, N language frontends, one wire shape, one error model, one
auth surface ([[command-event-decision-rule]], the compression principle).

`mobile` is a category; `ios`/`android` are platforms under it; `vr`/`ar` extend
the native platform SDKs (XR is an Apple or Android target). The CLI is its own
SDK consumer (it *is* the Rust SDK in use). Many apps are themselves composable
SDKs.

## Decisions (locked 2026-06-17, M5/BigMama/IntelMac)

1. **FFI boundary = JSON at the boundary.** `execute(cmd, params_json) -> result_json`
   + `subscribe(pattern) -> stream<event_json>`. Generic-free (Rust's generic
   `Commands.execute<T,U>` can't cross FFI), tiny, stable — it's exactly what
   Commands/Events are on the wire. The typed/idiomatic per-language layer is
   **generated** (ts-rs and the per-language equivalent), never hand-written; the
   JSON shape is the canonical contract.
2. **uniffi for BOTH native (Swift + Kotlin) now.** One `.udl` → both bindings →
   the xcframework + AAR. `swift-bridge` is DEFERRED — add the second toolchain
   ONLY when native-iOS/visionOS async ergonomics prove load-bearing for `apps/vr`
   / `apps/ar` (outlier-validation, not preemptive).
3. **TWO binding mechanisms over the ONE facade** (uniffi does NOT emit wasm/JS):
   - **uniffi** → native (Swift xcframework + Kotlin AAR) → also what Flutter bundles.
   - **wasm-bindgen** (or napi) → `sdk/typescript` for web/node. Web is its OWN
     binding path; nobody should expect uniffi→web.
   Same `client/continuum-client` facade underneath both.
4. **Flutter mechanism = reuse, not a third binding.** The Flutter plugin bridges
   Dart → platform-channel → the **swift/kotlin SDKs** (the idiomatic layer already
   built), packaging the xcframework + AAR inside. NOT raw uniffi, NOT
   `flutter_rust_bridge`. Slightly more indirection, but one binding reused
   everywhere — the matrix-rust-sdk distribution shape.

## Toolchain reality (who can build what)

The native-glue lane splits by **toolchain**, not just intent — Apple artifacts
need macOS/Xcode:

| Step | Runs on | Owner |
|------|---------|-------|
| Rust facade, uniffi `.udl` + bindgen (emits Swift **and** Kotlin source), Android AAR + Kotlin SDK | any OS (verified building on Windows) | BigMama |
| xcframework packaging, Swift SDK build/validate, visionOS | **macOS/Xcode only** | a Mac (M5/IntelMac) or a GitHub **macos-runner** CI job |
| wasm-bindgen web binding | any OS | (web lane) |

Keep the binding **single-source** (one `.udl`, generated Swift source shared); put
only the Apple *packaging/validation* on a Mac. A `macos-runner` CI job is the
durable home so it doesn't depend on any one operator's laptop.

## Build order

1. **Foundation** — `client/continuum-client` FFI-clean JSON facade (BigMama; it
   wraps airc-lib). M5 confirms the canonical command/event set the facade projects.
2. **Prove the facade** via the two already-real consumers — `sdk/typescript` (web)
   + `apps/cli` (Rust) — before native glue. Battle-test the boundary.
3. **Native platform SDKs** — uniffi → xcframework + AAR + the Swift/Kotlin
   idiomatic layer (BigMama).
4. **Cross-platform SDK** — `sdk/flutter` as a thin Dart plugin packaging those
   artifacts (BigMama).
5. **Apps** — `apps/mobile` (Flutter, the headline new embodiment), then `vr`/`ar`
   extending the native SDKs, as demand lands.

## Lanes (this round)

- **BigMama drives** — the foundation facade + native glue (Rust-FFI is her
  wheelhouse after the embedding/generate facility bridges).
- **M5 architects the API surface** — the canonical command/event set every SDK
  exposes + the generated typed-contract — and reviews. (Heads-down on
  cognition cutover → ToolExecutor; not a build lane this round.)
- **IntelMac integrates** from the recipe-walker / web-SDK side (closes the
  substrate→UI loop).

## Non-negotiables

- **Rust-first.** node only for genuinely-web clients; a headless user never
  installs node ([[rust-is-the-core-node-is-the-shell]]).
- No UI/business logic in an SDK — substrate decisions stay in `core/`.
- One owner per cross-cutting concern (e.g. `config.env` → `config_env.rs`,
  [[config-env-single-owner]]).
