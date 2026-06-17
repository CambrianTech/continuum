# client/ — the shared Rust client library

The seam between `core/` (substrate) and every embodiment. One Rust crate holds
all the client logic; N language frontends are thin skins over it. One
connection / command / event API, one wire shape, one error model, one auth
surface across every env.

> Canonical layering + the binding decisions: **[docs/architecture/CLIENT-SDK-PLATFORM-ARCHITECTURE.md](../docs/architecture/CLIENT-SDK-PLATFORM-ARCHITECTURE.md)**.
> The API surface every client presents: **[docs/architecture/SDK-API-SURFACE.md](../docs/architecture/SDK-API-SURFACE.md)**.

## Crates

- **`continuum-client/`** — the SDK. `Connection<T: Transport>` carrying identity
  (`session()`) + conversation scope (`scoped(context)`), `CommandClient`,
  `EventSubscriber`, typed `ClientError`. All connection/retry/scope/auth logic
  lives here. Transports are locality-specific:
  - `AircIpcTransport` — out-of-process / over the grid (airc IPC; shares wire
    envelopes with `core/continuum-airc-protocol`).
  - `InProcessTransport` (in `core/continuum-core/runtime`) — an in-core citizen
    (a persona, the foundry) holding the SAME `Connection` over the core's own
    `CommandExecutor`, no serialization. Same API, transport swapped by where you
    are ([[persona-is-a-client]]).

- **`continuum-client-ffi/`** — the FFI-clean facade over `continuum-client`:
  reduces the generic `Connection` to the JSON-at-the-boundary form
  (`execute(cmd, params_json) -> result_json`, `subscribe(class, callback)`,
  `emit`, `provide`, `session()`, `scoped(context)`). Generic-free + stable so
  one binding source serves every native SDK. **uniffi** reads this crate to emit
  ONE native binding → xcframework (Apple) + AAR (Android); **wasm-bindgen** is
  the separate web/node path (uniffi does not emit JS).

## Consumers

- `apps/cli/` — Rust binary; links `continuum-client` directly (no FFI — the
  in-process, simplest leg).
- `sdk/{swift,kotlin}/` — idiomatic skins over the uniffi xcframework / AAR.
- `sdk/flutter/` — bundles the xcframework + AAR and wraps them (NOT a separate
  binding).
- `sdk/typescript/` — thin facade over a `Transport` (wasm-bindgen, or the wire
  when the core is remote).

The contract: anything that talks to a continuum substrate crosses through this
crate. The behavior every frontend must satisfy is pinned by the one executable
conformance spec, `sdk/typescript/conformance.spec.ts`, mirrored per language.
