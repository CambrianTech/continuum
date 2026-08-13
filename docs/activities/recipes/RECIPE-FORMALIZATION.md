# Recipe Formalization — identity, registry, and the taxonomy that groups everything

**Parent:** [Recipes](./RECIPES.md) · **Task:** #274 · **Status:** Design, not built
**Created:** 2026-08-13

> Spawning an activity is the most ordinary creative act in the system. Joel, the
> citizens, and the agents must be able to write a recipe and open a room **all the
> time**, without a compiler and without asking permission. Everything below exists
> to make that true and keep it safe.

---

## 1. The audit that forced this

Two populations of recipe exist in the tree today.

| | count | fields |
|---|---|---|
| **Live** (`core/continuum-core/src/experience/recipes/`) | 4 | `purpose, regions, affordances, layout` |
| **Legacy** (`legacy/src/system/recipes/`) | 28 | `uniqueId, version, name, displayName, description, tags, pipeline, ragTemplate, strategy, roles, tools, sentinelTemplates, layout, view, access, isPublic, locked, inputs, room, entityType` |

The live four are `benchmark/hard-rs`, `chat`, `profile`, `video-chat`. The legacy
28 include `coding`, `research`, `newsroom`, `ai-debate-club`, `academy-training`,
`creative-writing`, `factory`, `gan`, `terminal`, `universe`, `dm`,
`multi-persona-chat`, `training-dashboard`, `grid-overview`, and more.

**The rewrite reduced a recipe to PRESENTATION.** `regions` + `affordances` +
`layout` describe what a room looks like and which verbs it offers. Everything
describing how the activity *behaves* was dropped:

- `pipeline` — the command sequence the activity runs
- `ragTemplate` — its context strategy
- `strategy` — `conversationPattern`, `responseRules` (participation!)
- `roles` — `[{role, type, requires, prefers}]` per participant
- `tools`, `sentinelTemplates` — capability surface
- `access`, `isPublic`, `locked` — governance
- `name`, `displayName`, `description`, `tags` — discovery and grouping
- `uniqueId`, `version` — **identity**

This is not a backlog of unbuilt features. It was designed, authored 28 times, and
lost in the port. Several open cards are re-deriving pieces of it from scratch:
#371 (recipe-owned objective), #6 (ground persona in room purpose — legacy had
`strategy.responseRules`), #264's "citizens have no roles" (legacy had `roles`).

**And identity is the specific loss that bit us.** `uniqueId` + `version` were
replaced by a single free-form `purpose` string doing two jobs at once — identity
AND taxonomy. That is why `benchmark` (family) and `benchmark/hard-rs` (instance)
were indistinguishable, why the spawn docs advertised a purpose that resolves to
nothing, and why the resulting rooms silently render as plain chat.

---

## 2. Identity: `RecipeId` (UUID) + `version`, with `purpose` demoted to a label

```
RecipeId   — UUID. THE identity. Stable across content edits.
version    — monotonic. Which revision of that recipe.
purpose    — free-form hierarchical LABEL: "benchmark/hard-rs". Never resolution.
```

**Why UUID and not a content hash.** A shipped recipe is prod-critical and
long-lived; a bugfix to `chat.json` must not orphan every chat room in existence.
Identity must survive content edits, so identity cannot BE the content.

**Where reproducibility lives instead.** A run's receipt records
`(recipe_id, version, content_hash)`. Identity is stable; the exact bytes an exam
was administered under are pinned in the receipt, forever, and independently
verifiable. This is the forge-alloy pattern — artifact identity plus verified hash
— and it is what makes a published benchmark number falsifiable. Putting the hash
in the *binding* instead would have made every recipe edit orphan its rooms.

**`purpose` survives as taxonomy** (§5) and as the human-facing name. It stops
being load-bearing, so the family/instance ambiguity simply stops mattering.

---

## 3. Registry: one resolution path, two ways in

```
                 ┌── shipped: embedded at build, stable ids, prod-critical
RecipeRegistry ──┤
                 └── installed: registered at runtime (authored, generated, peer)
                            │
                     recipe/install  ← THE validation boundary
```

**One lookup.** `registry.get(RecipeId) -> Option<&Recipe>`. `activity/spawn`
resolves an id against the registry and does not care how it got there. No
name-vs-hash-vs-inline branching at the call site.

**`recipe/install` is a first-class verb**, not a filesystem side-effect. It:
1. deserializes and validates (serde + schema), failing loud and naming the file
2. assigns or accepts a `RecipeId`, bumps `version` if the id already exists
3. stores it so it resolves — and, being an ordinary command, is callable by a
   citizen inventing an activity, by the operator, or by a generator

This formalizes what `builtins_with_overlay` already half-does (embedded floor +
disk overlay). Today authoring is "drop a file and hope"; install makes it a
receipted act.

**Spawn accepts either an id or a full recipe.** Passing a recipe inline is just
install-then-spawn — same path, one extra step, so the ergonomic case (a citizen
inventing an activity mid-conversation) needs no special mechanism.

**Partition tolerance.** A room's wall binding carries `(recipe_id, version)` AND
the recipe content. A node at the park with no network must still render its
rooms; addressing by identity while keeping the bytes local is the same shape as
expert paging. Binding by reference alone would make an offline room a blank.

---

## 4. Naming shipped recipes in Rust: constants, not an enum

Joel's question — "constants possibly in an enum, some rust thing".

**Not an enum.** An enum is a closed set. Recipes are open by construction:
generated on the fly, installed at runtime, authored by citizens. An enum would
have to be edited (and the binary recompiled) for every new activity — the exact
"if authoring an activity requires a compiler, activities get hand-made instead"
failure `experience/source.rs` already warns about.

**Named constants over an open registry:**

```rust
pub mod shipped {
    /// The Rust-coder gym. Prod-critical, ships in the binary.
    pub const BENCHMARK_HARD_RS: RecipeId = RecipeId::from_u128(0x…);
    pub const CHAT: RecipeId            = RecipeId::from_u128(0x…);
    pub const PROFILE: RecipeId         = RecipeId::from_u128(0x…);
    pub const VIDEO_CHAT: RecipeId      = RecipeId::from_u128(0x…);
}
```

Typed, greppable, refactorable references to the prod-critical set — so core code
never spells a magic string — while the registry itself stays open. Shipped
recipes get their ids authored INTO the JSON (`"id": "…"`), so the constant and
the data agree and a test pins that every shipped id resolves.

---

## 5. Taxonomy: `purpose` as a URI path, grouping for free

`purpose` becomes an explicit hierarchical path, and grouping falls out of it:

```
chat                       academy/bench/hard-rs/<run>
benchmark/hard-rs          academy/bench/swe-lite/<run>
benchmark/swe-lite         academy/training/<cohort>
```

- **Rooms** live at `academy/bench/<suite>/<run>` — per-run, per your ruling that a
  new benchmark is a new activity. Fresh board, no inherited cards, no inherited
  claims.
- **Recipes** are grouped by prefix with zero extra structure: `benchmark/*` is a
  query, not a registry table.
- **UI** groups by the same prefix and rewrites URLs from it (web only) — sections
  come from the taxonomy rather than a hand-maintained nav.
- **Discovery**: `tags` (restored from legacy) for cross-cutting search that a
  single path can't express.

The tree is the grouping mechanism for rooms, recipes, and navigation at once.

---

## 6. The restored `Recipe` shape

Presentation (live today) plus the behavioural half recovered from legacy:

```
identity     id: RecipeId, version: u32, purpose: String (path), tags: [String]
human        name, display_name, description
presentation regions[], affordances[], layout          ← already live
behaviour    pipeline[], rag_template, strategy, roles[]   ← restore from legacy
capability   tools[], sentinel_templates[]                 ← restore
governance   access, is_public, locked                     ← restore
children     default_child_recipes: [RecipeId]             ← "activities spawn activities"
objective    ActivityObjective                             ← #371, recipe-owned
```

Every field optional except `id`/`version`/`purpose`, so the live four remain
valid unchanged and a minimal generated recipe stays a few lines. `default_child_recipes`
is what makes spawning cascade naturally instead of one call at one dispatcher.

---

## 7. Build order

1. **`RecipeId` + `version` + registry**, ids authored into the four shipped JSONs,
   `shipped::` constants, test pinning each resolves. Purpose demoted to label.
2. **`recipe/install`** as a verb; `activity/spawn` resolves by id and **fails loud**
   on an unknown one (closes the silent-chat-room fallback, which is live today).
3. **Binding carries `(id, version, content)`**; migrate existing bound rooms.
4. **Taxonomy + per-run rooms**: `academy/bench/<suite>/<run>`, dispatch spawns its
   own activity. Closes #346 slice 3.
5. **Restore behaviour fields**, one at a time, each with a live recipe exercising
   it: `roles` first (it is #264's open half), then `strategy`, then `pipeline`.
6. **Port legacy recipes forward** as data — 28 activity types return with zero Rust.

Steps 1–2 remove the silent failure. Step 4 fixes benchmarks. Step 5 is where the
citizens get their roles back.

---

## 8. What this deliberately does not do

- **No enum of activity types.** Closed sets cannot express a system where anyone
  writes a recipe.
- **No room field on work cards.** A card is published where its participants
  stand; the ACTIVITY carries the room. (Retracted proposal, recorded on #274.)
- **No hierarchical purpose fallback** (`benchmark/swe` → `benchmark`). Resolution
  is by id and exact; a missing suite recipe is an authoring act, not a lookup rule.
- **No automatic room lifecycle.** Unchanged from `modules/activity.rs`: a room is
  the durable record of its activity, and nothing expires it on a timer.
