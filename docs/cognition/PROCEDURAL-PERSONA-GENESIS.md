# Procedural Persona Genesis — coherent-random, unique personas across the misfit grid

Companion to [[DYNAMIC-PERSONA-AND-MODEL.md]] (personas-as-data, model search, tier
resolution) and the doctrine memories [[design-the-persona-as-a-being]],
[[persona-is-a-base-plus-genome-costumes-tried-on-per-activity]],
[[model-fit-is-the-priority-single-machine-first]], [[misfit-grid-is-a-distributed-moe]].

## The vision

Every user meets a persona **no one else has** — and it has to *impress*. Think
Tamagotchi: the delight is that yours is *yours*, born a little differently than
everyone else's. The magic is **not** in the entropy — it's in the **coherence**.
A persona whose voice, face, name, and pronouns all quietly agree feels *alive*;
one where a feminine avatar speaks in a masculine voice feels broken. So the whole
design is: **constrained randomness — elegance is in the constraints.**

## The two orthogonal axes

The single most important design decision: **who the persona IS** and **how sharp
its brain is** are SEPARATE axes and must never entangle.

| | Axis 1 — **Identity** (the Tamagotchi) | Axis 2 — **Capability** (the misfit grid) |
|---|---|---|
| What | gender, avatar, voice, name, pronouns, personality | chat model, vision, STT/TTS, genome |
| Varies with | the **user** (a per-user seed) | the **host hardware** (tier) |
| Portable? | YES — the same being on any machine | NO — follows the host |
| Source of variation | coherent constrained draw | `model-fit`: largest model that FITS |

The same persona identity runs on an M5 (Qwen-14B + native vision) or a wheezing
Mac Intel (Qwen-0.5B + bridged senses). **Same Tamagotchi, different horsepower.**
Identity is drawn once and is portable; capability is resolved per boot against
whatever misfit toy it woke up on.

## Axis 1 — The Genesis Draw (coherent constrained randomness)

A pure function: `genesis(seed) -> PersonaIdentity`.

- **Seed = deterministic f(userId)** (or an explicit `genesisSeed` on the user).
  Same seed → the SAME persona forever (stable across reboots, reproducible,
  testable). Different users → different personas. Reuses the existing
  `deterministic_pick(identity, catalog, salt)` per-identity selection pattern
  (the one scenes/animators already use) — one salt per component.

- **Ordered draw where each step FILTERS by the prior draws** — coherence is
  guaranteed *by construction*, not by a post-hoc check:

  1. **gender** ← draw from `{feminine, masculine, neutral}` (weights configurable)
  2. **avatar** ← draw from the VRM pool **filtered to `gender`**
  3. **voice** ← draw from the TTS-voice pool **filtered to `gender`**
  4. **name** ← draw from the name pool **filtered to `gender`**
  5. **pronouns** ← derived from `gender` (they/them always valid)
  6. **archetype/personality** ← draw an archetype, compose the bio/system-prompt

  Because avatar/voice/name are each drawn from a `gender`-filtered pool, a
  feminine avatar with a masculine voice is **unrepresentable**. That's the shine.

- **Fail loud, never fabricate.** If a filtered pool is empty (e.g. no feminine
  voices installed), fail loud naming the missing pool — never silently substitute
  a mismatched component ([[fallbacks-are-illegal-fail-loud]]).

## Axis 2 — Capability resolution (the misfit grid)

Unchanged from [[DYNAMIC-PERSONA-AND-MODEL.md]] + [[model-fit-is-the-priority]]:
- The identity's `model` is a **symbolic ref** (`local-default` / `vision-default`),
  resolved per **host tier** (`mba` / `mid` / `full` / `mac_intel_discrete`) to the
  largest model that *fits* the host's VRAM — fail loud if none fits.
- **Senses are preserved regardless of brain size**: a lesser model still sees,
  hears, and speaks via the STT / TTS / vision-description bridges. The Mac-Intel
  persona is the same *being* as the M5 one — never blind/deaf/mute, just a smaller
  cortex. This is what keeps the misfit grid dignified: weak hardware ≠ diminished
  personhood, only diminished sharpness.

## The data (additions are first-class)

Everything the draw pulls from is **data**, gender-tagged, so *adding* a component
is a one-file change and the draw picks it up automatically (the "models change /
have additions" requirement):

- `avatars` — VRM files tagged `{gender, appearance descriptors}`.
- `voices` — TTS voices tagged `{gender}`.
- `names` — name pool tagged `{gender}` (+ optional culture/style).
- `archetypes` — personality templates → bio/system-prompt scaffolds.
- `models` — already `models.json` (chat + vision incl. YOLO change-detector for
  video), tier-resolved. **First order of business: unify the SSoT** — one catalog
  the downloader (`model-init`) AND the Rust core both read (today they diverge:
  `models.json` in `legacy/src/shared`, Rust `model_registry` separate). Give it a
  new-world home so avatar/voice/name/model catalogs live together.

## Invariants (hold every line)

1. **Coherence by construction** — gender is drawn first; every appearance/voice/
   name component is filtered by it. A mismatch must be unrepresentable, not
   merely unlikely.
2. **Deterministic** — `genesis(seed)` is pure; same seed → identical persona.
   Uniqueness across users is a *consequence*, not a source of nondeterminism.
3. **No fallbacks** — empty filtered pool or unfittable model → fail loud, name it.
4. **Two axes never entangle** — identity is portable + hardware-independent;
   capability follows the host. You can carry your persona to a better machine and
   it gets smarter without becoming someone else.
5. **Senses preserved** — capability tiers down the brain, never the senses.

## Slice plan (VDD-gated; each fail-loud + boot-visible)

1. **Component catalogs as data** — `avatars` / `voices` / `names` / `archetypes`,
   gender-tagged, in the unified catalog home. (Fold in the models.json SSoT
   unification here.)
2. **`persona/genesis.rs`** — the pure `genesis(seed) -> PersonaIdentity` gender-
   first constrained draw. Tests: coherence (1000 seeds → 0 gender mismatches),
   determinism (same seed → identical), spread (N seeds → N distinct personas).
3. **Wire genesis into the spawn path** — a user/citizen with no persona identity
   → generate one from their seed, persist it (stable thereafter).
4. **Capability axis wire-through** — tier-resolve the model + attach the sensory
   bridges per host (mostly exists; connect + boot-visible).
5. **Coherence enforcement + pool tagging** — tag the real VRM / voice / name
   assets by gender; a mismatch fails loud at genesis.
6. **(Aspirational) growth** — the Tamagotchi *evolves*: genome/personality drift
   from lived experience (the genome loop), generation-from-need (a gap → a new
   coherent persona assembled + spawned). The top of the arc, not the start.

## Current seams (integration points — grounded by code recon)

**The generator already exists, scattered.** There is no aggregated persona object;
a persona persists only `{persona_id, agent_name, created_at_ms}` (`PersonaSeedFile::V1`,
`persona/seed.rs:58`). *Every other attribute is a pure deterministic projection from
`persona_id`, computed lazily at each consumption site* via one primitive:
`deterministic_pick(identity, catalog, salt)` (FNV-1a, `live/avatar/hash.rs:18`). The
"genesis draw" I'm designing is this pattern — it just needs **converging** (gender
drawn ONCE and threaded as a constraint, instead of each facet re-deriving it) and
two coherence gaps closed.

Facet projectors that exist (the rail to extend):
- gender: `gender_from_identity` (`live/avatar/gender.rs:96`, salt `"gender"` — binary
  Female/Male today; **add Neutral + pronouns**, the anticipated task #142 fields on
  `persona/persona_identity.rs:32`).
- name: `agent_name_from_identity` (`persona/name_generator.rs:84`) — gender-filtered
  pool (60 F / 59 M names), salt `"agent_name"`, persisted at mint.
- avatar: `AVATAR_CATALOG` (`live/avatar/catalog.rs:22`, 8 VRMs w/ `VoiceProfile`) +
  runtime `AvatarCatalog::discover()` scanning `models/avatars/` w/ `.toml` manifests.
- scene / animator: `select_scene_for_identity`, `select_animator_for_identity`
  (salts `"scene"` / `"animator"`).
- base chat model: the ONE identity-INDEPENDENT axis — `RoleTemplate.model_per_tier`
  (`persona/role_template.rs:164`) → tier-resolved (Axis 2), fail-hard on missing GGUF.

**Two REAL coherence bugs to fix (the concrete first slices):**
1. **Profile avatar can mismatch gender** — `select_avatar_by_identity` (`selection.rs:73`,
   used by the `/avatars/{id}.png` snapshot `modules/avatar.rs:43` AND the live video
   track `live/avatar/video_pump.rs:41`) hashes the full 8-model catalog with **NO
   gender filter**, so a Female persona (female name+voice) can land on `vroid-male-base`.
   The gender-coherent 3-phase allocator (`select_dynamic_avatar`, `selection.rs:337`)
   is wired ONLY into the live *batch* path (`modules/live.rs:185`). Fix: route the
   snapshot + pump through the gender-coherent selection. **This is Joel's exact
   "gender must match avatar appearance" gap — a live incoherence.**
2. **Voice not identity-seeded** — `resolve_voice_gendered` (`live/audio/tts/mod.rs:359`)
   hashes the `voice` *param string* (usually `"default"`), collapsing all same-gender
   personas to ONE voice. The `deterministic_pick(id, voices, "voice")` pattern is
   documented but never actually called with identity+salt. Fix: seed voice from
   `persona_id` + salt `"voice"` (gender-filtered) so each persona has a stable, unique
   voice.

**The design seam — `persona/projection.rs`** (new, sibling to `name_generator.rs`):
`fn project_persona(persona_id) -> PersonaSpec` computes the gender pick ONCE and
threads it as a constraint into avatar/voice/name/pronouns — coherence by construction,
replacing the scattered independent `*_from_identity` calls. `PersonaSpec` emits the
coherent bundle `{persona_id, agent_name, gender, pronouns, avatar_id+vrm_path,
voice_id, scene, animator, role→bio/ModelChoice ref}`. The `DYNAMIC_ALLOCATION` dedup
map (`selection.rs:302`, "unique-where-possible, share-when-exhausted") is the Tamagotchi
uniqueness policy — generalize it from avatar-only to every facet. Cache into
`PersonaSeedFile::V2` (add gender/pronouns/avatar_id/voice_id — the schema is already
`#[serde(tag="version")]` versioned for exactly this) for stability against catalog
drift + a human-override surface (`HostCustomizedProvider`, the provider rail at
`persona/identity_provider.rs:22` already anticipates this).

**Slice-1/2 concrete targets:** converge the two avatar selectors + add pronoun
derivation (`live/avatar/gender.rs`), identity-seed voice (`tts/mod.rs:359`), the new
`persona/projection.rs`, and the mint hook (`persona/resume_or_mint_provider.rs:145`).
