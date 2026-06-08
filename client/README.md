# client/ — shared rust client library

The seam between `core/` (substrate) and every embodiment. One rust
crate, N language frontends, one connection / command / event API.

## Crates

- **`continuum-client/`** — `Connection<T: Transport>` + `CommandClient` +
  `EventSubscriber` + typed `ClientError`. `AircIpcTransport` is the
  canonical local-substrate impl (over airc IPC, shares wire envelopes
  with `core/continuum-airc-protocol`).

## Consumers

- `apps/cli/` — Rust binary, links `continuum-client` directly (no SDK).
- `sdk/{flutter,swift,kotlin,typescript}/` — language wrappers bridge
  `continuum-client` to their platform's ecosystem via FFI.

The contract: anything that wants to talk to a continuum substrate
crosses through this crate, ensuring one wire shape, one error model,
one auth surface across every env.
