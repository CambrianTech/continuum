// Vendored Experience (Join Contract) manifest closure — the room-level STRUCTURE a
// renderer projects into a Workspace: `purpose`, `regions` (scope/role/slot bound to a
// live payload `kind`), `affordances` (verb + who_may + proves), `membership`
// (standing), and an optional `layout` tree. ts-rs source of truth:
// `protocol/typescript/experience/`. Vendored (copied, not cross-imported) by
// `scripts/vendor-views.mjs` — the `.ts` files here are NOT hand-edited; on a manifest
// change run `npm run vendor:views`. THIS barrel (the exported names) IS hand-maintained.
//
// `TrustLevel` (from `../grid/`) is re-exported here too: `Affordance.who_may` is a
// `TrustLevel | null`, so a consumer of `Affordance` needs to name it.

export type { Experience } from './Experience';
export type { Region } from './Region';
export type { RegionScope } from './RegionScope';
export type { RegionRole } from './RegionRole';
export type { Affordance } from './Affordance';
export type { ProofSpec } from './ProofSpec';
export type { Member } from './Member';
export type { Standing } from './Standing';
export type { Layout } from './Layout';
export type { LayoutChild } from './LayoutChild';
export type { TrustLevel } from '../grid/TrustLevel';
