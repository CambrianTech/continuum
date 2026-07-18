# @continuum/perception

Universal eyes/ears/hands for what personas create or observe (#187). Full spec:
[`docs/architecture/PERCEPTION-SURFACE.md`](../../docs/architecture/PERCEPTION-SURFACE.md).

## The one idea

Perception is the **dual of production**. Every creatable-or-observable thing — a web page,
a 3D scene, an animation, a live camera — implements ONE `Surface` trait exposing three
channels plus a diff:

| Channel | Method | What it is |
|---|---|---|
| **SEE** | `render(view) → Percept` | pixels to judge (image today, filmstrip for motion) |
| **REASON** | `probe() → StructuredState` | the structure to reason over + aim actions at (DOM/a11y tree, scene graph) |
| **ACT** | `act(action)` | drive it — click/type/hot-swap CSS, orbit camera, move a node |
| **JUDGE** | `diff(before, after) → Delta` | did my change do what I intended? (the money signal + training label) |

`PerceptionSession` wraps a Surface as the loop a persona actually thinks in:
`open(target) → observe() → interact([acts]) → (auto-diff) → observe() → close()`.

## What the outlier-validation exercise proved

Two implementations span the extremes — `DomSurface` (web, via Playwright) and
`SceneSurface` (a Bevy-shaped 3D scene over the real `SceneDescription`). Fitting BOTH to
one trait without forcing showed exactly which parts of perception are universal:

- **SEE, JUDGE, and REASON are universal** — both surfaces produce the identical
  `image/png` `Percept`, consume the identical pixel `Delta` (one shared `imageDiff`), and
  emit the same `ProbeNode` tree shape.
- **Only VIEW-hints (`ViewSpec`) and ACT-verbs (`Action`) are surface-flavored** — a web
  view has `selector/theme`; a 3D view has a `camera`. So the trait is generic over exactly
  those two axes: `Surface<V, A>`, each surface owning its own view/action union (no central
  god-enum). `setViewport` is the one universal concrete actuator; `url` is optional.

## The architecture that matters: the core is agnostic and browserless

**The headless Rust core never renders anything and never assumes a browser, a display, or a
GPU is present** — a datacenter rack instance has none of those. It addresses perception by
capability NAME (`perception/observe`) and routes the call to whatever peer in the mesh
actually has eyes for that surface. This is the `WireShape::Provided` seam (mirrors
`interface/screenshot`): **one command name, N adapters**, core stays pure.

Therefore:

- **A `Surface` is an ADAPTER, not a core dependency.** `DomSurface` lives wherever a
  browser exists — a laptop client offering its live tab, or a dedicated render-worker node
  that *chose* to install Playwright's Chromium. The core never links it, never spawns it,
  never calls `findChromium()`. `SceneSurface`/a future `BevySurface` are offered by GPU
  nodes; a lightweight core doesn't have them either.
- **The core routes to capability, not location.** A browserless node delegates render to a
  peer that can render and gets back the same `Percept`. Perception is universal; only
  *where the pixels are produced* varies.
- **The aid ladder keeps a browserless node from going blind.** SEE routes out, but REASON
  is portable — the **Probe** needs no display, and CV-aids (YOLO/semseg/OCR, rung 2) run
  over whatever bytes a peer hands back. That's the whole reason the trait separates SEE
  (routable) from REASON (portable).

### Forbidden move

**Do not make the core depend on a browser/renderer being installed.** If perception ever
`spawn`s Chromium from the Rust core, or a code path assumes a local display/GPU, it will
not run on a headless rack — which is most production installs. Perception is `Provided`:
the core asks, an eye-node answers.

## Files

- `surface.ts` — the neutral `Surface<V, A>` contract + `Percept`/`ProbeNode`/`Delta`.
- `imageDiff.ts` — the ONE pixel diff every surface shares (JUDGE is universal).
- `domSurface.ts` — the web adapter (outlier A): Playwright screenshot + DOM/a11y probe + driver.
- `sceneSurface.ts` — the 3D adapter (outlier B): a deterministic software projector over
  `SceneDescription`, standing in for Bevy's offscreen readback (same trait, swap `render()`).
- `session.ts` — `PerceptionSession`, the persona loop over any surface.

## Status & next

Steps 1–3 of the spec's build order are done (trait + both outlier adapters + session), all
headless-testable. Next: expose `perception/observe` as a `Provided` `CommandSpec` (Rust)
routed to a Node provider that hosts these adapters on eye-nodes; then CV-aid adapters
(rung 2), then critique/score/vote → capture → train the design personas → re-run the
Frontend Code Arena.
