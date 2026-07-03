// Vendored positron wire types — the neutral state-subscription protocol the
// core's WS ingress speaks (`core/continuum-airc-protocol/src/ws.rs` projects
// `WsClientMessage::Subscribe`/`State` onto exactly these positron frames).
//
// These are ts-rs–generated from the upstream `positron_core` crate
// (`~/Development/positron`, package `@positron/core`). They are vendored here
// — rather than taken as a cross-repo `file:` dependency — so the SDK stays a
// self-contained, public-user-installable workspace package
// ([[headless-core-many-clients]], [[solve-for-public-users]]). Provenance is
// the ts-rs header on each file; do not hand-edit — re-vendor from
// `@positron/core`'s generated tree on a positron wire change.
//
// Scope: `StateConnection` uses `ClientMessage` (the Subscribe frame) and
// `ServerMessage`/`StateEnvelope` (the State frame). The Command* / Observe
// types complete the closure so the union types resolve and a future
// command-over-positron / AI-observer path imports from one place.

export type { StateLayer } from './StateLayer';
export type { KindRevision } from './KindRevision';
export type { StateEnvelope } from './StateEnvelope';
export type { ObserverSpec } from './ObserverSpec';
export type { CommandSource } from './CommandSource';
export type { CommandEnvelope } from './CommandEnvelope';
export type { ClientMessage } from './ClientMessage';
export type { ServerMessage } from './ServerMessage';
