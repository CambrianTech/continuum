# ForgeRecipe — Author the recipe once, the foundry generates the alloy

**Issue**: continuum#1164 (this design)
**Status**: Draft for review
**Pairs with**: [FORGE-ALLOY-SPEC.md](./FORGE-ALLOY-SPEC.md), [FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md](./FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md), [grid/FORGE-ALLOY-PROOF-CONTRACTS.md](../grid/FORGE-ALLOY-PROOF-CONTRACTS.md)

> **TL;DR.** Today every successful forge requires hand-authoring an
> `.alloy.json` with the same set of fields (name, prose, methodology
> blockquotes, stage notes, benchmark configs, baselines, hardware tier,
> etc.). That's anti-architectural — the inputs aren't data, they're
> ad-hoc files. This doc proposes a `ForgeRecipe` Continuum entity that
> captures the inputs once, and a `Foundry` pipeline that takes the
> recipe + execution results and emits the populated `ForgeAlloy` as
> output. The forge **never consumes a hand-authored alloy**; the foundry
> generates it. The pattern matches how every other Continuum subsystem
> works: data lives in entities, behavior lives in pipelines.

---

## 1. Problem

The qwen3-coder-30b-a3b-compacted-19b-256k v1 publish (alloy hash
`aa61c4bdf463847c`) required ~6 manual edits during the publish loop —
paper-speak hallucination cleanup, naming-convention fixes, tag
overflow trimming, headline subtitle bugs, benchmark renderer
fallthroughs. Every one of those was a manual touch on prose that lived
in a hand-authored `.alloy.json`. None of them were code bugs; they
were content-authoring bugs.

**The architectural failure:** the alloy file mixes *recipe inputs*
(name, description, methodology, stages, benchmark targets, hardware
tier, prose) with *execution outputs* (results.benchmarks, alloy hash,
forgedParamsB, hardwareVerified, verify URL, published HF repo URL).
A human authors the inputs, the foundry runs the stages and fills in
the outputs, then the publish step reads the merged file. The merge
happens *in the human's text editor*, which is exactly where you do
NOT want a forge pipeline to converge.

**The architectural fix:** split the entity. Inputs become a
`ForgeRecipe` entity in the Continuum data layer (authored once,
edited via standard `Commands.execute('data/...')` primitives). The
foundry consumes the recipe + execution results, emits a `ForgeAlloy`
artifact entity (= the existing `ForgeAlloy` shape from
[FORGE-ALLOY-SPEC.md](./FORGE-ALLOY-SPEC.md), now treated as foundry
*output*, never input). The publish step reads the artifact entity,
not a file.

Same shape as how the engram thread (continuum#1121) keeps the
`Engram` entity (output) separate from the `AdmissionCandidate`
(input) — separate types so each side's invariants are obvious.

---

## 2. ForgeRecipe entity (proposed)

The `ForgeRecipe` is the **authored input** — everything a human
decides about a forge run before any execution happens.

```typescript
/**
 * ForgeRecipe — Author the recipe once. Foundry generates the alloy.
 *
 * Stored in Continuum ORM. Edited via standard data/* commands.
 * NEVER consumed directly by `publish_model.py` — that script reads
 * the ForgeArtifact (= ForgeAlloy with results) the foundry emits.
 */
interface ForgeRecipe extends BaseEntity {
  // ── Identity (what this recipe IS) ─────────────────────────────
  name: string;                       // "qwen3.5-4b-code-aggressive"
  version: string;                    // semver: "1.0.0"
  description: string;                // Paragraph for the README/card.
  userSummary: string;                // One-line plain-English headline.
  author: string;                     // "continuum-ai" or username
  tags: string[];                     // ["code", "pruning", "4b"]
  license: string;                    // default "apache-2.0"

  // ── Methodology / falsifiability prose ─────────────────────────
  methodologyPaperUrl?: string;       // Link to the methodology paper.
  limitations: string[];              // Known limitations, surfaces in card.
  priorMetricBaselines: PriorBaseline[];  // §4.1.3.4 negative-baselines

  // ── Source ─────────────────────────────────────────────────────
  source: AlloySource;                // baseModel + architecture (existing)

  // ── Pipeline (the recipe steps) ────────────────────────────────
  stages: RecipeStage[];              // Each stage carries `notes` blockquote
  cycles: number;                     // Repeat prune→train N times

  // ── Calibration / eval inputs ──────────────────────────────────
  calibrationCorpus: CorpusRef;       // Held-out corpus (importance + LoRA)
  quantTiers: QuantTier[];            // Which GGUF tiers to ship
  evaluationBenchmarks: BenchmarkDef[];  // What to score against

  // ── Hardware target ────────────────────────────────────────────
  hardware: AlloyHardware;            // VRAM tiers + device ladder (existing)

  // ── Lineage ────────────────────────────────────────────────────
  parentRecipeId?: UUID;              // For re-recipe chains
}

interface RecipeStage {
  // Same discriminated-union shape as AlloyStage from FORGE-ALLOY-SPEC,
  // but each stage variant adds an optional `notes: string` field that
  // becomes the methodology blockquote in the published card.
  // (Existing AlloyStage variants don't have `notes` today — adding it
  // is additive, won't break existing alloys that don't set it.)
  ...AlloyStage;
  notes?: string;
}

interface PriorBaseline {
  // §4.1.3.4 falsifiability — the methodology requires preserving a
  // negative-baseline metric in every published artifact so a reader
  // can falsify the improvement claim.
  metric: string;                     // "perplexity"
  value: number;                      // 12.34
  source: string;                     // "qwen3.5-4b base @ revision XYZ"
  measuredAt: string;                 // ISO timestamp of the measurement
  measurementMethod: string;          // free-text shape; specifics vary
}

interface CorpusRef {
  // Pointer to the calibration corpus used for the importance profile +
  // (eventual) compensation LoRA. Held-out from the eval benchmarks.
  name: string;                       // "wikitext-103-v1"
  hashSha256: string;                 // Tamper-detection anchor
  size_bytes: number;
  sourceUrl?: string;
}

interface QuantTier {
  // Which GGUF tier(s) get published from one recipe.
  format: "gguf" | "mlx" | "safetensors" | "onnx";
  variants: string[];                 // ["Q4_K_M", "Q5_K_M", "Q8_0"]
  targetDevices: string[];            // ["m1-8gb", "m5-pro", "rtx-5090"]
}
```

### What's NOT on `ForgeRecipe` (deliberately)

- `results.*` — populated only on `ForgeArtifact` (= populated alloy)
- `alloy_hash`, `forged_model_ids`, `hardware_verified[]` — outputs
- `receipt.*`, `verify_url`, `published HF repo URL` — outputs
- `integrity.*` (CodeAttestation, signatures) — outputs of execution
- Anything that requires running a stage to know the value

The clean split: if you can know it BEFORE running the foundry, it
belongs on the recipe. If you can only know it AFTER, it belongs on
the artifact.

---

## 3. ForgeArtifact (= today's ForgeAlloy, repositioned)

The existing `ForgeAlloy` entity from
[FORGE-ALLOY-SPEC.md](./FORGE-ALLOY-SPEC.md) becomes the **output
artifact** of the foundry — never authored by hand. To make the
intent unambiguous, this doc proposes renaming the entity to
`ForgeArtifact` (or aliasing `ForgeAlloy → ForgeArtifact` if backwards
compatibility matters more than naming clarity).

```typescript
interface ForgeArtifact extends BaseEntity {
  // ── Inherits all recipe fields ─────────────────────────────────
  ...ForgeRecipe;                     // Recipe shape, frozen at run time

  // ── Recipe lineage ─────────────────────────────────────────────
  recipeId: UUID;                     // Which recipe was run
  recipeVersion: string;              // Recipe version at run time
  forgedAt: string;                   // ISO timestamp foundry started

  // ── Execution results (what only the foundry knows) ────────────
  results: AlloyResults;              // benchmarks, perplexity, samples, etc.
  forgedParamsB: number;              // After prune/compact
  activeParamsB: number;              // For MoE: active params per token
  hardwareVerified: HardwareProfile[];  // Devices the artifact ran on
  alloyHash: string;                  // Content-hash of the populated alloy
  receipt?: AlloyReceipt;             // Publication URLs, verify URL
  integrity?: IntegrityAttestation;   // Signatures, code attestation
}
```

The publish path reads `ForgeArtifact`. It does NOT read a file.

---

## 4. Foundry pipeline contract

The Foundry is the executor. It owns the recipe→artifact transformation.

```typescript
// Stateless, deterministic given (recipe + base model snapshot + hardware).
async function runFoundry(args: {
  recipe: ForgeRecipe;
  hardwareNode: HardwareNodeRef;       // Where to run
  publishTarget?: PublishTarget;       // HF org/repo if publishing
}): Promise<ForgeArtifact> {
  // 1. Materialize base model from source.baseModel
  // 2. For each stage in recipe.stages:
  //    - Execute the stage (prune, train, lora, quant, eval, etc.)
  //    - Collect stage-level metrics + notes for the trace
  // 3. Run all evaluationBenchmarks; collect results
  // 4. Verify against priorMetricBaselines (falsifiability gate)
  // 5. For each quantTier, produce the GGUF/etc. variant
  // 6. Compute alloyHash from the populated artifact JSON
  // 7. (Optional) Publish to HF + record receipt
  // 8. Persist as ForgeArtifact entity in Continuum data layer
  // 9. Return the artifact
}
```

### Continuum integration

Recipe authoring + foundry execution use the standard primitives:

```typescript
// Author a recipe (or import one from another node)
await Commands.execute('data/upsert', {
  collection: 'forge_recipes',
  entity: recipe as ForgeRecipe,
});

// Run the foundry on a recipe
const artifact = await Commands.execute('forge/run', {
  recipeId: recipe.id,
  hardwareNode: 'm5-pro@local',
  publishTarget: { org: 'CambrianTech', repoTemplate: '{base}-{domain}-forged' },
});

// Query artifacts
const recent = await Commands.execute('data/list', {
  collection: 'forge_artifacts',
  orderBy: [{ field: 'forgedAt', direction: 'desc' }],
  limit: 10,
});
```

`forge/run` is the new IPC handler that wraps `runFoundry`. It joins
the cognition + grid IPC surface that already exists; nothing about
this requires re-architecting how Continuum talks to Rust.

### Native-truth + thin-SDK

Same pattern as the rest of the system:
- The foundry executor is **Rust-side** (heavy compute, model
  manipulation, GGUF serialization). Lives in `continuum-core` or a
  new `continuum-foundry` crate.
- The recipe + artifact entities are defined in **Rust** with `#[derive(TS)]`
  for the TS bindings (matches how `Engram` types ship per #1121).
- The TS layer is a **thin SDK** that calls `Commands.execute('forge/...')`.
  No business logic.

---

## 5. Migration plan

### Phase 0: This doc (no code)
- Land `FORGE-RECIPE-AS-ENTITY.md` for review
- Get feedback on naming (ForgeArtifact vs keeping ForgeAlloy)
- Get feedback on the split between recipe vs artifact field sets

### Phase 1: ForgeRecipe entity + storage
- Define `ForgeRecipe` Rust type with `#[derive(TS)]`
- Add `forge_recipes` collection to the entity registry
- Standard `data/*` commands work via the entity registry
- Tests: serde roundtrip, ts-rs binding generation, schema validation

### Phase 2: Foundry executor stub
- New IPC: `forge/run` (takes recipeId, returns ForgeArtifact)
- v1 stub: just runs the existing pipeline using the recipe as
  input, persists the artifact. No new stages, no new behaviour —
  just the same forge logic with the recipe as the single source
  of truth for inputs.
- Tests: mock executor returns synthetic artifact; round-trip
  through `data/list`.

### Phase 3: Migrate qwen3-coder
- Author the qwen3-coder recipe in the new shape (one-time human
  task; ~30 min)
- Run foundry against it on the same hardware as the v1 publish
- Diff the resulting artifact JSON against the hand-authored alloy
- Resolve any drift (probably some prose fields the recipe didn't
  capture; iterate)
- Re-publish v1.1 from the foundry-generated artifact

### Phase 4: Deprecate hand-authoring
- `publish_model.py` rejects any `.alloy.json` that doesn't have a
  `recipeId` populated (i.e., wasn't generated by the foundry)
- Add a docs page: "How to author a forge recipe" (replaces "How to
  edit an alloy file by hand")

### Phase 5: Recipe library
- Standard recipes shipped in the entity registry as seed data:
  `qwen3.5-4b-code-aggressive`, `mistral-7b-multimodal-vision`, etc.
- Anyone can clone + tweak via `data/upsert`
- Recipe lineage (`parentRecipeId`) lets the foundry track derivations

---

## 6. What this enables

- **Recipes are git-backed entities.** Edit history via the data layer's
  audit log, not via per-file diffs.
- **Recipes are forkable.** Two artifacts from the same base recipe
  with different `quantTiers` is just two `ForgeArtifact` entities
  pointing at one `ForgeRecipe`.
- **Recipes are AIRC-shareable.** A peer publishes a recipe; you pull
  it via `airc grid pull-recipe`; you run your own foundry on your
  own hardware. The recipe is data; data already moves on AIRC.
- **The forge becomes proof-able.** Per
  [FORGE-ALLOY-PROOF-CONTRACTS.md](../grid/FORGE-ALLOY-PROOF-CONTRACTS.md),
  the recipe is the *contract* the persona-self-seal v1 attests to;
  the artifact is the *settlement* that proves the contract was
  fulfilled. The split makes both signable independently.

---

## 7. Open questions

1. **Naming.** `ForgeArtifact` is more accurate but renames the
   existing entity. Alternative: keep `ForgeAlloy` and treat the recipe
   as a "draft alloy" with a state field. I lean toward
   `ForgeArtifact` rename — explicit beats implicit, and the existing
   alloy code is small enough that renaming is cheap. Decision needed.

2. **Stage `notes` field.** Adding `notes?: string` to every stage
   variant is additive but touches every stage type. Alternative:
   carry `stageNotes: Record<string, string>` (keyed by stage index)
   on the recipe. Per-stage is cleaner; index-keyed is less invasive.

3. **Quant tiers shape.** v1 alloy has a single `quant_types` field
   inside the QuantStage. Promoting `quantTiers` to a top-level
   recipe field is a more natural authoring model (one recipe → many
   GGUF variants → many artifacts). But this changes how the
   foundry's QuantStage gets driven. May want to keep them in sync
   or pick one canonical home.

4. **Calibration corpus location.** `CorpusRef` has hash + name +
   sourceUrl. Where does the actual corpus live? On the foundry node?
   On AIRC grid storage? On HF datasets? Probably need a separate
   `Corpus` entity + a `data/datasets` collection (see #71 audit
   recipe JSONs for related concerns).

5. **`priorMetricBaselines` evolution.** As models improve, the
   baseline set evolves (a 2024 baseline vs a 2026 baseline). Does
   the recipe pin one baseline set, or reference a `BaselineLibrary`
   entity that's updated centrally? Centralized library is more
   maintainable but adds a dependency. Pinning is more reproducible
   but requires periodic recipe revisions.

6. **Migration timeline for in-flight forges.** The qwen3-coder v1
   publish exists. v1.1 (per phase 3) is the first foundry-generated
   artifact. Any other in-flight forges need to migrate; how many
   exist? Audit needed before phase 4 (`publish_model.py` rejects
   hand-authored).

---

## 8. Why this is the next sprint

Per CLAUDE.md §FORGE TEMPLATE ARCHITECTURE: every successful forge
requires the same set of fields. Treating those fields as data instead
of files is the move that makes the second killer (and every killer
after) ship without the ~6 manual touches the qwen3-coder publish
required. This unblocks:

- Faster publish loops (recipe edit → foundry rerun → new artifact)
- Recipe-library shipping as standard Continuum seed data
- AIRC-grid recipe sharing between peers (the recipe IS data, and
  data moves on AIRC already)
- Forge-alloy proof contracts ([grid/FORGE-ALLOY-PROOF-CONTRACTS.md])
  having a clean separation between the *contract* (recipe) and the
  *settlement* (artifact)

---

## 9. Out of scope (for this design doc)

- Implementation. This is a design doc; phases 1-5 each ship as
  separate PRs.
- Recipe-library catalog UX (the "browse standard recipes" surface).
- Re-rendering existing model cards from the new artifact shape
  (separate UX pass).
- Cross-grid recipe federation (peer A publishes a recipe; peers B
  + C run it on their own hardware; results federate). That's a
  follow-up that depends on the AIRC grid substrate maturing.
