# Dynamic Personas & Model Search — de-hardcoding the cooperative multi-persona system

> We are a cooperative multi-persona system, and it is still hardcoded: a fixed
> roster, a fixed model-per-role table, a static model catalog, and personas that
> only exist by editing Rust. This is the spec to make **who the personas are** and
> **what models they run** *data and dynamic discovery*, not code.

Companions: [[ask-anything-assemble-best-self-or-train]], [[shop-genome-market]],
[[room-purpose-is-per-recipe-not-an-enum]], [[unsloth-universal-model-gateway]],
[INFERENCE-LANES-REALISTIC](../architecture/INFERENCE-LANES-REALISTIC.md),
[GENOME-FOUNDRY-SENTINEL](../architecture/GENOME-FOUNDRY-SENTINEL.md).

---

## 1. The hardcoding today (what we're replacing — with file refs)

| What | Where | Shape |
|---|---|---|
| The roster (which personas spawn) | `persona/spawner_module.rs::plan_for_tier()` | hardcoded `Vec<DesiredRole>` (one Helper today; `[Helper, Coder]` commented for "slice 14") |
| Model per role × tier | `persona/role_template.rs` `helper_template()` / `coder_template()` | hardcoded `model_per_tier` maps of model-id strings |
| Roles | `persona/role_template.rs` `RoleId` | enum `Helper/Coder/Sentinel/Custom` |
| Model catalog | `model_registry/catalog.rs::models()` | static Rust `Vec<Model>` — NOT read from any `/v1/models` |
| Model resolution | `cognition/model_resolver/` | static filter chain over the static catalog |
| Persona identity | minted in `modules/persona_instance_manager.rs` (`Uuid::new_v4()` + derived name) + `persona/seed.rs` JSON | no persona-as-data; no `persona/create`; name derived, not authored |
| Per-persona model | `persona/supervisor.rs::UnslothPersonaAdapterFactory` | **discards `profile.model_id`, sends `model: None`** → every persona shares unsloth's ONE loaded model |

**The sharpest consequence:** there is currently **no per-persona model** — every persona
runs whatever single model unsloth has loaded. A cooperative team of specialists that
all think with the identical model is not yet the system we describe.

## 2. The thesis (what it should be)

Per [[ask-anything-assemble-best-self-or-train]]: given a *need*, the system
**assembles the best self** or **forages/trains** to become it. Per
[[room-purpose-is-per-recipe-not-an-enum]]: behavior is **data**, not an enum. So:

- **Models are discovered + selected to FIT the hardware** — this is the hard,
  high-value problem and the priority. Machines vary wildly (a 53 GB M-series, a
  Windows box with a big discrete GPU, a small laptop, CPU-only). The system must
  detect the host's real capability and pick the **largest/best model that actually
  fits** from the discovered set (unsloth's `/v1/models` ∪ the trust-scoped genome
  market) — never a hardcoded model-per-tier table.
- **Personas are data — a curated desired list now; generated-on-demand is the ideal
  endgame.** We already know roughly which personas we want to build (Helper, Coder,
  specialists…). So near-term a persona is a `PersonaSpec` (data: name, specialty,
  persona-card, a *model-fit policy*, trust scope) drawn from a curated list;
  generation-from-need (slice 6) is the aspirational top, not the starting point. The
  weight of the work is **model fit**, not persona invention.

### Scope: single-machine first; the grid is a deferred capability axis

Most users start on **one machine**, so optimize model-fit for the single host first
and do our best there. The grid adds a **shared capability** (route a too-big need to a
peer with the VRAM, [[compute-lease-boundary]] / [[inference-lanes-realistic]]) — a real
axis, but a *later* one. Design model-fit so "the capability I can reach" can grow from
"this host" to "this host + grid peers" without reshaping the selection logic, then ship
the single-host case first.

## 3. Invariants (the lessons — hold every line)

1. **No fallbacks.** If a persona's model can't be selected/assembled, **fail loud and
   name it** — never silently downgrade to a default model
   ([[fallbacks-are-illegal-fail-loud]]). A persona with no resolvable model does not
   spawn-on-a-stand-in; it surfaces the gap.
2. **One place.** A persona is defined in ONE data source (entity/seed), never split
   across Rust + data. The runnable model set has ONE discovered source (unsloth +
   market); the static catalog degrades to *priors/metadata*, not truth.
3. **Intentional + visible.** Boot/roster **announces** which personas are live and
   which model each runs (the same `boot_status` discipline that now announces the
   inference path). You always know — no silent "everyone shares one model."
4. **unsloth is the runtime.** Selection resolves to a concrete model id the gateway
   serves (loading via the keystone / genome page-in if needed); the persona adapter
   sends that **explicit** id — which is also what lets N personas run N models on the
   one gateway ([[inference-lanes-realistic]]).
5. **Trust-scoped discovery.** Model / genome / persona discovery is bounded by the
   deployment trust boundary (home / hospital / public) via `GridTrustAuthPolicy`
   ([[lora-layers-p2p]]).

## 4. Architecture

**A. Model search/selection** (replaces the static catalog + `model_per_tier`):
- *Discover:* query the runnable set — unsloth `/v1/models` (live gateway catalog) ∪
  the trust-scoped genome market. The Rust catalog stays only as priors (benchmarks,
  capability tags), never as the authority on what can run.
- *Select:* given a `PersonaSpec`'s need (specialty/task/fitness target), pick the best
  by fitness — capability match, context, benchmark priors, A/B history
  ([[search-then-ab-dont-start-from-zero]]).
- *Assemble:* the result may be **base + LoRA composed** for the need (the foundry
  composer), not just a bare pick.
- The persona adapter sends the **selected** model id (kill the `model: None` shortcut),
  so per-persona models are real and multi-lane.

**B. Persona generation as data** (replaces `plan_for_tier` + role templates + minted-only):
- `PersonaSpec` entity (data): identity, specialty, persona-card, model-selection
  policy, trust scope. Generalizes the existing `RoleTemplate` ORM from roles to
  personas.
- `persona/create` command: materialize a persona from a `PersonaSpec` at runtime.
- The roster = a **data query** (specs scoped to deployment + tier), not `plan_for_tier`'s
  hardcoded `Vec`. The existing spawner already materializes per-row — feed it the query.
- *Generate from need:* a manager/sentinel persona can author a `PersonaSpec` from a
  stated gap ("we need a gastro specialist") → model-search assembles its model → spawn
  ([[academy-training-design-corpus]], the cooperative-management angle).

## 5. Slice plan (VDD-gated; each no-fallback + boot-visible)

Reordered to lead with **model-fit** (the priority), single-host first.

1. **Capability detection + model discovery (single host).** Detect the host's real
   capability (VRAM/tier — the boot log already prints `GPU=… VRAM=…`) and discover the
   runnable set from unsloth's `/v1/models`. Boot announces both. The static
   `catalog.rs` becomes priors. *No selection logic yet — just "what can I run, on what."*
2. **Fit selection (replace `model_per_tier`).** Given capability + the discovered set,
   select the **largest/best model that fits** for a need; delete the hardcoded
   `model_per_tier` table and `plan_for_tier`'s model strings. Fail loud if nothing fits
   (name the gap — don't silently pick a tiny default). *This is the core de-hardcoding.*
3. **Per-persona model is real.** Adapter sends the **selected** model id (kill
   `model: None`); personas can run different fitted models/LoRAs on the one gateway;
   boot announces each persona's model. *Kills "everyone shares one model."*
4. **PersonaSpec as data + `persona/create`.** Personas become data drawn from a curated
   list (delete `plan_for_tier`'s hardcoded roster `Vec`); roster = a query; each spec
   carries a model-fit policy consumed by slice 2.
5. **Fitness + foundry assembly + genome-market discovery (trust-scoped).** Compose
   base + LoRA for the need; extend discovery to the P2P market within
   `GridTrustAuthPolicy` scope ([[shop-genome-market]], [[search-then-ab-dont-start-from-zero]]).
6. **Grid-shared capability + persona generation from need (aspirational).** "Capability
   I can reach" grows to include grid peers (route too-big needs, [[compute-lease-boundary]]);
   a manager/sentinel persona authors a `PersonaSpec` from a gap and the system assembles + spawns it.

## 6. Code map

| Concern | Today | Becomes |
|---|---|---|
| Roster | `spawner_module.rs::plan_for_tier()` (Rust Vec) | data query over `PersonaSpec`s |
| Persona def | minted UUID + `seed.rs` JSON | `PersonaSpec` entity (ORM) + `persona/create` |
| Model per persona | `role_template.rs` `model_per_tier` + adapter `model: None` | spec's model-selection policy → discovered selection → explicit model id |
| Catalog | `model_registry/catalog.rs` static Vec | unsloth `/v1/models` ∪ genome market; static Vec = priors |
| Selection | `model_resolver` static filter | fitness selection over discovered set + foundry assembly |

The bones exist (PersonaInferenceProfile, the per-row spawner, RoleTemplate ORM, the
unsloth gateway, the foundry, GridTrustAuthPolicy). The build is **making the roster and
the model a discovered/generated data flow** — slice by slice, each held to §3.
