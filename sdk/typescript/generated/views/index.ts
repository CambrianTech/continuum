// Vendored continuum ViewState payloads — the CONCRETE state a widget renders,
// the thing that fills `StateEnvelope.payload` for a given `kind`.
//
// Distinct from `../positron/`: those are the NEUTRAL positron transport frames
// (StateEnvelope, ClientMessage, …) that any adopter shares; THESE are
// continuum's own view payloads (`kind="chat"` → ChatViewState), ts-rs–exported
// from the `continuum-positron` crate (source of truth:
// `protocol/typescript/positron/`). Vendored here — not imported across
// packages — so the SDK stays a self-contained, public-user-installable
// workspace package and a UI app depends only on `@continuum/sdk-typescript`
// ([[headless-core-many-clients]], [[persona-is-a-client]]).
//
// Scope: the `chat` widget closure only (ChatViewState + its row/roster/
// identity types). Wall/kanban/etc. get vendored when a widget renders them —
// outlier-validate then STOP. Re-vendor from `protocol/typescript/positron/` on
// a view-payload change; do not hand-edit.

export type { ChatViewState } from './ChatViewState';
export type { ChatMessageView } from './ChatMessageView';
export type { RosterSlotView } from './RosterSlotView';
export type { SenderKind } from './SenderKind';
export type { Provenance } from './Provenance';
