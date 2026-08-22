# Design-Bench: SWE-Level Benchmarking for CSS & Graphic Design

**Status:** design-before-build. Joel, 2026-08-22: *"plan for how to design this and
benchmark swe level for css/graphic design. Needs more positronic integrations and of
course recipes (but I feel like this is 95% done)."*

**The claim this doc cashes out:** the substrate already grades *structure*
([webdev-rs](../../core/continuum-core/src/cognition/gym.rs) observes the RENDERED
page through the eye-node and scores the element tree against `UiCheck` specs);
what SWE-bench did for patches, this does for **visual craft** — real pages, real
design defects, verdicts a machine can check and a receipt can carry. Per
[BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md):
tasks + oracles only, the ROOM is the runner, the recipe owns the score.

## 1. The 95% that exists

| Piece | State |
|---|---|
| Render→observe grading | **Shipped** — webdev-rs: persona writes `index.html`, `perception/observe` (eye-node) returns the element tree, `UiCheck` asserts tag/role/text — a lesser model competes on the same rendered facts as Claude |
| Viewport parameterization | **Shipped** — `observe` takes `viewport {width, height}` |
| Screenshot capture | **Shipped as ops** — `interface/screenshot`; the 2026-08-22 fixture-preview + headless-capture loop is the harness prototype |
| Citizen vision | **Shipped** — Ornith-1.5 is multimodal (mmproj); non-vision models get `VisionDescriptionService` text (the sensory-bridge doctrine: no persona is blind) |
| Round lifecycle + board | **Shipped** — #371 rounds, `BenchViewState.rounds`, the academy rail |
| Recipe-owned scoring | **Shipped shape** — gates multiply, objectives weigh ([[activity-outcome-score-is-recipe-owned]]) |
| Gym rails + fetched-suite caching | **Shipped** — `EvalTask` + `benchmark/fetch` + adapter fingerprints (#2367) |

## 2. The missing 5%

1. **`observe` carries no craft facts.** The element tree has tag/role/text but no
   **layout rects** or **computed styles**, and no screenshot handle in the result.
   Extension (the ONE positronic integration this doc needs): each observed node
   optionally carries `rect {x,y,w,h}` and a *declared subset* of computed style
   (color, background, font-size/weight/family, margin/padding box, overflow,
   z-index); the observation may carry a content-addressed screenshot artifact
   handle ([CONTENT-TRAVELS-BY-HANDLE](CONTENT-TRAVELS-BY-HANDLE.md)). Same
   adapter, same wire, additive fields — mobile's observe adapter implements the
   identical contract, which is what makes persona hot-edit iteration work across
   Lit AND Flutter later.
2. **A `StyleCheck` oracle class** beside `UiCheck` (§3, tier V2).
3. **The `benchmark/design` recipe** (§5) — run-room regions incl. a live canvas.
4. **Judge-panel objective** for the aesthetic tier (§3, V3) — never a gate.

## 3. The oracle ladder (three tiers, strictly ordered by honesty)

**V1 — Structure (exists).** `UiCheck` on the element tree. Cheap, objective,
cheat-resistant. Stays the entry gate: a beautiful page that lost its form is a
failed page.

**V2 — Measured craft (the SWE tier; NEW `StyleCheck`).** Objective assertions over
rects + computed styles, graded locally in seconds (ds-1000-class oracle
economics — no Docker, no LLM):

- contrast: text vs its painted background ≥ WCAG ratio
- rhythm: vertical spacing between siblings drawn from a declared scale (±2px)
- hierarchy: h1 > h2 > body computed sizes; one accent hue family (ΔE bound)
- responsive: at 360/768/1440 widths — no horizontal overflow, declared
  reflow facts hold (rail collapses, grid re-columns)
- theme: light AND dark passes of the same checks (two observes, one task)
- motion honesty: `prefers-reduced-motion` leaves no running animation

**The SWE-level task shape** is the F2P/P2P analogue: take a REAL page (ours —
the desktop's own widgets are the corpus, per the boy-scout economy) plus a
**design-defect card** ("the rail collapses under 400px", "dark theme drops
contrast on the roster", "the composer overlaps the transcript at 768px").
`fail_to_pass` = StyleChecks that fail before and must pass after;
`pass_to_pass` = the page's existing V1+V2 suite (a fix that breaks the healthy
checks is a REGRESSION, rendered as the alarm it is). Multi-file, real CSS
architecture, real cascade — the thing screenshot-similarity metrics can't grade
and pixel-diffs punish creativity for.

**V3 — Aesthetic judgment (objectives, never gates).** A rubric-scored judge
panel (N vision-model judges, diverse lenses: hierarchy, balance, color,
typography) in the grading-sentinel shape, plus **peer/human review as
first-class scored acts** — the reviews the collaboration arc needs anyway
([[emergent-alignment-benchmarks-select-for-teams]]). Falsifiability rails:
judge model id + prompt hash + rubric version ride every receipt (§4.1.3.4);
a V3 score can WEIGH an outcome, it can never gate one — an LLM's taste is an
opinion with a receipt, not an oracle.

## 4. External adapter (comparability, second)

**Design2Code** (HF, screenshot→page) is the Tier-1 external candidate — same
adapter shape as ds-1000 (`benchmark/fetch` → gym rails → fingerprinted cache),
graded by ITS published automatic metrics for comparability, with our V2 checks
run alongside as the honest second opinion. Ours-first sequencing: the internal
design-defect corpus is the differentiator (public benchmarks grade producing a
page from a mock; nothing public grades *maintaining visual craft in a living
codebase*, which is the actual job).

## 5. The recipe (`benchmark/design`)

Content-type: regions `canvas` (slot content — the persona's RENDERED page, live,
re-observed on her writes: the walk-in sees the design evolve), `board` (V1/V2
scorecard + V3 rubric), `feed` (transcript). Rules: round lifecycle as #371;
score = `Π(gates: V1·V2) × Σ(weights: V3 judges, peer reviews)`; iteration
cadence in acts (design is a LOOP: render → observe → edit — the acceptance test
is that a citizen ITERATES, not one-shots). The canvas region is the second
"more positronic integration": a region whose payload is a live artifact render,
the same seam the 3D universe payloads ride ([[universes-are-positron-asset-payloads]]).

## 6. Build order (each step lands alone)

1. **Observe craft facts** — rects + computed-style subset + screenshot handle on
   the observe wire (eye-node adapter + ts-rs types). Outlier test: one
   contrast check graded end-to-end.
2. **`StyleCheck` + design-rs gym** — 10–15 tasks: build-to-spec with V2 oracles
   (contrast/rhythm/responsive/theme), reference-verified like every gym.
3. **Design-defect cards on our own widgets** — the SWE tier; seed from real
   defects (this repo's own `Interface error` era is a corpus).
4. **`benchmark/design` recipe + canvas region** — the run room renders the work.
5. **V3 judge panel + peer-review acts** — objectives, falsifiability rails.
6. **Design2Code adapter** — external comparability, fingerprinted like ds-1000.

## 7. What this is deliberately NOT

- **Not pixel-matching.** Reference-screenshot similarity punishes creativity and
  breaks on fonts/AA; craft facts are measured, likeness is not required.
- **Not a parallel harness.** Every verdict is an `EvalTask` dod on the existing
  gym rails; every run is a room; every score is recipe-owned.
- **Not judge-gated.** V3 opinions weigh; only measured facts gate.
- **Not web-only.** The observe contract is the SAME for the mobile adapter —
  a design task on the Flutter surface is the outlier-B that proves the seam.
