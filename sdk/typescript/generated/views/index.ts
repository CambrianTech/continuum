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
// Scope: the vendored widget closures — `chat` (ChatViewState + its row/roster/
// identity types) and `foundry` (ForgeViewState + ForgeModelView). Wall/kanban/etc.
// get vendored when a widget renders them — outlier-validate then STOP.
//
// The `.ts` files here are NOT hand-edited: they are copied verbatim from
// `protocol/typescript/positron/` by `scripts/vendor-views.mjs` (the declared
// VENDORED set). On a view-payload change, run `npm run vendor:views`; the
// `vendor:views:check` step in `check:clients` fails loud if they ever drift (#80).
// This barrel (the exported names) IS hand-maintained — add a line when a closure
// grows.

export type { ChatViewState } from './ChatViewState';
export type { ChatMessageView } from './ChatMessageView';
export type { RosterSlotView } from './RosterSlotView';
export type { SenderKind } from './SenderKind';
export type { Provenance } from './Provenance';

// roster widget kind (kind="roster" → RosterViewState) — the Join Contract's roster
// region payload, decomposed out of ChatViewState (path-3 per-region ViewStates).
export type { RosterViewState } from './RosterViewState';

// foundry widget closure (kind="foundry" → ForgeViewState + its model row)
export type { ForgeViewState } from './ForgeViewState';
export type { ForgeModelView } from './ForgeModelView';
