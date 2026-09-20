# Client / SDK / Platform Architecture

> The headless Rust core is the server. Everything that talks to it is a **client**,
> and clients are built in two tiers: **SDKs** (libraries) and **apps** (consumers
> of SDKs). None is privileged; the desktop is just one app. See
> [[headless-core-many-clients]].

## Why this rewrite: performance (not style)

Nobody likes a rewrite — this one is justified by **speed and CPU**, not aesthetics.
The old system was slow and CPU-intensive because the hot path ran **serde/parse +
IPC + ORM in Node** ([[airc-performance-doctrine]]). It will not work at grid scale
unless it's optimized, and **headless is the enabler** of that optimization. Every
rule below serves performance:

- **Logic in Rust, Node off the hot path** — no per-request parse/IPC/ORM tax in a
  JS runtime; one optimized implementation (BLAS/SIMD/GPU where it matters,
  [[optimization-is-always-first]]).
- **Headless** — no browser/render loop burning CPU on machines that are servers
  (AWS, BigMama, grid nodes); rendering happens only if a human client attaches.
- **Thin, generated SDKs** — no hand-written marshalling layers (CPU + drift); the
  same Rust lib, not N reimplementations.
- **Consolidated, bounded work** — e.g. cognition runs once over a consolidated
  burst at `O(capacity)`, never per-event FIFO.

If a choice here ever trades performance for convenience, it's the wrong choice.

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
2. **uniffi for BOTH native (Swift + Kotlin) now.** One annotated crate → both
   bindings → the xcframework + AAR. **Proc-macro mode, NOT a `.udl`** (proven
   2026-06-17, see below): the facade is annotated in-place
   (`#[uniffi::export]`/`Object`/`Record`/`Error`), bindgen reads the compiled
   cdylib. `swift-bridge` is DEFERRED — add the second toolchain ONLY when
   native-iOS/visionOS async ergonomics prove load-bearing for `apps/vr` /
   `apps/ar` (outlier-validation, not preemptive).
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

## The TypeScript / web layer (most non-Rust code — still generation-first)

The Node/TS world will hold the **most non-Rust code** — because the UI is rich —
but the same law applies: logic concentrates in Rust, TS stays a boundary +
presentation layer.

- **`sdk/typescript` = the web boundary, generated-first.** As much **ts-rs** as we
  can get away with (types come from Rust, never hand-written); hand-written TS only
  for what generation can't cover. Its job is to make the web boundary
  *straightforward* — not to hold logic.
- **It's `/shared`.** Because we chose TS, the boundary types + helpers are
  environment-agnostic and reused across **frontend AND backend** (the
  `shared/browser/server` tier — `shared/` imports neither browser nor server).
  One type definition, both sides. This is the specific payoff of TS for the web
  embodiment that the native SDKs don't get.
- **The UI mass lives in the app, not the SDK.** `apps/web` holds the widgets, DOM,
  rendering, presentation; `apps/desktop` (Tauri) wraps that same UI. The SDK stays
  thin; the app is where the platform's "most code" legitimately accumulates — but
  it's UI, not substrate logic.

## Toolchain reality (who can build what)

The native-glue lane splits by **toolchain**, not just intent — Apple artifacts
need macOS/Xcode:

| Step | Runs on | Owner |
|------|---------|-------|
| Rust facade + uniffi proc-macro annotations + bindgen (emits Swift **and** Kotlin source from the cdylib), Android AAR + Kotlin SDK | any OS (verified building + generating on Windows) | BigMama |
| xcframework packaging, Swift SDK build/validate, visionOS | **macOS/Xcode only** | a Mac (M5/IntelMac) or a GitHub **macos-runner** CI job |
| wasm-bindgen web binding | any OS | (web lane) |

Keep the binding **single-source** (the annotated Rust facade is the source of
truth; generated Swift/Kotlin source is a regenerated build artifact, never
committed); put only the Apple *packaging/validation* on a Mac. A `macos-runner`
CI job is the durable home so it doesn't depend on any one operator's laptop.

### Proven: the uniffi binding (2026-06-17, PR #1675)

Outlier-B for the SDK interface is landed — uniffi 0.31 binds the full 4-verb
facade cleanly, with no shape forced on it:

- **Proc-macro mode, no `.udl`.** `continuum-client-ffi/src/lib.rs` is annotated
  in-place: `#[uniffi::Object]` (`ContinuumClient`, `Subscription`,
  `Registration`), `#[uniffi::Record]` (`SessionIdentity`), `#[uniffi::Error]`
  (`FfiError`), `#[uniffi::export(with_foreign)]` callback interfaces
  (`EventCallback`, `CommandHandler`), async verbs under
  `#[uniffi::export(async_runtime = "tokio")]`, `connect()` as
  `#[uniffi::constructor]`, and `Uuid` via `custom_type!` (↔ `String`).
- **Bindgen is in-crate** (`src/bin/uniffi-bindgen.rs` + uniffi `cli` feature) so
  the generator version is pinned to the runtime version by Cargo:
  ```sh
  cargo run --bin uniffi-bindgen -- generate \
    --library <cdylib: .dylib/.so/.dll> --language <swift|kotlin> --out-dir <dir>
  ```
- **Generated native surface** (Swift; Kotlin mirrors with suspend funs +
  `Uuid = String` typealias + sealed `FfiError`). `snake_case` → `lowerCamel`
  (`params_json` → `paramsJson`):
  ```swift
  open class ContinuumClient {
    public static func connect(home:agentName:socket:targetPeer:) async throws -> ContinuumClient
    open func execute(command:paramsJson:) async throws -> String
    open func provide(command:handler:)   async throws -> Registration
    open func subscribe(class:callback:)  -> Subscription
    open func emit(class:payloadJson:)     async throws
    open func scoped(contextId:)           -> ContinuumClient
    open func session()                    -> SessionIdentity
  }
  ```
  This is the symbol set the native typed/idiomatic emitter (M5's
  `sdk_codegen`) targets — real names, not guesses.

## Locking the abstraction: three divergent platforms in parallel (outlier validation)

Per the CLAUDE.md outlier-validation strategy — don't build platforms exhaustively
or hopefully; build the **most divergent** ones in parallel so they *prove* the
abstraction. If `client/continuum-client` (the common Rust SDK) + the facade serve
all three cleanly, the interface is locked and every other platform is trivial.

| Track | Binds the common Rust SDK via | Validates the axis |
|-------|-------------------------------|--------------------|
| **cli** (`apps/cli`, Rust) | links `continuum-client` **directly** | in-process, no FFI, no wire — the simplest path |
| **mobile** (`apps/mobile` + `sdk/{flutter,swift,kotlin}`) | **uniffi** → xcframework/AAR, Flutter bundles them | cross-language FFI + cross-toolchain — the hardest path |
| **nodejs** (`sdk/typescript` + a node consumer) | **wasm/napi** (or RustCoreIPC wire) | JS runtime, possibly a remote core — the third axis |

They share ONE conformance spec (`sdk/typescript/Commands.test.ts` is the reference;
each language mirrors it — Rust cargo, swift/kotlin) + ONE hot-path timing budget.
Building them together is what surfaces an abstraction leak early: if a verb or a
type can't cross all three cleanly, the facade — not the platform — is wrong, and
we fix it once at the root. The common Rust SDK is the single source of truth all
three wrap; nothing reimplements logic per platform.

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

## Onboarding & sharing: a node is a shareable contact card

Joining or sharing a mesh node should feel like sharing a contact — because it
*is* one. The grid's directory layer (airc `#1247`: identity + reachable
endpoints + rooms + capability offers, published as small JSON "cards" through a
gist/registry, metadata-only — never the message plane) makes a node's
self-presentation tiny enough to travel over **any** low-bandwidth channel. That
is the whole onboarding UX, and it's where the mobile SDKs earn their keep.

**A node/grid card is vCard-shaped** — "who I am, how to reach me, what I offer."
Everyone already knows the gesture for sharing a contact, so the mesh inherits it:

- **QR — scan to join.** One device shows its card, another scans → enroll + connect.
- **Deep / universal links** — `airc://join/…` or an `https://…/join/…` link that's
  tappable straight out of any messenger; opens the app, pins the key, lands on the
  directory.
- **Native share sheet** — "Share this room/grid" → AirDrop / Messages / copy-link.
- **NFC tap** — tap-to-share two phones (or a sticker), lower friction than QR.

**Durability rule (so a share never rots): encode the stable POINTER, not the
volatile endpoint.** Router ports change on restart (the airc `#1243` stale-port
bug); a QR/link with an inline `host:port` would go stale. Encode "this gist +
pin this key" instead — the gist refreshes the live endpoints behind a stable
front door, so a printed QR or a months-old link still resolves to the current
router. (An inline-endpoint QR is fine for ephemeral "join me on this LAN right
now"; the gist-pointer form is the durable default.)

**Public vs private is just the card's audience.** A public node posts its card
openly; a private-VPN grid's card is shared only within trust (DM the link to an
invitee). Same card, same mechanism — the trust boundary is *which gist*.

### Identity = passkeys (secure, synced, no key management)

The card carries a **public** key to pin; the matching **private** key is the
user's mesh identity, and it should live where modern platforms already keep
secrets best: a **passkey** — a hardware-backed (Secure Enclave / StrongBox),
biometric-gated, platform-synced credential. The payoff:

- **No raw-key management.** The user never sees or copies a key; Face/Touch ID
  unlocks it. (Replaces the brittle `identity.key`-on-disk story for human users.)
- **Identity follows the human across devices.** iCloud Keychain / Google Password
  Manager sync the passkey, so the same mesh identity appears on phone, tablet,
  laptop without an export/import dance.
- **Phishing-resistant + revocable** per WebAuthn, and it composes with the
  existing trust model — the mesh still pins an Ed25519 **public** key; passkeys
  are how the **private** half is held + synced + unlocked.

Open design point (resolve when the mobile SDK lands): whether the passkey
**is** the airc signing key (if the platform authenticator can produce/sign in
the curve airc's protocol uses) or **guards** an Ed25519 key sealed in the
secure element. Either way the user-facing contract is identical — biometric
unlock, synced across your devices, never a key to copy.

### Where it slots in the stack

The substrate is already laid: the **uniffi facade** (`#1675`, landed) is what the
Swift/Kotlin SDKs wrap, and the **directory cards** come from airc. The
*native affordances* — camera/QR, NFC, WebAuthn/passkeys, universal links, the
share sheet — live in the **platform SDKs** (`sdk/swift`, `sdk/kotlin`) and the
mobile app on top, not in the Rust core (which stays headless + UI-free). So the
build path is concrete: facade ✅ → native SDKs (the typed layer + these
platform affordances) → `apps/mobile` onboarding (scan/tap/click → passkey
unlock → enroll → directory → connect). The directory makes the payload
*shareable*; mobile makes sharing it *delightful*; passkeys make it *secure*.

## Non-negotiables

- **Rust-first.** node only for genuinely-web clients; a headless user never
  installs node ([[rust-is-the-core-node-is-the-shell]]).
- No UI/business logic in an SDK — substrate decisions stay in `core/`.
- One owner per cross-cutting concern (e.g. `config.env` → `config_env.rs`,
  [[config-env-single-owner]]).
