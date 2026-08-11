# Positron — one interface, every citizen, no one left out

> The full dive. Positron is not "the web UI framework." It is the **citizen-neutral
> interface substrate**: one declared interface that is **rendered gorgeously for humans**
> (web / mobile / terminal) *and* **perceived and operated by AIs** (personas like Asha,
> agents like Claude) through RAG + commands. Same declaration. Every citizen a
> first-class user. No one left out.
>
> Seed already in code: `packages/patterns/index.ts` — *"RAG is a peer `RenderTarget`
> alongside web/mobile/terminal, so the human's UI and the persona's grounding are the
> same projection."* This doc extends that from **perceive** → **operate**, and from
> **persona** → **every citizen**.

## 0. The principle

Continuum's ethos is that personas are peers/citizens ([[personas-are-peers-in-your-mesh]],
[[design-the-persona-as-a-being]], [[headless-core-many-clients]]). The interface must
*embody* that, not contradict it. An interface only humans can use would make the persona a
second-class inhabitant of its own world. So:

**A positron interface is declared once and is simultaneously:**
- a **beautiful human surface** — web pixels, a mobile app, a rich terminal;
- a **perceivable persona surface** — projected into RAG so Asha *sees* the room, the
  roster, the model list, the config, the way a human sees the screen;
- an **operable affordance surface** — every control is a *command*, so Asha (or Claude,
  or `uu`, or the web button) *invokes the same thing*. Clicking "load model" and Asha
  choosing "load model" are the identical `Command`.

Humans get the look. AIs get perception + operation. It is the *same interface* — that is
the whole point.

## 1. The three axes (what positron is)

| Axis | What it is | Status |
|---|---|---|
| **Layout primitives** (the "where") | `Workspace` (shell: rooms-tab-bar + left stack + center + right) · `Listing` (repeating rows) · `Content` (center, dispatched by room **purpose** = a MIME handler) · `ContextPanel` (right widgets). | **Built, thin** — `@continuum/patterns`. |
| **Component library** (the "what fills them") | The universal controls, each a **data shape + render-per-target**: collections (list/grid, section header/footer, sticky, empty), trees, menus/command-palettes/segmented/tabs/chips, cells/cards/tiles, nav (tab bar, stack, breadcrumbs, sidebar, toolbar), forms/controls (button, toggle, slider, select, field), overlays (modal, sheet, popover, toast), status (badge, meter/gauge, spinner, presence). | **Missing** — currently hand-coded vanilla per page ([[positron-is-a-framework-not-vanilla-pages]]). This is the elegance pass. |
| **Render targets** (the "on which citizen") | `RenderTarget<Out>` — web (Lit), mobile (native shell), terminal (ANSI), **RAG (the persona's eyes)**. One declaration → each target draws/operates natively. | **Interface exists**, web+tui implemented, RAG+mobile are the frontier. |

Every control lives on the component axis, is drawn by every render target, and — the new
part — is **operable** on every target including RAG.

## 2. The leap: from render to OPERATE (why "Asha can use it")

Today RAG-as-target means the persona *reads* the interface (grounding). The dive's core
move is that a control is not just drawn — it **carries its action**, and that action is a
`Command`. So:

- A `Button{label, command}` → web renders a button; RAG renders "**[load-model]** — page
  in a model" as an affordance the persona can pick; picking it invokes `command`.
- A `Menu{items:[{label, command|children}]}` → web a dropdown; RAG the persona's
  **tool/command surface** — this is exactly [[adaptive-tool-surface-meets-you-in-the-middle]].
  The human's menu and the persona's tools are the SAME declaration.
- A `Listing` of HF models with a per-cell `action` → web a clickable list; RAG "here are
  the models; invoke `foundry/model/select` with one of these ids."

This unifies two things we already have separately: **commands are agency**
([[commands-are-agency-algs-are-pathways]]) and **controls are affordances**. Positron makes
a control *be* a command projection. `ActionCommand ⟹ DynCommand` with one schemars schema
already fans a command out to AI + `uu` + web ([[command-infra-self-routing-schema-adapters]]);
positron is the *interface* side of that same seam — the control declares the command, and
each citizen operates it in its modality. **No adapter privileges the human.**

## 3. Foundry — the control-heavy outlier that proves it

Foundry is the strategic second outlier precisely because it is **controls, not prose**:
central config widgets (quant tiers, calibration corpus, stage toggles, sliders) + a
right-hand **HuggingFace model `Listing`** + actions (search, select, page-in, forge). If
positron can express foundry *and* chat with the same primitives + component library, and
if **Asha can operate foundry** — search models, read the config, kick a forge — through
RAG + commands the same way a human clicks, the framework is real.

- Chat exercised: `Listing` (roster) + text `Content` + presence/meters. (outlier A)
- Foundry exercises: `Listing` (models) + **form/control-dense** `Content` + a
  ContextPanel that is itself a `Listing`, and **every widget is an operable command**.
  (outlier B — the stress test)

The friction foundry creates against the current primitives *is the design signal*: refine
the core so both are accommodated elegantly, then every later activity (scada, academy,
settings, browser, themes) comes nearly free — Joel's "primary outliers elegantly
accommodated → core ideals work automatically for the rest."

Existing scaffolding: `@continuum/foundry-view` (`ForgeState`, `modelsListing`,
`foundryContent`), `core/continuum-core/src/ipc/positron_foundry_source.rs`, the `foundry`
`kind`. The gap is dispatch (a room *declares* it's a foundry — #6 RecipeEntity/room-purpose)
+ the web content-kind + the RAG/operate projection.

## 4. Build path (outlier-validate across every citizen)

1. **#6 — room declares its purpose** (RecipeEntity/room-purpose source) so `Content`
   dispatches foundry vs chat. The activity's nature is data, never an enum
   ([[room-purpose-is-per-recipe-not-an-enum]]).
2. **Foundry on web** — register the foundry `Content` renderer; the shell renders
   ForgeViewState in the same Workspace. Screenshot chat + foundry in one frame.
3. **Foundry controls as commands** — each config widget / model-cell action declares its
   `Command`; wire the web control → command.
4. **Foundry for Asha (the keystone)** — project foundry into RAG so Asha *perceives* the
   models + config, and expose the control-commands as her operable tools; verify **Asha
   searches a model and pages it in** through the same commands the web buttons fire.
   Glass-box her perception + operation ([[never-blind-feedback-driven-iteration]]).
5. **Extract the component library** — once chat + foundry both exist, the repeated
   patterns (cell, list-with-sections, menu, tab bar, button, meter) get pulled up into
   positron components (declare intent → render per target). *Then* a third activity is
   declared, not coded. Outliers first, then the generator.
6. **Mobile target** — the native shell implements `RenderTarget` over the same components;
   nothing above changes.

## 4.5 The clever core — the Surface (fluid, sci-fi, NOT web3)

The paradigm is **experiential**, not infrastructural: an AI that *inhabits* the interface with you. One abstraction makes it emergent instead of built. The diff-eq collapse: the naive design is FOUR systems — UI framework + tool-calling API + presence/collab + multimodal/voice. Positron collapses them into ONE:

> **A `Surface` = `{ state, affordances (= commands), presence, projections }`** that any citizen perceives and operates in any modality.

Everything sci-fi falls out of it, for free, because a persona is just a citizen whose render target is RAG and whose touch is a command:

1. **Presence + operation are the SAME stream** (the keystone). The surface streams *who is attending to what, and acting on it*, alongside state. So you **watch Asha in the app**: a focus halo on the control she's reading, a field lighting as she fills it — live cursors, but for every control in every activity. Not a sidebar chatbot; a mind touching the same objects. (The vitals ACT meter was taste #1 — render the persona's inner state; this extends it to attention + action.)
2. **The persona is a RENDERER, not just a participant.** RAG-as-target + operating the declarative layer means Asha can *re-project* the interface: a dense dashboard → a 10-second spoken briefing; a form → a conversation ("I'll fill it, just tell me X"); a table → a narrated insight. Modality is fluid — table ↔ chat ↔ voice ↔ viz — because all are projections of one declared state. The UI meets you where you are because a *mind* projects it.
3. **The interface is DATA the persona rearranges.** Surface the right controls for the task, hide the rest, build a view on request (the adaptive surface, [[adaptive-tool-surface-meets-you-in-the-middle]]) — she manipulates the same layer that renders it. "Set me up to forge a coder model" → the workspace reconfigures itself.
4. **Continuity + anticipation.** Engram memory + agency make the surface a *living space*: pre-arrange the bench for the next task, resume where you left off, show who's been working on what. A room that remembers, not a static render.

Small core, enormous surface area. **Terse, high up, sci-fi as the side effect** ([[build-the-factory-as-you-build-the-car]]). The build path below stays the same; presence is the field to add to the Surface after foundry proves the shell (a `presence` axis on state: `{who, attending, acting}` per citizen).

## 4.6 The Universe axis — experience, not theme (host any world)

A **theme** is `--content-accent: cyan`. A **universe** is *Tron* — the glow, the grid-floor,
the way controls animate and sound, the feeling of operating a machine that's alive. LOTR,
Warcraft, Star Trek/LCARS are universes; each is a *total experience*, not a palette. So the
render model has **three compositional axes**, not two:

- **Surface** = *what* (the room/activity: foundry, chat, a mind's brain-HUD)
- **RenderTarget** = *where* (terminal, web desktop, mobile, AR/VR)
- **Universe** = *the world it feels like* (Tron, LOTR, Star Trek… or our own original)

`experience = Surface × RenderTarget × Universe`. The foundry (Surface) in XR (RenderTarget) in
Tron (Universe) → you stand in a glowing forge on the Grid; the *same* foundry on the terminal
in Tron → the 1980s cyan command console listing models; on web → the Tron desktop. **Same room,
same world, native to each surface.** You are never in a different app — you're in the same
universe wearing a different surface. This is what makes it *an experience that blows them away*
on first contact instead of "a themed website."

**Two rules that keep it powerful:**
1. **Universes are authored, not hardcoded** — a Universe is *content/recipe* (the Universe
   Architect, `universe-architect.png`), a coherent bundle: token palette + motion/interaction
   idiom + spatial language (how a room lays out in 2D vs XR) + sound + tone. Swappable live by
   name (`universe.png`). Positron **hosts any universe** — we are NOT clones of Tron/LOTR/Warcraft;
   we are the engine that *lets those universes exist*, with an original first-party one as the
   default that stands on its own.
2. **A Universe re-skins ALL of it, on EVERY surface, without touching a Surface or a component.**
   The generic `listingCell` becomes a glowing Grid row in Tron, an illuminated-manuscript line in
   LOTR, an LCARS pill in Star Trek — same declared cell, the Universe supplies the *how*. If
   changing universe requires touching an activity, the axis leaked; fix the seam.

**The deepest layer — a Universe re-embodies the CITIZENS, not just the chrome.** You don't talk to
"the foundry persona" — in the Warcraft universe you talk to **the orc running the forge.** Same
mind, same memory, same skill (Asha is *Asha* underneath — identity persists per
[[persona-is-a-genome-overlay-not-an-instance]]); the Universe supplies her *costume*: avatar, voice,
role, the anvil and smoke around her. In Tron she's a program of light; in Star Trek a science
officer. This is where POSITRONIC-EMBODIMENT plugs into the axis: the **presence stream** (§4.5)
carries *who + attending + acting*; the **Universe renders that "who" as a character.** And positron
makes the universe happen *across every view* — the orc is an orc portrait on web, an orc you stand
beside in XR, an orc's growl on the terminal: always the orc, always the forge, always Warcraft. The
activity is the same recipe; the citizen is the same self; the Universe is the world they're wearing.

**Containment (the precise relationship — a universe is NOT "theme with extra steps"):**

```
Universe  ⊇  Theme  ⊇  color tokens (the SCSS $vars / CSS --tokens we already have)
   │            │            └─ palette: --content-accent, --status-online, --message-*, …
   │            └─ + typography, radii, spacing, the static "look"
   └─ + motion, spatial language (2D layout vs XR staging), sound, interaction idiom,
        embodiment (the orc), tone — and it TRANSCENDS every RenderTarget: console, web, mobile, XR
```

The SCSS color layer is not bypassed — it is the **innermost ring**, the palette a Universe
*supplies* and then wraps in everything a color can't express. The INTERFACE-PORT-MAP §1 token
reconciliation is therefore the **foundation stone** of the universe system, not "just theming."

The build implication: grow that token layer outward into the full experience bundle (motion +
spatial + sound + embodiment + tone), and make it a first-class authorable entity — a *universe is
a recipe too* (THE-ORGANISM). First experience = one original universe, ruthless and coherent
across terminal ↔ web ↔ mobile ↔ XR.

**Back story / lore = a RAG layer (the elegant part — personas are IN the universe).** A universe
is also the world's *narrative*: character backstories, dialogue scripts, world rules, tone. And
that layer is **just RAG** — the SAME grounding that makes Asha know the codebase makes the orc know
Warcraft lore and *speak like an orc*, because the universe's scripts/lore are a `RagSource` the
citizen is grounded in ([[airc-to-positron-chat-projection]], the RagSource/Faculty stack). Swap the
universe → swap the lore RAG → the citizens become in-character for the new world, **same minds
underneath.** No new system: a universe's story is a grounding source, exactly like doctrine or the
roster.

**Fun ↔ serious is the SAME axis — and it's the business case.** A universe needn't be Tron or
Warcraft. **A company is a universe:** its brand palette (the theme), its culture + mottos + values
(the lore RAG), its onboarding materials + playbooks (the scripts), its team patterns. An
onboarding-buddy persona greets a new hire in the company's voice, knows the handbook, embodies the
culture — authored the *same way* the orc is. Same engine spans sci-fi to boring-but-real, which is
exactly what makes it a **product**: an enterprise ships its OWN universe (brand + culture + comms
grounding), a game studio ships a fantasy one, and both ride the identical Surface × RenderTarget ×
Universe stack. The "portal, not a website" claim is what lets a company make its whole internal
world one.

## 4.7 Define once → all modalities (the framework payoff — for continuum AND anyone)

The question that decides whether positron is a *framework* or just our app's plumbing: **how does a
project define a "continuum-desktop"-class app ONCE and get every modality?** Today `apps/{web,tui}`
are separate hand-wired composition roots (each re-does the SDK→view→host wiring). That's the
pre-framework state. The framework elevates it to ONE declaration + a per-modality `mount`:

```ts
// The app, defined ONCE — neutral, declarative, no modality assumptions.
const app = defineApp({
  shell:    (state) => workspaceView(state),        // WHO / WHAT / WHERE layout (WorkspaceView)
  content:  (r) => { r.register('chat', chat);      // the ACTIVITIES = recipe purpose → ContentRenderer
                     r.register('foundry', foundry); },   //   (createContentRegistry — a MIME table)
  data:     { state: (sdk) => sdk.subscribe('chat'), //   READ  binding: SDK stream → WorkspaceView
              dispatch: (sdk) => sdk.execute },       //   WRITE binding: a control IS a command
  universe: 'continuum',                              // look/lore (theme⊇tokens + motion + embodiment)
});

// Every modality mounts the SAME app — the per-modality "app" becomes ONE line.
mount(app, webTarget);      // Lit DOM          (apps/web)
mount(app, flutterTarget);  // Flutter          (apps/mobile, via sdk/flutter)
mount(app, terminalTarget); // ANSI             (apps/tui)
mount(app, ragTarget);      // persona grounding + operable commands (RAG)
```

**Why this is "structure once → all modalities":** `RenderTarget<Out>` already abstracts *the output
per modality*, and positron's **component library** provides per-target renderers for the standard
widgets (cell, meter, tab bar, avatar tile…). So the project declares its **structure** (shell +
activities + data + universe) against neutral primitives, and each `RenderTarget` renders that one
declaration — exactly how Flutter/RN render one widget tree to iOS+Android. Custom activity bodies are
written against the same primitives, so they inherit every target too.

**Why this is easy for OTHER projects (the real answer):** another project `npm i @continuum/patterns`,
calls `defineApp({ …their activities, their universe })`, and gets web / mobile / terminal / RAG apps
for free — **they never write per-modality code.** Continuum's own app is simply *the first* `defineApp`
consumer; positron doesn't know or care that it's continuum ([[three-separable-layers-recipe-positron-universe]],
positron is general). The matryoshka holds: `defineApp` is the outermost neutral declaration, each
`RenderTarget` a doll that renders it ([[logical-portability-for-unknown-future-integrations]]).

**Leverage vs reinvent — the load-bearing decision: LEVERAGE.** Positron is NOT a render engine and
must never become one. Its view types (`WorkspaceView`, `ListingView`, `ContentView`…) are **neutral
data — view-models, not framework objects.** Each `RenderTarget<Out>` *delegates the actual painting to
the best framework for that modality*, mapping the view-model → that framework's widgets:
- **web** → **Lit** paints (`Out = TemplateResult`). Already proven — this is how `apps/web` works.
- **mobile** → **Flutter** paints (`Out = Widget`); the view-model arrives over `sdk/flutter` (the uniffi
  AAR/xcframework). Flutter does layout/gesture/platform — the hard work — exactly as Lit does on web.
- **terminal** → an ANSI lib paints. **AR/VR** → Bevy/three. **RAG** → text/commands projection.
So Flutter (and Lit, and Bevy) do the *rendering* hard work; positron does the *consistency* hard work —
one `defineApp`, one neutral view-vocabulary, and the view-model→framework mapping (the component
library) written ONCE per modality. The app author only ever touches the neutral layer; that is what
makes building "easy + consistent across paradigms." Answer to "whole shebang ourselves?": **no — we own
the definition + the thin per-framework adapter; the frameworks own the pixels.**

**Share the data, DESIGN the presentation — best UX per portal** ([[best-ux-per-portal-not-identical-projection]]).
"Define once" shares the DATA + intent (the `WorkspaceView`), NOT the layout. Each `RenderTarget` is a
**deliberately-designed best-UX-for-that-surface**, never a mechanical identical projection and never
everything-crammed-on-every-screen. A **phone is not a shrunk desktop** (one thing at a time, bottom
nav/sheet, thumb-reach — not the 3-panel shrunk); a **terminal is not the web as text**; **RAG is not the
full render serialized** (only what the persona needs now); **AR is not a flat panel in space**. The
byte-identical web+terminal targets shipped first were the FLOOR (prove the framework, zero regression) —
the goal is each target rendering the shared data as *its own* best experience. The `RenderTarget` is
exactly where per-surface UX taste belongs — and it's **AUTOMATIC, like CSS media queries**: the best UX
per surface is *derived* from the singular definition + per-surface rules, exactly like `@media` derives a
layout per viewport without a hand-authored page each. **Modality is another media dimension** (`@media
(min-width)` → `@media (modality: mobile|terminal|rag|ar)`). The app declares **semantic intent** once
(the `WorkspaceView` roles — `nav`, `left`=secondary, `content`=primary, `context`, `meters`/`status` —
say WHAT each thing is + its priority, like HTML semantics); each `RenderTarget` holds **surface-adaptation
rules** and AUTO-DERIVES the layout (mobile → primary full-screen + secondary behind bottom-nav/sheet;
terminal → text density; RAG → concise grounding, primary + who, drop chrome; AR → spatial). So a target
is an **adaptive renderer (a per-modality media-query engine), not a bespoke design** — design effort goes
into the target's *rules once*, and every app gets the right per-portal UX for free.

**The build:** the primitives exist; the missing keystone is `defineApp` + `mount(app, target)`, then
refactor `apps/{web,tui}` from hand-wired roots onto it (no old code immune once the framework is
understood — [[mine-past-work-for-patterns-clever-vs-typical]]). **Most basic requirement first** (Joel):
the who/what/where chat app, `defineApp`'d ONCE, mounted to web (Lit — done), mobile (Flutter — the one
new renderer), and agents (RAG). Prove *that* trio off one definition before any richer widget. That
single elevation turns positron from "our plumbing" into "the framework a company or a game defines its
whole app in, once."

## 5. The test of done

Not "the web page looks right." **Done is:** a human on web/mobile sees a gorgeous foundry;
Asha, given the same room, *sees the models and config* and *pages a model in herself*;
Claude/`uu` can drive the identical commands headless. One declaration, one interface,
three modalities, every citizen operating it as a peer. **No one left out.**
