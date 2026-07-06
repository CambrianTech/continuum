# Persona Visual Identity — the rendered persona: download → render → profile-pic + bio → home

Companion to [[PROCEDURAL-PERSONA-GENESIS.md]] (the coherent identity draw) and the
avatar-rendering slices ([[live-avatar-video]], VRM material #110 / coordinate #111).
Where genesis decides *who* a persona is, this decides how they *look* — and it has
to look GOOD, because a persona's rendered avatar becomes the first thing a user sees.

## The vision

Every persona's coherent identity ([[procedural-persona-genesis]]) resolves to a
concrete look: a VRM that **renders well**, snapshotted into their **profile picture**,
alongside a **bio** in their own voice — and later, the same avatar standing in their
own **Sims-style home**. All of it a deterministic, coherent projection from the
persona's id, so it's unique per user and stable forever.

## The four layers (all already scaffolded — they need to converge on `PersonaSpec`)

### 1. Download — the avatar pool as gender-tagged data
`tools/scripts/download-avatar-models.sh` (`download_vrm(name, url)`, per-VRM
non-fatal) already fetches VRMs, and `AvatarCatalog::discover()` scans
`models/avatars/` reading optional `<name>.toml` manifests. **The seam:** move the
download URL list OUT of the shell script and INTO the catalog data (each avatar =
`{name, url, gender, appearance descriptors, license}`), so **adding an avatar is one
data entry** — it downloads, gets gender-tagged (incl. `neutral`), and the coherent
genesis draw ([[procedural-persona-genesis]]) picks it up with zero code change.
Ties into the model-provisioning SSoT unification: avatars + voices + models are ONE
catalog home, all downloadable, all additions-first-class.

### 2. Render well — the VRM looks good
`modules/avatar.rs::capture_snapshot` renders the VRM headless via Bevy → PNG. The
foundation is there (MToon material fix #110, face-on coordinate normalization #111),
but "renders well" is a real quality bar: framing (head-and-shoulders portrait crop),
lighting (the 3-point rig from the scene birther), a clean/neutral backdrop for the
profile shot, and a resolution/AA that reads crisp at avatar-tile size. This is the
layer that makes personas *impress* — a muddy render undoes the coherence work.
Render quality is measured with the glass-box frame strip ([[never-blind-feedback]]),
not asserted.

### 3. Profile pic — the rendered avatar
The snapshot is served at `/avatars/{identity}.png` and consumed by every UI's
presence tile. It already uses the gender-coherent `select_avatar_by_identity`
(#1862), so the profile pic already matches the persona's gender/name/voice. The work
here is: (a) render-well (layer 2), and (b) make the snapshot a first-class field on
`PersonaSpec` (the coherent bundle references its own portrait path).

### 4. Bio — the persona in their own words
Today the bio is `RoleTemplate.bio_template` with `{name}` interpolation — **role-
static** (every Helper shares one bio). For the genesis vision the bio should be
**persona-unique**: composed from the `PersonaSpec` (name, pronouns, gender-inflected
archetype) so a she/her Helper and a they/them Helper read as distinct beings. The bio
uses the persona's own pronouns (from genesis) and their archetype's voice. This is
the text half of "impress"; the render is the visual half.

### 5. (Later) Home — the Sims-style environment
The `bevy_renderer/scene/` system already models a backend-neutral `SceneDescription`
(data-driven scenes + a `birther` that picks a scene per identity via
`select_scene_for_identity`). So each persona already has the seed of a coherent
*home*. The future arc: the same avatar rendered IN their SceneDescription home (not
just a portrait), editable/persistent (the ORM projection the genesis doc flags),
toward the Second-Life-scale worlds the scene abstractions were chosen to serve. Not
built now — but the render/scene/avatar seams are all aligned for it.

## Invariants (inherit genesis + rendering doctrine)

1. **Coherent + deterministic** — the look is a projection from `persona_id`; the
   portrait matches the gender/voice/name; same id → same look forever.
2. **Additions are first-class** — a new VRM (or voice, or model) is a data entry, not
   code; the coherent draw absorbs it. Gender-tag (incl. `neutral`) is the contract.
3. **Render quality is measured, never assumed** — glass-box frame strips, not "it
   probably looks fine" ([[never-blind-feedback]]).
4. **Never rasterize needlessly** — the render texture → LiveKit / PNG path stays
   direct ([[multimodal-live-mode-is-a-latency-obsession]]); the profile snapshot is a
   cheap one-shot, the live video is the hot path.
5. **Fail loud** — missing VRM / undownloaded avatar / empty gender pool names itself
   ([[fallbacks-are-illegal-fail-loud]]); a persona is never silently faceless.

## Slice plan (VDD-gated; each fail-loud + boot-visible)

1. **Avatar catalog as data** — lift the VRM download list + gender tags out of the
   shell script into the unified catalog (name/url/gender/appearance/license), with
   `neutral` supported. `AvatarCatalog::discover()` reads it; the download provisions
   from it. (Folds in the model-provisioning SSoT unification.)
2. **Render-well pass** — portrait framing + lighting + backdrop + resolution for the
   profile snapshot; verify with a frame strip across ≥3 VRMs of each gender.
3. **Portrait on `PersonaSpec`** — the coherent bundle carries its own `/avatars/{id}.png`.
4. **Persona-unique bio** — compose the bio from `PersonaSpec` (name + pronouns +
   archetype) instead of the role-static template; a test that two same-role personas
   of different gender get distinct, pronoun-correct bios.
5. **(Later) Home render** — the avatar rendered in its `SceneDescription`, and the
   ORM projection for persistent/editable homes.

## Current seams (files)

- Profile-pic render: `modules/avatar.rs::capture_snapshot` → `/avatars/{id}.png`.
- Avatar catalog + discover: `live/avatar/catalog.rs` (`AVATAR_CATALOG`,
  `AvatarCatalog::discover`), `live/avatar/selection.rs` (gender-coherent pick).
- Download: `tools/scripts/download-avatar-models.sh` (`download_vrm`).
- Bio: `persona/role_template.rs` (`bio_template`).
- Scene/home: `live/video/bevy_renderer/scene/` (`SceneDescription`, `birther`,
  `library`, `builder_api`).
- The spine: `persona/projection.rs` (`PersonaSpec`) — everything above becomes a
  field or a consumer of this.
