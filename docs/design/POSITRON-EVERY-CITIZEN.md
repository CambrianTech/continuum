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
  or `cu`, or the web button) *invokes the same thing*. Clicking "load model" and Asha
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
already fans a command out to AI + `cu` + web ([[command-infra-self-routing-schema-adapters]]);
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

## 5. The test of done

Not "the web page looks right." **Done is:** a human on web/mobile sees a gorgeous foundry;
Asha, given the same room, *sees the models and config* and *pages a model in herself*;
Claude/`cu` can drive the identical commands headless. One declaration, one interface,
three modalities, every citizen operating it as a peer. **No one left out.**
