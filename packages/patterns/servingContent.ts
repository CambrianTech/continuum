/**
 * The `serving` activity's neutral `Content` body — the node-ops CONSOLE face.
 *
 * The serving stack is the machine room ([[north-star-metrics]]: persona-tok/s
 * × quality ÷ cost): this face renders it center-stage as a full console —
 * per-node panels carrying the live control loop (model banner, the headline
 * tok/s numeral, hit/fetch sparklines, the bandit's arm bank, the pager event
 * feed). It is a room PURPOSE (`SERVING_PURPOSE`), reached by the same nav
 * semantics as any activity and rendered by a purpose-registered `Content`
 * renderer — the ops sibling of `'arena'` / `'foundry'` / `'live'`. Shapes
 * only: consumer-neutral, DOM-free, ANSI-free.
 *
 * Console doctrine (Joel 2026-08-01): left rail = navigation + one condensed
 * performance instrument; the graphical FULL VIEW lives HERE, center-stage
 * with all the scroll room, futuristic-console legibility — state readable at
 * a distance. Multi-node from day one: the grid renders every node's panel
 * (local first), so BigMama's 5090 works beside the M5 the moment the
 * cross-grid feed carries her view. Honest absence: a node with no pager
 * capture renders its banner + an awaiting line, never a fabricated gauge.
 */

import type { ServingPanelView } from './index';

/** The `Content` purpose key the serving console dispatches on. A room recipe
 *  that declares this purpose IS a serving-ops room. */
export const SERVING_PURPOSE = 'serving';

/** One node's console panel — identity + its live serving view. */
export interface ServingNodeVM {
  /** Host name ("bigmama.local", "m5.local"). */
  readonly node: string;
  /** True for the node this surface runs on (renders first, marked). */
  readonly local: boolean;
  /** The node's live serving view — same shape the rail widget folds. */
  readonly view: ServingPanelView;
}

/** The serving console's content body. */
export interface ServingContentBody {
  /** Per-node panels, local node first. Empty = no serving feed anywhere —
   *  the awaiting frame renders (the frame is the promise). */
  readonly nodes: readonly ServingNodeVM[];
  /** True only when a live envelope stream is attached — a static projection
   *  renders the honest "snapshot" banner (same contract as the arena). */
  readonly feedLive: boolean;
}
