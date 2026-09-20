# sdk/ — per-language SDKs over `client/continuum-client`

Thin, idiomatic language skins over the one Rust client SDK. **All logic lives
in `client/continuum-client`** ([[headless-core-many-clients]]); an SDK here holds
ZERO logic — it converts the client's surface to its platform's conventions
(Swift async/await, Kotlin coroutines/Flow, Dart streams, TS Promise) over
generated types, nothing more. One shared core, N language frontends, ONE
mirrored API surface.

> Canonical layering + the binding decisions this table follows:
> **[docs/architecture/CLIENT-SDK-PLATFORM-ARCHITECTURE.md](../docs/architecture/CLIENT-SDK-PLATFORM-ARCHITECTURE.md)**.
> The API surface every SDK presents: **[docs/architecture/SDK-API-SURFACE.md](../docs/architecture/SDK-API-SURFACE.md)**.

## The binding layers

```
client/continuum-client        (Rust — the SDK; all logic)
  └─ client/continuum-client-ffi   (uniffi-annotated facade; JSON at the boundary)
       ├─ uniffi  → ONE native binding → xcframework (Apple) + AAR/Maven (Android)
       └─ wasm-bindgen (or napi) → the web/node binding   (uniffi does NOT emit JS)
```

**ONE native binding (uniffi → xcframework + AAR) serves native iOS, native
Android, AND Flutter.** There is NOT a separate `flutter_rust_bridge` binding.

| SDK | Binding | Consumed by |
|-----|---------|-------------|
| `swift/` | idiomatic Swift over the **uniffi xcframework** (async/await + `AsyncStream`) | native iOS / visionOS apps |
| `kotlin/` | idiomatic Kotlin over the **uniffi AAR** (suspend + `Flow`) | native Android / Quest apps |
| `flutter/` | **bundles the xcframework + AAR** and wraps them via platform channels (Dart) — NOT `flutter_rust_bridge` | `apps/mobile`, `apps/ar`, `apps/vr` |
| `typescript/` | thin facade over a `Transport` — **wasm-bindgen** of `continuum-client` for web, or the RustCoreIPC wire when the core is remote | `apps/web`, `apps/desktop`, `apps/mcp` |

`swift-bridge` is DEFERRED — uniffi binds both native targets today; add a second
toolchain only if visionOS/AR async ergonomics prove it load-bearing.

## The contract

Every SDK presents the same four-verb surface — `execute` / `provide` (Commands)
+ `subscribe` / `emit` (Events) — plus `session()` / `scoped(context)` and the
addressable `Handle`. That surface is pinned by **one executable conformance
spec**, `sdk/typescript/conformance.spec.ts`: each language SDK mirrors its
clauses name-for-name (Rust `cargo test`, Swift `XCTest`, Kotlin `JUnit`). If a
behavior isn't in the spec it isn't in the contract; if it is, every SDK proves
it. That's what keeps N language skins from drifting into N subtly-different
clients.

## What an SDK is NOT

- NOT a place for business logic, session/event handling, or auth — those live
  once in the Rust client + the substrate, and an SDK that re-implements them is
  the bug ([[lock-uniform-client-early]]).
- NOT a home for legacy wire types or the old `RustCoreIPC` client — that lives
  at `core/continuum-core/bindings/` and is being reworked into app code, not SDK
  code. An SDK package is the facade + generated types + the conformance spec,
  and nothing else.
