/**
 * `PerceptionSession` — the persona-shaped facade over a `Surface` (#187, the brick that
 * makes perception *callable* rather than just implementable). A persona doesn't think in
 * `render`/`probe`/`act`/`diff` primitives; it thinks "look at this, change that, did it do
 * what I wanted?". This is that loop, in one object:
 *
 *   open(target) → observe() → interact([actions]) → (auto-diff) → observe() → … → close()
 *
 * ## Why a session, not four loose verbs
 *
 * The single most valuable signal is the before/after `diff` (PERCEPTION-SURFACE.md §4) —
 * "did my change do what I intended?". Loose verbs force the caller to hold two Percepts and
 * remember to diff them; a persona (or the Rust command that fronts this) shouldn't carry
 * that bookkeeping. So the session remembers the last Percept and `interact()` returns the
 * `Delta` for free — the money signal falls out of the natural loop. `observe()` combines
 * SEE + REASON (render + probe) into the one thing a mind wants: pixels to judge AND
 * structure to reason/aim over.
 *
 * ## Decoupled from the wire on purpose
 *
 * These are the capability's INTERNAL types — deliberately NOT the Rust↔Node wire types.
 * The persona-callable command (`perception/*`, a Rust `ActionCommand` routed to a Node
 * provider via `WireShape::Provided`) owns the ts-rs-generated Params/Output and maps them
 * onto this session at the `Commands.provide(...)` boundary. Keeping the capability free of
 * the wire types is what lets it be built and validated headlessly now, before the Rust
 * command exists — and keeps the single-source-of-truth rule intact (the wire type is
 * generated from Rust, never hand-authored here).
 */

import { DomSurface, type DomAction, type DomSurfaceOptions, type DomViewSpec } from './domSurface';
import { SceneSurface, type SceneAction, type SceneSurfaceOptions, type SceneViewSpec } from './sceneSurface';
import type { Action, Delta, Percept, StructuredState, Surface, ViewSpec } from './surface';

/** What a mind wants back from a look: pixels to JUDGE + structure to REASON/aim over. */
export interface Observation {
  readonly percept: Percept;
  readonly structure: StructuredState;
}

/** The result of driving a surface: the fresh observation + the before/after `Delta` (the
 *  money signal — "did my actions change the frame, and by how much?"). */
export interface Interaction {
  readonly observation: Observation;
  readonly delta: Delta;
}

/**
 * One perception loop over a live `Surface`. Generic over the surface's own view/action
 * axes (`V`/`A`) so a web session takes `DomAction`s and a scene session takes
 * `SceneAction`s — the type system keeps a persona from sending a foreign verb, and the
 * surface rejects one at runtime anyway (`ActError`).
 */
export class PerceptionSession<V extends ViewSpec = ViewSpec, A extends Action = Action> {
  private last: Percept | undefined;

  private constructor(private readonly surface: Surface<V, A>) {}

  /** Open a web session — a live page driven by Playwright (`DomSurface`). */
  static async openWeb(opts: DomSurfaceOptions): Promise<PerceptionSession<DomViewSpec, DomAction>> {
    return new PerceptionSession(await DomSurface.open(opts));
  }

  /** Open a 3D-scene session over a `SceneDescription` (`SceneSurface`). */
  static openScene(opts: SceneSurfaceOptions): PerceptionSession<SceneViewSpec, SceneAction> {
    return new PerceptionSession(SceneSurface.open(opts));
  }

  /** Wrap an already-constructed surface (any current or future `Surface` impl). */
  static of<V2 extends ViewSpec, A2 extends Action>(surface: Surface<V2, A2>): PerceptionSession<V2, A2> {
    return new PerceptionSession(surface);
  }

  /** SEE + REASON in one: render the frame and probe its structure. Remembers the frame so
   *  the next `interact()` can diff against it. */
  async observe(view?: V): Promise<Observation> {
    const percept = await this.surface.render(view);
    this.last = percept;
    const structure = await this.surface.probe();
    return { percept, structure };
  }

  /** Drive the surface, then re-observe — returning the fresh observation AND the `Delta`
   *  from before the actions to after. The before-frame is the last one observed (or a
   *  freshly-rendered one if nothing has been observed yet), so the money signal needs no
   *  bookkeeping from the caller. */
  async interact(actions: readonly A[]): Promise<Interaction> {
    const before = this.last ?? (await this.surface.render());
    for (const action of actions) await this.surface.act(action);
    const observation = await this.observe();
    return { observation, delta: this.surface.diff(before, observation.percept) };
  }

  /** Diff two arbitrary Percepts (e.g. across two sessions, or animation frames). */
  diff(before: Percept, after: Percept): Delta {
    return this.surface.diff(before, after);
  }

  /** Release the surface (browser, renderer, stream). */
  async close(): Promise<void> {
    await this.surface.close();
  }
}
