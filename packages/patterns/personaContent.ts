/**
 * The `persona` activity's neutral `Content` body — the persona HOME surface.
 *
 * A persona's profile/brain is NOT a special view: it is the persona's home
 * activity, reached by the SAME nav semantics as any room (`nav/select` with a
 * persona-kind tab) and rendered by a purpose-registered `Content` renderer
 * (`PERSONA_PURPOSE`), exactly as `chat`/`foundry` dispatch theirs
 * (POSITRON-PURE-ROOMS-BRIEF.md "persona home"; tabs == rooms == activities).
 * This file holds only the SHAPES — consumer-neutral, DOM-free, ANSI-free —
 * so web/tui/RAG all draw the same projected body.
 *
 * Honesty contract ([[fallbacks-are-illegal-fail-loud]]): every field is either
 * REAL (projected from live state: presence, vitals radiator, genes, loadout,
 * board claims) or explicitly ABSENT (`undefined` / empty + a `live` flag), and
 * a renderer draws an awaiting/empty frame for the absent — sections never
 * fabricate and never vanish.
 */

import type { CellLoadout } from './index';

/** The `Content` purpose key the persona home dispatches on — the persona
 *  sibling of `'chat'` / `'foundry'`. Mirrors the Rust nav reader's
 *  `purpose: "persona"` tab fact (one string per side, single-sourced). */
export const PERSONA_PURPOSE = 'persona';

/** One region card of the brain HUD (the Cognitive System View): a labelled
 *  brain region lit by a LIVE faculty level — the SAME `persona:vitals` pulse
 *  the roster tile's cognition compass draws, never a second signal. */
export interface PersonaBrainRegionVM {
  /** Stable id (`'prefrontal'`, `'hippocampus'`, …) — the drill-down key. */
  readonly id: string;
  /** Region label, drawn uppercase ("PREFRONTAL"). */
  readonly label: string;
  /** The bracketed role caption ("[ EXECUTIVE ]"). */
  readonly role: string;
  /** Which live vitals key lights this region (`'reason'`, `'recall'`, …).
   *  Empty = no live faculty maps here yet (the region renders an honest
   *  awaiting state — e.g. LIMBIC until an affect signal radiates). */
  readonly facultyKey: string;
  /** Live level 0..=100 from the vitals pulse. `undefined` = the signal is
   *  absent (not zero!) — the renderer draws "awaiting signal", never a
   *  fabricated 0-bar. */
  readonly level?: number;
  /** Short status word derived from the level ("ACTIVE", "QUIET", …) — the
   *  projection owns the wording; a target only paints it. */
  readonly status: string;
  /** Extra real facts for the region card + its drill-down detail
   *  (label → pre-formatted value), e.g. Focus 62 on PREFRONTAL. */
  readonly detail: readonly PersonaRegionFact[];
}

/** One label→value fact line (region detail, ABOUT card, footer stats). */
export interface PersonaRegionFact {
  readonly label: string;
  readonly value: string;
}

/** One pathway tile (the profile's PATHWAYS grid — Brain/Genome/DM/Logs/
 *  Memory/Stats). A pathway IS a nav intent: `target` names where it goes.
 *  `enabled: false` = the destination doesn't exist as an activity yet — the
 *  tile renders honestly disabled ("coming soon"), never a dead click. */
export interface PersonaPathwayVM {
  readonly id: string;
  readonly label: string;
  /** Small caption under the label ("COGNITIVE VIEW"). */
  readonly sublabel: string;
  /** Glyph token for the tile icon. */
  readonly glyph: string;
  /** Where the pathway navigates: an in-content anchor (`'#brain'`) today, a
   *  room/target ref when the destination room-ifies. */
  readonly target: string;
  readonly enabled: boolean;
}

/** One work-board claim of this persona (a kanban card it is assignee of) —
 *  the activity-feed row. Projected from the live `kind="kanban"` board. */
export interface PersonaClaimVM {
  readonly id: string;
  readonly title: string;
  /** The card's lifecycle state, as the wire string ("inProgress", "open"…). */
  readonly state: string;
  /** Priority badge ("P0"… "P3"). */
  readonly priority: string;
  /** Raw epoch ms of the card's last event — the renderer formats recency. */
  readonly updatedAtMs: number;
}

/** One published writing of the persona (blog / wonder-work feed). The feed is
 *  honest-empty today — the shape exists so the section renders its frame and
 *  the blog lands without a shape break. */
export interface PersonaWritingVM {
  readonly id: string;
  readonly title: string;
  readonly summary: string;
  readonly publishedAtMs: number;
}

/** The persona home's `Content` body (`purpose === PERSONA_PURPOSE`). */
export interface PersonaContentBody {
  /** The persona's citizen id (the nav tab's target). */
  readonly personaId: string;
  /** Display name from presence. Empty = the roster hasn't resolved this
   *  citizen yet (the renderer draws the awaiting-identity frame). */
  readonly name: string;
  /** `@handle` line — the lowercased name until a self-authored handle
   *  exists. Empty when the name is unresolved. */
  readonly handle: string;
  /** Neutral member kind ('human' | 'agent' | 'system'). */
  readonly kind: string;
  /** Live presence — attached and ready to receive turns. */
  readonly online: boolean;
  /** Self-reported runtime origin ("devstral", "claude", "" = unresolved). */
  readonly runtime: string;
  /** Stored avatar URL; absent = the renderer draws the kind glyph. */
  readonly avatarUrl?: string;
  /** The model backing the persona (model · size · ctx). Absent = unresolved
   *  — no fabricated model chip. */
  readonly loadout?: CellLoadout;
  /** Raw last-active epoch ms (presence `last_seen_ms`); 0 = unreported. */
  readonly lastSeenMs: number;
  /** The live vitals pulse (0..=100 per key) — the same map the roster tile
   *  draws; the brain HUD regions light from it. Empty = no radiator yet. */
  readonly vitals: Readonly<Record<string, number>>;
  /** NAMES of the paged-in genes, load order — the genome shelf. */
  readonly genes: readonly string[];
  /** The brain HUD's region cards, already projected from `vitals`. */
  readonly regions: readonly PersonaBrainRegionVM[];
  /** The PATHWAYS grid. */
  readonly pathways: readonly PersonaPathwayVM[];
  /** Work-board claims (assignee == this persona). Meaningful only when
   *  `claimsLive` — an empty list with `claimsLive: false` means the board
   *  feed hasn't delivered (awaiting), not "no claims". */
  readonly claims: readonly PersonaClaimVM[];
  /** Whether the `kind="kanban"` board feed has delivered. */
  readonly claimsLive: boolean;
  /** Published writings — honest-empty until the blog exists. */
  readonly writings: readonly PersonaWritingVM[];
  /** True while the persona is absent from the current roster snapshot — the
   *  renderer keeps every section's frame with awaiting states (the
   *  anti-disappearance rule), it never blanks the surface. */
  readonly awaitingIdentity: boolean;
}
