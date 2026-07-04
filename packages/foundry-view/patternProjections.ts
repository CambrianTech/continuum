/**
 * Foundry → pattern-primitive projections.
 *
 * The client half of foundry as outlier B (ACTIVITY-ROOM-PATTERNS.md): a
 * `ForgeViewState` (kind="foundry") projected onto the consumer-neutral primitives
 * (`@continuum/patterns`), exactly as `@continuum/chat-view` does for chat. The
 * foundry room's model catalogue is the **model `Listing`** (the "HF model list on
 * the right" — a `ContextPanel`), and its `Content` is keyed by the room's
 * `purpose`. Same primitives as chat, different data → the same rows render on any
 * `RenderTarget` (web, terminal, or a persona's grounding).
 */

import type {
  ListingView,
  ListingCell,
  ContentView,
  ContextPanelView,
} from '@continuum/patterns';
import type { ForgeModelView } from '@continuum/sdk-typescript';
import type { ForgeState } from './ForgeState';

/** One catalogue model → a `Listing` cell. The projection resolves the display
 *  fields (subtitle from params + source); a target only draws them. */
function modelCell(m: ForgeModelView): ListingCell {
  // `params_b` is optional (0-is-unknown → absent, honest); label with it when known.
  const subtitle = m.params_b != null ? `${m.params_b}B · ${m.source}` : m.source;
  return {
    id: m.model_id,
    title: m.display_name,
    subtitle,
    badges: [m.source],
    status: 'none',
  };
}

/** The foundry model catalogue projected as the `Listing` primitive — the same
 *  primitive the chat roster and rooms list use, different data. This is the
 *  workbench's right-hand context list. */
export function modelsListing(state: ForgeState): ListingView {
  return {
    id: 'models',
    title: 'Models',
    cells: state.models.map(modelCell),
  };
}

/** The foundry `ContextPanel` — today the model `Listing`; recipe/run widgets join
 *  it additively. */
export function foundryContextPanel(state: ForgeState): ContextPanelView {
  return { listings: [modelsListing(state)] };
}

/** The foundry activity's `Content` body. `Content` is keyed by the room's
 *  `purpose` ("foundry"), so a target's registered foundry renderer draws it. Today
 *  the body is the model set (the config/recipe centre widgets grow it additively);
 *  a chat room would carry a different purpose + body with no shell change. */
export interface ForgeContentBody {
  readonly models: readonly ForgeModelView[];
}

/** The foundry room's `Content`, dispatched by `purpose`. */
export function foundryContent(state: ForgeState): ContentView<ForgeContentBody> {
  return {
    purpose: state.purpose,
    body: { models: state.models },
  };
}
