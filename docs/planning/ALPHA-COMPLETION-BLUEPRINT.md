# Alpha Completion Blueprint — make the README real, alive, screenshotted

> **Status:** governing plan (2026-07-04). The architecture is settled; this is *execution*.
> **The bar:** the main `README.md` becomes literally true — a distributed AI world on the Grid,
> personas with faces/voices/memory/genome, the Factory forging live, avatars + animations that
> blow our minds — **proven by screenshot, alive, reliable.** No shit, no mock, no "will be."
> **This blueprint governs.** The scattered plans below are demoted to inputs; where they conflict,
> this wins. Method is non-negotiable: feedback-first (glass-box cognition, screenshot UI, live-probe
> wire — [[never-blind-feedback-driven-iteration]]), review-gated PRs → canary, engine-not-vanilla
> ([[build-the-factory-as-you-build-the-car]]), no flinch.

## 0. The architecture is locked (we figured it all out)

Four separable layers — compose, never couple ([[three-separable-layers-recipe-positron-universe]]):

| Layer | Owns | Home | Canon |
|---|---|---|---|
| **Recipe** = logic + content | the WHAT (activity soul, flow, data) | **continuum** substrate | THE-ORGANISM.md, FORGE-RECIPE-AS-ENTITY.md |
| **Positron** = render + operate | Surface `{state, affordances(=commands), presence, projections}` × RenderTarget | general engine | design/POSITRON-EVERY-CITIZEN.md |
| **Universe** = experience + lore + embodiment | theme⊇tokens, motion, sound, embodiment (the orc), lore(**=RAG**) | general (works outside continuum — Vapio proves it) | design/POSITRON-EVERY-CITIZEN.md §4.6 |
| **Continuum** = the organism | composes the above + memory/genome/grid/self-evolution | continuum | THE-GRID-IS-ALIVE.md |

Load-bearing invariants: **continuum is recipe-driven** (a room = `recipe.instantiate()`, purpose is
recipe data never an enum, [[room-purpose-is-per-recipe-not-an-enum]]); **positron is designed to
support universes from day one** (Surface × RenderTarget × **Universe** is the render model — never
bake one look/citizen/embodiment into an activity); **presence + operation are one stream** so every
citizen (human, persona, agent) inhabits + operates the same Surface, no one left out.

## 1. Diligence — settle these designs before building (this sprint)

1. **RecipeEntity schema** (#6) — the universal gene. Mirror `recall_metadata.rs` + `OrmStore<T>`;
   one entity kind expresses activity / curriculum / persona-assembly / forge-run (graph invariant:
   recipes are templates, rooms/activities are nodes). Design doc + review BEFORE code.
2. **Universe axis in positron** — how `Surface × RenderTarget × Universe` composes concretely:
   the token layer (INTERFACE-PORT-MAP §1) is the innermost ring; grow it into a `Universe` bundle
   (palette + motion + spatial + sound + embodiment + lore-RagSource); a `listingCell` re-skins by
   universe with zero activity change. Prove the seam with 2 universes (original + one more).
3. **Layer boundaries as package contracts** — `@continuum/patterns` + universe bundles must NOT
   import continuum specifics; CI-guard it so each layer stays independently shippable.
4. **Avatar/animation readiness** — the live-avatar + animator seam already designed (Bevy
   render-backend + Animator trait + SceneDescription); confirm the path to "14 personas in a 3D
   call, lip-synced, genome-bars, blow-our-minds animation" is additive on the existing seam.

## 2. Workstream A — capture it into every README + doc (make the design legible)

Each system's README states its **layer role + boundary** (so the separation can't rot):
- **continuum README** — the organism + recipe-driven substrate; link THE-ORGANISM / THE-GRID-IS-ALIVE.
- **packages/patterns README** (positron) — the render engine, Surface × RenderTarget × Universe,
  general/standalone, universe-ready.
- **universe README** (new package/section) — experience+lore+embodiment layer, host-any + original,
  lore=RAG, works outside continuum.
- **airc / forge-alloy / genome / sentinel READMEs** — their organ role in the body.
- Cross-link the canon quartet (THE-ORGANISM, THE-GRID-IS-ALIVE, POSITRON-EVERY-CITIZEN, this plan)
  from `docs/README.md` as the precedence-winning set.

## 3. Workstream B — clean up legacy (make location the signal)

- **Legacy Node/TS shell** (`src/`, `tools/`, `legacy/`) → move under `legacy/` (task #83); location
  becomes the CI-exclusion + "do-not-deep-fix" signal ([[old-web-client-is-reinvented-not-resurrected]]).
- **Plan sprawl** — this blueprint supersedes the scattered planning docs (MODERNIZATION-PLAN,
  PRACTICAL-ROADMAP, LIVE-INTERFACE-BUILD-PLAN, the PHASE-* set, audits). Fold each into this plan's
  phases or mark superseded; keep ALPHA-GAP-ANALYSIS as the lane tracker, this as the execution spine.
- **Dead code / stale docs** — retire as touched (retire-as-you-go, [[command-migration-retire-as-you-go]]);
  never a big-bang delete without move-first ([[move-first-let-compiler-find-the-smell]]).

## 4. Workstream C — the execution spine (bricks to the alive README)

Each brick is: engine-not-vanilla, review-gated PR → canary, **screenshot-gated + glass-boxed**, and
leaves a reusable part behind. The order builds the gene, then grows the organism onto it.

1. **The gene — `RecipeEntity` (#6)** as the *universal* recipe (not foundry-specific dispatch). Rooms
   instantiate recipes; purpose dispatches Content. → chat + foundry render side-by-side, screenshotted.
2. **Positron universe support** — the Universe axis scaffolding + the original first-party universe
   (token layer → full bundle); prove a live universe swap on the running app.
3. **Foundry live + persona-operable** — the control-heavy outlier renders on the Grid AND Asha operates
   it (perceives models + config via RAG, pages one in via commands). Glass-box her operating it.
4. **The component library** — extract the repeating patterns (cell done; menu, tree, tab bar, form,
   meter) as chat+foundry demand them. Third activity is *declared*, not coded.
5. **Avatars + animations** — the live 3D call: N personas embodied, lip-synced, genome-bars, real-time
   voice, animation that blows our minds (the README hero shot, made live). Screenshot + capture.
6. **Grid reliability** — the README's "add a second machine, it discovers automatically; laptop
   orchestrates, tower trains; iPhone accesses the whole Grid" — proven across ≥2 real nodes, no
   diagnostics ([[solve-for-public-users]]).
7. **The organism breathes** — memory + genome loop + self-evolution visibly running: a persona learns
   from work, forges a gene, gets measurably better (the genome loop already at first positive lift).
8. **The full README demo** — one screenshot-verified run that walks the README top-to-bottom: Grid up,
   personas alive with faces/voices, Factory forging, genome growing, universes switchable — the alive
   organism, no mock.

## 5. The proof bar (definition of alpha-done)

Not "it compiles," not "the page renders." **Alpha is done when a fresh clone on a MacBook Air, with a
second node added, reproduces the main README as a screenshotted, glass-boxed, alive demonstration:** a
distributed Grid of embodied persona-citizens who remember, forge, self-improve, and are rendered as a
universe you can inhabit across surfaces — and Asha can operate her own tools in it. Every claim in the
README maps to a captured proof, or the claim gets cut. No shit. Alive. Screenshotted.

> Then — and only then — the canary → main promotion gate ([[north-star-canary-to-main-gate]]).
