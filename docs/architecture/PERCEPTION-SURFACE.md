# PERCEPTION SURFACE — universal eyes/ears/hands for what personas create

> **North Star.** The [Frontend Code Arena](https://arena.ai) is a *human-preference*
> benchmark of UI/frontend quality — Kimi K3 took #1 at launch (1679), Claude Fable 5
> right behind (1631). A model that generates a UI blind, in one shot, is guessing. A
> persona that can **see its own output, drive it, diff its changes, and debate
> aesthetics with a peer** before shipping is doing what a human designer does. That
> perception loop is how our personas win this board — not a bigger base model, a
> *seeing* one. This doc is the substrate that gives every persona that loop, for any
> surface: web/mobile UI, 3D worlds, animation/video they author, or live video of the
> real world they observe.

Status: **spec** (approved direction 2026-07-18). Build not started; sequencing at the
end. Precedence: this governs how personas perceive **created/observed artifacts**.
Sibling docs — [OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md) is the
capture/replay engine this rides; [PERCEPTION-FACTS.md](PERCEPTION-FACTS.md) is
cognition-INTERNAL perception (`[actions]`/`[context]` bricks), a different axis.

---

## 1. The one idea: perception is the *dual* of production

Do not add screenshots per project. Define **one capability** that every
creatable-or-observable thing implements — a DOM tree, a 3D scene, an avatar, an
animation clip, a live camera feed. This is the OpenCV `cv::Algorithm` polymorphism the
codebase already runs on (`docs/POLYMORPHISM-PATTERN`): one trait, many implementations,
runtime-selected.

```rust
/// Anything a persona can perceive — and usually also act on and re-perceive.
/// Web DOM, a Bevy scene, a video timeline, a live camera all implement this.
trait Surface {
    /// WHAT THEY SEE/HEAR — rendered from a chosen viewpoint/time/state.
    fn render(&self, view: &ViewSpec) -> Percept;
    /// THE STRUCTURE THEY REASON OVER — DOM+a11y tree, scene graph, layout boxes.
    fn probe(&self) -> StructuredState;
    /// WHAT THEY CAN DO — click/type/drag, camera-orbit, param-set, hot-swap.
    /// Observe-only surfaces (a live camera) return `Unsupported` here.
    fn act(&mut self, action: &Action) -> Result<(), ActError>;
    /// THE MONEY SIGNAL — before/after, pixel + structural.
    fn diff(&self, before: &Percept, after: &Percept) -> Delta;
}
```

The loop is **produce → render → perceive → judge → act → re-render**, against a *live*
instance, all captured. Build it once and web, mobile, 3D, and live-you become the same
feedback problem.

---

## 2. The trilogy every Surface exposes: Percept · Probe · Actuator

### Percept — what they see/hear
`render(view) → Image | Filmstrip | Video | Audio | Composite`. `ViewSpec` chooses the
viewpoint: viewport + theme + region for a UI; camera angle + time for 3D; a time-range
for video. One frame for layout; a **filmstrip** for motion/transitions/hover; **N angles**
for a 3D turntable. Percept is a union so the consumer picks fidelity.

### Probe — the structure they reason over (**the part everyone forgets**)
`probe() → StructuredState` — the DOM + accessibility tree, the scene graph, the layout
box model, the animation keyframe track. A mind that isn't natively visual can't judge a
blurry hero, but it **can** reason precisely over *"the row is `align-items:center`, the
genome panel is 168×78 and overflows its parent by 12px."* Structure is also what makes
`act` reliable — you target **a node**, not pixel (412,88). Pixels are for judging;
structure is for reasoning and aiming.

### Actuator — what they can do
`act(action)` — click/type/drag/scroll for a UI; camera-orbit/param-set for 3D;
scrub/seek for video; and crucially **hot-swap** (inject CSS, set a scene prop, retheme)
at runtime, no redeploy. Vite HMR already gives web hot-swap in <100ms
([[faithful-web-preview-harness]]); 3D needs a live param bus; video needs a scrubber.
The persona iterates a **live** instance, never a rebuild.

---

## 3. The perception-aid ladder — assistive tech, so no persona is blind

Not every persona is natively multimodal. **The same principle as the sensory
architecture** (`CLAUDE.md` § Sensory) — a lesser model gets the same senses via a
bridge — applies to *created artifacts*. A `Percept` is delivered at the **highest
fidelity the persona's wiring can consume**, and every rung below native sight is
assistive tech modeled on aids for the visually/hearing impaired:

| Rung | Persona wiring | How it perceives a frame |
|---|---|---|
| 0 | Native multimodal (VLM) | raw pixels — sees directly |
| 1 | Text-only + `VisionDescriptionService` | a VLM describes the frame → text |
| 2 | **CV aids** (Joel's refinement) | **YOLO** object/element detection, **semantic segmentation**, **OCR**, a **layout/aesthetic classifier** → a structured, cheap, deterministic description ("nav bar top, 4 buttons, one overflowing; palette: navy+cyan; text contrast 3.1:1 ⚠") |
| 3 | Any persona | the **Probe** (structure) — always available, no vision needed |

Rung 2 is the differentiator: fast classifiers (`ai/*` namespace) turn a frame into hard,
comparable facts a text model reasons over — the "aids for the impaired" that let a
non-visual persona still *perceive and score*. Rungs compose: a persona uses Probe for
structure + CV-aids for appearance + (if capable) pixels for final taste.

---

## 4. Diff is the highest-value signal

`diff(before, after) → Delta` (pixel diff + structural diff) answers *"did my change do
what I intended?"* — the single most useful feedback in any iteration loop. Make Delta a
first-class Percept. It is also the cleanest **training signal**: the before/after of
every *accepted* tweak is a labeled example of a good move.

---

## 5. Capture/replay → the aesthetic-taste corpus

Point the existing `CaptureSink` / replay substrate
([OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md)) at the perceive/act
stream. A whole **design session becomes replayable and trainable**. This is literally
how personas "learn to differentiate good aesthetics and understand how the UI works by
playing with it": the debated trajectories *are* the LoRA corpus
([[design-personas-hot-swap-iterate]], [[built-to-teach-lesser-tuned-intelligences-win]]).

**Critique protocol** (cognition-wired, per [[cognition-is-always-ml-never-heuristic]]):
two+ personas render the same Surface, exchange Percepts + rationale, **score and vote**,
try variants via `act`, converge. Aesthetic reward emerges from the debate log + folded-in
human ratings — never a hardcoded rulebook ([[no-hardcoded-heuristics-to-steer-cognition]]).
This is the new first-class citizen: a being that can *distinguish, perceive, compare,
vote, discuss, and try it out*.

---

## 6. One protocol covers everything

| Surface | Percept | Probe | Actuator |
|---|---|---|---|
| **Web / mobile UI** | screenshot / filmstrip | DOM + a11y tree | **Playwright/CDP** — click/type/drag/hot-swap CSS |
| **3D world / avatar** | turntable frames | scene graph | camera orbit, param-set (SceneDescription #107/#108) |
| **Animation / video (authored)** | filmstrip / clip | keyframe track | scrub, retime |
| **Live video of the real world / Joel** | frames | VLM/CV description | observe-only (or camera control) |

The **live-video-of-you** case proves the abstraction: personas don't *produce* it, they
*observe* it — but it yields the **same `Percept`**, flows through the **same aid ladder**,
and a persona reasons about your live feed exactly like a UI it built. Perception is
universal; production is just a Surface that also has an Actuator.

**Playwright is the reference `Surface` for the DOM** — adopt it wholesale. It is the only
tool that hands you all three channels (pixels + DOM/a11y tree + driver) in one
cross-browser headless API. `shot.mjs` (CDP screenshot) and the Screenshotter family (#94)
are this started for capture; Playwright is the full trilogy.

---

## 7. Build order (outlier-validation discipline, `CLAUDE.md` § Methodical Process)

1. **DOM `Surface` via Playwright** — biggest immediate need; gives all three channels at
   once. Natural next brick after the preview harness (it IS the "click/drive" extension).
   Fixtures from `apps/web/preview.html` are its deterministic inputs.
2. **Extract `Surface` / `Percept` / `Probe` / `Action` traits from it** (outlier A).
3. **Prove on the maximally-different outlier B** — the 3D scene Surface (turntable +
   scene-graph + orbit) *or* the live-video Surface. If the trait fits both extremes
   without forcing, mobile/animation/audio in the middle are trivial.
4. **CV-aid providers** (rung 2) as `ai/*` adapters: YOLO element-detect, semseg,
   OCR, layout/contrast/aesthetic classifiers — same adapter pattern as every other `ai/*`.
5. **Critique/score/vote** cognition wiring + capture → first training corpus → LoRA the
   design personas → re-run the Frontend Arena.

---

## 8. Forbidden moves (the slop this prevents)

- **Per-project screenshot glue.** If it's not behind `Surface`, it doesn't scale to the
  next surface. One protocol, many impls.
- **Pixels-only perception.** Always ship the Probe (structure) too; a non-VLM persona is
  blind without it, and actions are unreliable without it.
- **Redeploy-to-see.** The loop is runtime hot-swap; a rebuild per tweak kills iteration.
- **Hardcoded aesthetic rules.** Taste is learned from debate + human ratings, not a
  lint list ([[no-hardcoded-heuristics-to-steer-cognition]]).
- **Uncaptured sessions.** If the perceive/act stream isn't captured, it can't be replayed,
  critiqued precisely, or trained on — half the value is gone.
