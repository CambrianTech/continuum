/**
 * The UNIVERSE framework — ambient world-skins as first-class objects.
 *
 * ## The design (OOP, the house registry pattern)
 *
 * A universe is a CLASS implementing [`Universe`], registered once in
 * [`universeRegistry`] (exactly like `webContentRegistry` /
 * `webWidgetRegistry` — one registry per open axis, no switch statements).
 * The host widget resolves a key, calls `mount()`, and from then on speaks
 * ONLY the [`UniverseInstance`] interface:
 *
 *   - `element`   — the DOM node the host places behind its panels;
 *   - `update()`  — the ambient world-state (citizens, work energy) pushed
 *                   each time the host's data changes;
 *   - `onFact()`  — one call per REAL event (message landed, act radiated,
 *                   verdict settled). Spectacle is fact-driven by contract:
 *                   a universe never invents motion, it responds to truth.
 *   - `dispose()` — teardown, timers and listeners included.
 *
 * No `querySelector`, no duck-typed method probing, no events guessing at
 * hosts — the interface IS the contract (2026-08-31, after exactly those
 * hacks were called out). Adding a universe = one class + one register
 * call; the host never changes.
 *
 * ## The laws every implementation inherits
 *
 * - PAINT BUDGET: no free-running rAF unless throttled + visibility-paused;
 *   no per-frame shadowBlur/full-screen gradients (the cosmos post-mortem).
 * - TRUTH-DRIVEN: motion beyond gentle idle ambience must trace to an
 *   `onFact`/`update` input.
 * - `prefers-reduced-motion` stills everything.
 */

/** A real event the interface just learned — the only fuel for spectacle. */
export type UniverseFactKind = 'message' | 'act' | 'verdict';

export interface UniverseFact {
  readonly kind: UniverseFactKind;
}

/** Ambient world-state the host pushes when its data changes. */
export interface UniverseFrame {
  /** The focused room's members (name + liveness) — worlds may personify. */
  readonly citizens: readonly { readonly name: string; readonly active: boolean }[];
  /** Normalized 0..1 work intensity (live runs over capacity). */
  readonly energy: number;
}

/** A mounted universe — what the host holds and speaks to. */
export interface UniverseInstance {
  readonly element: HTMLElement;
  update(frame: UniverseFrame): void;
  onFact(fact: UniverseFact): void;
  dispose(): void;
}

/** A registered universe — the factory the registry hands the host. */
export interface Universe {
  /** The `?universe=` key (`'cosmos'`, `'positron'`, …). */
  readonly key: string;
  mount(): UniverseInstance;
}

/** The one registry (no switches, no central lists to edit — register and go). */
class UniverseRegistry {
  private readonly universes = new Map<string, Universe>();

  register(universe: Universe): void {
    this.universes.set(universe.key, universe);
  }

  get(key: string): Universe | undefined {
    return this.universes.get(key);
  }
}

export const universeRegistry = new UniverseRegistry();
