/**
 * The `grid` activity's neutral `Content` body — the GRID's SCADA view.
 *
 * The left NODES strip is the grid's condensed state (the portal); THIS is
 * its full center-stage activity (console doctrine): one rich panel per
 * node — identity, the resource window, the serving control loop — with
 * route health and resident citizens joining as their feeds land (#257
 * peer attestation, #283 cross-grid serving, delivery-truth from the airc
 * ledger). A room PURPOSE (`GRID_PURPOSE`), same nav semantics as any
 * activity, rendered by a purpose-registered `Content` renderer. Shapes
 * only: consumer-neutral, DOM-free, ANSI-free.
 *
 * Honest absence: a node panel renders exactly the sections its feeds
 * deliver — no fabricated gauges, no invented peers; an empty grid renders
 * the awaiting frame (the frame is the promise).
 */

import type { GaugeView, ServingPanelView } from './index';

/** The `Content` purpose key the grid view dispatches on. */
export const GRID_PURPOSE = 'grid';

/** One node's grid panel — identity plus whichever live views its feeds carry. */
export interface GridNodeVM {
  /** Host name ("m5.local", "bigmama.local"). */
  readonly node: string;
  /** True for the node this surface runs on (renders first, marked). */
  readonly local: boolean;
  /** The node's resource window (CPU/MEM/GPU) — absent until its metrics
   *  feed delivers. */
  readonly resources?: GaugeView;
  /** The node's serving control loop — absent until its serving feed
   *  delivers. */
  readonly serving?: ServingPanelView;
}

/** The grid view's content body. */
export interface GridContentBody {
  /** Per-node panels, local node first. Empty = no node feeds anywhere. */
  readonly nodes: readonly GridNodeVM[];
  /** True only when live envelope streams are attached. */
  readonly feedLive: boolean;
}
