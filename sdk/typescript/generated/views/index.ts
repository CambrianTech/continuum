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

// nav closure (kind="nav" → NavViewState) — a citizen's open tabs (with derived
// unread), current tab, per-room read cursors, and bookmarks. Per-USER, unlike
// the per-room chat/roster/foundry kinds; served from the citizen's ?me= scoped
// session substrate.
export type { NavViewState } from './NavViewState';
export type { NavTab } from './NavTab';
export type { NavBookmark } from './NavBookmark';
export type { NavTargetKind } from './NavTargetKind';

// system-metrics closure (kind="system-metrics" → SystemMetricsViewState) — the
// node's live CPU/MEM series the SYS gauge draws, core-carried window.
export type { SystemMetricsViewState } from './SystemMetricsViewState';
// serving closure (kind="serving" → ServingViewState) — the serving glass box
// (#141 slice 1: header + pager series + bandit arms + event cards).
export type { ServingViewState } from './ServingViewState';
export type { ServingHeaderView } from './ServingHeaderView';
export type { ServingArmView } from './ServingArmView';
export type { ServingEventCard } from './ServingEventCard';
export type { MetricSeriesView } from './MetricSeriesView';

// kanban closure (kind="kanban" → KanbanViewState) — the room's work board;
// vendored for the persona home's claims feed (cards filtered by assignee).
export type { KanbanViewState } from './KanbanViewState';
export type { KanbanCardView } from './KanbanCardView';
export type { KanbanLaneView } from './KanbanLaneView';
export type { KanbanCardState } from './KanbanCardState';
export type { KanbanLaneState } from './KanbanLaneState';
export type { KanbanPriority } from './KanbanPriority';
export type { KanbanPullRequest } from './KanbanPullRequest';
export type { BenchViewState } from './BenchViewState';
export type { BenchRunRow } from './BenchRunRow';
