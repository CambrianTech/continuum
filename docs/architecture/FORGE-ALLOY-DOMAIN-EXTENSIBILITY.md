# Forge-Alloy Domain Extensibility — Refactor Proposal

> **Status:** Design proposal — schema-side architecture proposal for forge-alloy.
> **Updated 2026-04-08:** the consumer-side adapter architecture in sentinel-ai
> is mid-sprint and is documented separately at
> [`sentinel-ai/docs/PLUGIN-SPRINT.md`](../../../sentinel-ai/docs/PLUGIN-SPRINT.md).
> The schema work in this doc is **roadmap step 5** of the plugin sprint —
> the consumer-side adapter set is designed to register against the
> `llm-forge` domain extension once it lands. Read the plugin sprint doc
> first for the full state across both repos.
>
> **Companion docs:** [FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md),
> [FACTORY-PIPELINE-UI.md](FACTORY-PIPELINE-UI.md),
> [FACTORY-UX-VISION.md](FACTORY-UX-VISION.md),
> [`sentinel-ai/docs/PLUGIN-SPRINT.md`](../../../sentinel-ai/docs/PLUGIN-SPRINT.md).
> **Author intent:** lock in the universal-blueprint-with-pluggable-domains architecture so it stops getting forgotten and re-violated by future implementation work.

---

## What this preserves from this week's work (read first)

This refactor **cannot lose** the work shipped this week. Three mechanisms guarantee it:

1. **The HF artifacts are immutable.** [`continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k`](https://huggingface.co/continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k) (alloy hash `aa61c4bdf463847c`, 88.4 HumanEval), [`continuum-ai/olmoe-1b-7b-compacted-5b`](https://huggingface.co/continuum-ai/olmoe-1b-7b-compacted-5b) (alloy hash `bba0a92ff0c8bebb`, 36.0 HumanEval), [`continuum-ai/qwen2.5-coder-7b-compacted`](https://huggingface.co/continuum-ai/qwen2.5-coder-7b-compacted) (61.0 HumanEval), and the 14 legacy artifacts all stay live on HuggingFace exactly as they are. Their alloy hashes don't change. Their verify URLs don't change. Their cryptographic chain of custody stays intact. The refactor never touches them.

2. **The methodology paper stays merged on `main`.** [PLASTICITY-COMPACTION §4.1.3.4](../papers/PLASTICITY-COMPACTION.md) (calibration-aware MoE expert importance, two empirical anchors, the +9.7 / +8.0 cross-architecture validation table) and [§4.1.3.4.1](../papers/PLASTICITY-COMPACTION.md) (the calibration-corpus discipline gate) are in the merged paper. The schema refactor doesn't touch the paper. The empirical findings, the negative-baseline anchors, the within-model A/B isolation, and the 13-point ceiling observation all stay exactly as merged.

3. **Work item 4 is the bit-equivalent regression test.** Before any refactor PR merges, a round-trip test loads each of the three published `continuum-ai/*.alloy.json` files from HF, validates them against the new domain-aware schema, and asserts the output is byte-equivalent (or semantically equivalent if field ordering changes). **No work item merges until that test passes on all three artifacts plus every alloy in `forge-alloy/examples/`.** The regression gate exists specifically to make "we lost the morning's work" impossible.

**Can we repeat this week's results?** Yes. The reproducibility test is the same as the regression test: re-author the same alloy through the new domain-aware Factory widget, hand it to sentinel-ai (once its plugin work lands — out of scope for this proposal), and the produced student model is bit-identical to the published artifact. The alloy hash *is* the reproducibility guarantee — that's what forge-alloy's universal core has done since day one. Two architectures (Qwen3MoE and Olmoe) and two within-model A/Bs (Qwen3-Coder-30B router-L2 vs activation-count, OLMoE broad-corpus vs code-corpus) form four reproducible cells anyone with the same hardware can re-run from the published artifacts alone.

**What the refactor changes:** only how the same alloy fields are *organized* in the schema. The fields themselves — every prose section, every benchmark hash, every priorMetricBaseline, every calibrationCorpus reference — stay the same byte content, just relocated from the flat root list into the `llm-forge` domain extension. The published artifacts validate against both the old and the new schema during the migration window.

---

## Re-running the forges (concrete repeatability chain, end-to-end)

The concern that matters is: **can we re-execute the forge that produced each shipped artifact and get bit-identical output?** The schema refactor must not break this chain. Here is the chain for each shipped artifact, what's required to re-run it, and where the schema refactor sits.

### Reproducibility chain (universal — applies to every shipped artifact)

Re-running any shipped forge requires five things, all of which are public, immutable, and unaffected by the schema refactor:

1. **The published alloy file** (`.alloy.json`) from the HF repo — declares all forge parameters. The refactor relocates fields within the JSON but doesn't drop any. **Loaded the same way by sentinel-ai whether the schema is flat-root or domain-namespaced.**
2. **The calibration corpus** (e.g. `calibration/heldout_code300.jsonl`) uploaded inside the same HF repo. SHA-256 of the file is recorded in the alloy's `expert-activation-profile` stage. **Pull the file, verify the hash, you have the exact corpus the forge used.**
3. **The base model** (e.g. `Qwen/Qwen3-Coder-30B-A3B-Instruct`) — public on HuggingFace, downloadable. **Same SHA, same starting point.**
4. **The sentinel-ai script versions** at the commit referenced in the alloy's `integrity.code` field. Currently sentinel-ai's main branch contains everything needed for the morning's two artifacts (qwen3-coder + OLMoE). **`git checkout <sha>` and you have the exact code that ran.**
5. **The deterministic execution path:** prune is deterministic (per-layer top-K from a fixed importance JSON), GGUF conversion is deterministic, llama.cpp greedy decoding at temperature 0 is deterministic. **Same inputs produce bit-identical outputs.**

### Per-artifact reproducibility status (today, before any refactor)

| Artifact | Forge code path | Calibration corpus | Reproducibility status |
|---|---|---|---|
| [`continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k`](https://huggingface.co/continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k) | `expert_activation_profile.py` + `cpu_expert_prune_v2.py --importance-json` (PR #166 + #168) | `calibration/heldout_code300.jsonl` in repo | ✅ **fully repeatable** from main |
| [`continuum-ai/olmoe-1b-7b-compacted-5b`](https://huggingface.co/continuum-ai/olmoe-1b-7b-compacted-5b) | same | `calibration/heldout_code300.jsonl` in repo (+ `heldout_broad300.jsonl` for the negative-baseline cell) | ✅ **fully repeatable** from main, including the negative-baseline cell |
| [`continuum-ai/qwen2.5-coder-7b-compacted`](https://huggingface.co/continuum-ai/qwen2.5-coder-7b-compacted) | `forge_model.py` (dense head pruning) + `compensation_lora.py` (KL distillation) | held-out mix in repo | ✅ **fully repeatable** from main (PR #161 path) |
| `qwen2.5-coder-{0.5b,1.5b,3b}-general-forged` | older `forge_model.py` dense LoRA path | none recorded | ✅ functionally repeatable from main; exact bit-equivalence requires the original git SHA |
| `qwen3.5-{0.8b,2b,4b,9b,27b}-general-forged` and `qwen3.5-*-code-forged` variants | pre-§4.1.3.1 activation-magnitude metric (the global-flat path that the §4.1.3.1 fix replaced) | none recorded | ⚠ **exact reproduction requires `git checkout` to a pre-§4.1.3.1 commit.** Functionally-equivalent reproduction (using the §4.1.3.1-fixed metric) works on current main but produces a *better* student than the original. The original artifacts stay on HF and remain downloadable; they just can't be bit-reproduced from current main without time-travel. |
| `qwen3.5-4b-code-128k-forged` | context-extension stage (YaRN) | n/a | ✅ context-extend code path is in main; reproducible |

The morning's two artifacts (qwen3-coder-30b-a3b + OLMoE) are at the top of this list with **fully repeatable** status. The legacy Qwen3.5 forges have the time-travel caveat — that caveat exists today, before any refactor, and is unrelated to the schema work. The schema refactor neither helps nor hurts the legacy reproducibility status.

### What the refactor must guarantee about reproducibility

**Three explicit guarantees, all enforced by Work item 4 (the regression test):**

1. **Round-trip byte equivalence on every shipped alloy.** Load each published `.alloy.json` from HF, validate against the new domain-aware schema, serialize back, assert byte-equivalent. If field reordering occurs, assert *semantic* equivalence (same fields, same values, no information loss). **Fails the merge if any shipped alloy round-trips differently.**

2. **Re-author equivalence via Factory widget.** For each of the morning's two artifacts: open the published alloy in the new domain-aware Factory widget, save without changes, assert the saved alloy is byte-equivalent (or semantically equivalent) to the input. **Fails the merge if the Factory widget can't reproduce the exact alloy that shipped.**

3. **End-to-end re-forge equivalence (gated on sentinel-ai's plugin work landing separately).** Once sentinel-ai's internal plugin sprint lands (out of scope for this proposal), re-run the forge via `grid/job-submit` against each published alloy and assert the produced student safetensors are bit-identical to the published artifact (sha256 of every shard matches the recorded `integrity.modelHash`). **This is the gold-standard reproducibility test.** It is gated on sentinel-ai work, so it lives as a follow-up test, not a blocker for the schema refactor.

The first two guarantees are bit-equivalent regression gates that run as part of the refactor PR. The third guarantee is the asymptotic goal once the full plugin chain is in place.

### What gets harder if we DON'T do the refactor

The current ad-hoc fields in the published alloys (`expert-activation-profile` stage, `compensation-lora` stage, `calibrationCorpora[]`, `priorMetricBaselines[]`) **don't validate against the current `FORGE-ALLOY-SPEC.md` schema at all.** The two morning artifacts have invalid alloys per the existing spec. Repeating the morning's forge today requires a forge engine that ignores schema validation and trusts the ad-hoc fields. That's a fragile guarantee — any future tightening of the validator drops the morning's artifacts on the floor. **The refactor is what makes the morning's alloys schema-valid going forward**, which is the real protection of this week's work.

---

## TL;DR

[`forge-alloy`](https://github.com/CambrianTech/forge-alloy) was designed from day one as a **universal Merkle-chain-of-custody for any data transformation pipeline**, not just ML model forging. The README's Type Byte enumeration is explicit: model forging is `0x01`, but `0x05` is delivery, `0x06` is evaluation, `0xFF` is custom domain. Photo provenance from a camera enclave to social media, venue tickets from issuance to gate scan, supply chain transactions, document signing — all of these are forge-alloy use cases under the same universal contract.

The **current Continuum-side spec** ([`FORGE-ALLOY-SPEC.md`](FORGE-ALLOY-SPEC.md)) treats forge-alloy as **ML-only by construction**. Every stage type (`prune`, `train`, `lora`, `quant`, `eval`, `expert-prune`, `context-extend`, `modality`) lives directly under `ForgeAlloy.stages` with no domain namespace. Adding ticketing or photo provenance would require either polluting that flat list with cross-domain stage types or building a parallel format — neither acceptable.

The **fix**: refactor the schema so the universal core stays domain-agnostic and the existing ML stages move into an `llm-forge` domain extension. Continuum's Factory widget loads only the domain extensions it cares about. New domains plug in by registering their own stage types without touching the core or any other domain.

This proposal **adds** the refactor as a follow-up to the existing spec, **does not break** any published alloy (every shipped artifact's alloy round-trips identically through the new domain-aware loader), and **scopes the actual work** into 6 sequenced work items totaling ~4 hours of focused effort, all on the Continuum and forge-alloy sides, with **zero edits to sentinel-ai**.

---

## Why this matters

### 1. The current spec is ML-locked but forge-alloy isn't

The forge-alloy [README](https://github.com/CambrianTech/forge-alloy/blob/main/README.md) is unambiguous:

> Stages are **domain-extensible**. The core contract defines the phase structure. Each domain (LLM, vision, audio, diffusion) registers its own stage types. The executor, attestation, and pipeline runner are domain-agnostic.

And the [Type Byte enumeration](https://github.com/CambrianTech/forge-alloy/blob/main/README.md#type-byte-domain-classification):

```
0x01  Model forge       Prune, train, quant — AI model transformation
0x02  Adapter training   LoRA, skill acquisition
0x03  Dataset            Provenance of training data
0x04  Compute receipt    Grid transaction, GPU-hours
0x05  Delivery           Model published/deployed
0x06  Evaluation         Benchmark scores, quality gates
0x07  Vision encoder     Modality addition (CLIP, SigLIP)
0x08  Audio encoder      Modality addition (Whisper)
0xFF  Custom domain      Schema in payload
```

[APPLICATIONS.md](https://github.com/CambrianTech/forge-alloy/blob/main/docs/APPLICATIONS.md) lists non-ML applications explicitly: photo authenticity attestation (camera enclave → edits → publish, decentralized C2PA), supply chain provenance, document signing, ticketing.

The Continuum-side spec we authored ahead of merge ignored all of this. Every stage type listed in `FORGE-ALLOY-SPEC.md` is ML-specific. There is no domain extension mechanism, no domain registry, no way for a non-ML domain to register its stage types without editing the core.

### 2. The gap is already biting us within ML

Even staying inside ML, the existing spec's flat stage list has been pushed past its capacity by the work shipped this month:

- **`expert-activation-profile`** — required by the §4.1.3.4 calibration-aware MoE expert importance methodology. Not in the spec. I authored the [`continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k`](https://huggingface.co/continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k) and [`continuum-ai/olmoe-1b-7b-compacted-5b`](https://huggingface.co/continuum-ai/olmoe-1b-7b-compacted-5b) alloys with this stage type *invented ad-hoc* because the spec couldn't express it.
- **`compensation-lora`** — required by the §4.1.3.3 KL-distillation-against-teacher methodology. Spec has `LoRAStage` but no `lossType`/`teacher`/`kdTemperature` fields. I overloaded `LoRAStage` with non-schema fields.
- **`calibrationCorpora[]`** at the alloy root — required by the §4.1.3.4.1 calibration-corpus discipline gate. Not in the spec. Invented ad-hoc.
- **`priorMetricBaselines[]`** at the alloy root — required for §4.1.3.4 falsifiability (the negative-baseline empirical control that makes the methodology claim independently testable). Not in the spec. Invented ad-hoc.
- **`ExpertPruneStage.expertTensorLayout`** field — required to express which MoE family the alloy is targeting (Qwen3MoE vs Mixtral vs Granite vs DeepSeek-V2 — five distinct module-tree layouts in the wild today, none of which the spec acknowledges). My instinct was to bolt regex hacks into sentinel-ai's `cpu_expert_prune_v2.py`. That was the wrong instinct. The right answer is **declare the layout in the alloy**, let the engine dispatch to its registered family handler.

### 3. Future Continuum domains are blocked

If you (Joel) ever want a **Continuum Ticket Forge** (a Factory-widget UI for issuing venue tickets with cryptographic chain of custody from box-office issuance to gate scan), the current spec blocks it. Same for a **Continuum Photo Provenance Forge** (camera enclave signs the capture, every edit is a signed stage, social media publish is the final stage with the QR code embedded in EXIF). Same for **Continuum Compute Receipts** (grid jobs as alloys, the artifact is the receipt, not a model). Same for any future domain.

The current spec's flat `stages` list with hardcoded ML-only types means each new domain would either fork forge-alloy or live as a parallel format. Neither is acceptable. The forge-alloy README explicitly designs against this.

---

## Proposed architecture

### Universal core (domain-agnostic)

The forge-alloy schema's root contract stays universal. Every alloy has:

```jsonc
{
  // Universal — every alloy carries these regardless of domain
  "name": "string",
  "version": "semver",
  "description": "string",
  "author": "string",
  "license": "spdx string",
  "tags": ["string"],

  // Universal — declares which domain extensions this alloy uses
  "typeByte": "0x01",                    // see README Type Byte enumeration
  "domains": ["llm-forge"],              // ordered list of domain extension ids

  // Universal — any source identifier (model id, photo capture id, ticket batch id)
  "source": { /* domain-specific shape */ },

  // Universal — ordered list of stages, each tagged by stage type
  "stages": [ /* domain-specific stage objects, validated against the
                  domain extensions listed above */ ],

  // Universal — chain-of-custody machinery
  "cycles": "integer",
  "results": { /* domain-specific result shape */ },
  "receipt": { /* universal — publication metadata */ },
  "integrity": { /* universal — signatures, trust tier, code attestation */ }
}
```

The universal core knows nothing about ML, photos, tickets, compute receipts, or any specific domain. It just enforces the chain-of-custody walk and the integrity attestation surface.

### Domain extensions (registered, namespaced)

Each domain ships its own JSON schema fragment that defines **only the stage types it owns**. Domain extensions are referenced by id from the alloy's `domains[]` field. The validator loads each domain extension and validates the alloy's `stages[]` against the union of stage types declared by the listed domains.

```jsonc
// /domains/llm-forge.json — the existing ML stages move here
{
  "id": "llm-forge",
  "version": "1.0.0",
  "typeByte": "0x01",
  "stages": {
    "expert-activation-profile": { /* schema */ },
    "expert-prune":               { /* schema */ },
    "compensation-lora":          { /* schema */ },
    "prune":                      { /* schema */ },
    "train":                      { /* schema */ },
    "lora":                       { /* schema */ },
    "compact":                    { /* schema */ },
    "quant":                      { /* schema */ },
    "eval":                       { /* schema */ },
    "publish":                    { /* schema */ },
    "context-extend":             { /* schema */ },
    "modality":                   { /* schema */ }
  },
  "rootExtensions": {
    "calibrationCorpora":   { "type": "array", "items": { /* schema */ } },
    "priorMetricBaselines": { "type": "array", "items": { /* schema */ } }
  }
}
```

```jsonc
// /domains/ticketing.json — example future domain
{
  "id": "ticketing",
  "version": "1.0.0",
  "typeByte": "0xFF",
  "stages": {
    "ticket-issued":   { /* schema: venue, eventId, seat, holder, issuer signature */ },
    "ticket-transferred": { /* schema: from, to, signature, timestamp */ },
    "ticket-scanned":  { /* schema: gate, scannerId, signature, admit/deny */ }
  }
}
```

```jsonc
// /domains/photo-provenance.json — example future domain
{
  "id": "photo-provenance",
  "version": "1.0.0",
  "typeByte": "0xFF",
  "stages": {
    "capture":     { /* schema: cameraEnclaveId, gpsHash, signature, exif */ },
    "edit":        { /* schema: tool, operation (crop/color/etc), signature */ },
    "publish":     { /* schema: platform, postId, qrEmbed, signature */ }
  }
}
```

### How Continuum's Factory widget consumes domains

The Factory widget is **domain-aware**. When the user picks a recipe to author, the widget asks "which domain?" — `llm-forge`, `ticketing`, `photo-provenance`, etc. The widget loads only that domain's stage editors. The `PipelineComposer` in `continuum/src/widgets/factory/stages/PipelineComposer.ts` becomes a domain-scoped composer; today it implicitly assumes `llm-forge`, the refactor makes that explicit.

A future "Continuum Ticket Forge" widget reuses the same `PipelineComposer` shell, loads the `ticketing` domain extension, and ships a different set of stage editors. The same `grid/job-submit` Rust handler accepts the resulting alloy because the universal core is unchanged — only the consumer engine differs.

### How engines consume domains

An engine declares which domains it supports:

- **`sentinel-ai`** declares `["llm-forge"]`. It refuses to execute alloys with stages from any other domain.
- **A future Continuum-native Candle engine** also declares `["llm-forge"]` once it's built, and competes with sentinel-ai for the same alloys.
- **A camera firmware** that signs photo captures into forge-alloys declares `["photo-provenance"]`. It's a forge-alloy producer, not consumer; the consumer would be a verifier on a phone or social platform.
- **A venue scanner** that verifies ticket alloys at the gate declares `["ticketing"]` as a consumer.

The same `grid/job-submit` machinery in Continuum dispatches to the right engine based on the alloy's `domains[]` field. Engines register with Continuum at startup with their domain support list.

### Backwards compatibility

Every alloy currently published under [`continuum-ai`](https://huggingface.co/continuum-ai) is implicitly `domains: ["llm-forge"]`. The migration path:

1. The new schema defaults `domains` to `["llm-forge"]` when the field is absent (legacy alloys keep validating)
2. The `llm-forge` domain extension contains every stage type currently in `FORGE-ALLOY-SPEC.md`'s flat list, plus the four new ones I invented ad-hoc (`expert-activation-profile`, `compensation-lora`, plus the `calibrationCorpora` and `priorMetricBaselines` root extensions)
3. The validator round-trips every published alloy byte-equivalently — confirmed by a regression test that loads each shipped HF alloy, validates against the new schema, and asserts the JSON is unchanged

No published artifact moves. No alloy hash changes for existing artifacts. The cryptographic chain of custody for every shipped model is preserved.

---

## Work items

The total scope is ~4 hours of focused work, all on Continuum and forge-alloy, **zero edits to sentinel-ai**.

### Work item 0 — Domain registry refactor in forge-alloy (~30 min)

**Repo:** `forge-alloy`. **Files:** `schema/forge-alloy.schema.json`, `python/forge_alloy/types.py`, `schema/domains/llm-forge.json` (new).

- Add a `domains` field to the alloy root schema (array of strings, default `["llm-forge"]` for backwards compat)
- Extract the existing stage types (`PruneStage` through `ModalityStage`) from the root schema into `schema/domains/llm-forge.json`
- The root `AlloyStage.oneOf` becomes `{ "$ref": "#/$defs/domainStageUnion" }` where `domainStageUnion` is computed from the union of stage types declared by every domain in `domains[]`
- Document the domain registry mechanism in forge-alloy README under a new "Domain Extensions" section
- Validator: when loading an alloy, also load every referenced domain extension JSON file from `schema/domains/<id>.json` (or from a registered URL for non-bundled domains)

**Verification:** every alloy in `forge-alloy/examples/` validates round-trip equivalent. Every published `continuum-ai/*` alloy validates round-trip equivalent.

### Work item 1 — `llm-forge` domain extension content (~30 min)

**Repo:** `forge-alloy`. **File:** `schema/domains/llm-forge.json`.

Add the four new stage types and root extensions that I invented ad-hoc and shipped against the live `continuum-ai/qwen3-coder-30b-a3b-compacted-19b-256k` and `continuum-ai/olmoe-1b-7b-compacted-5b` artifacts:

- **`expert-activation-profile`** stage — calibration corpus reference, metric (`activation_count`), max_length, device
- **`compensation-lora`** stage — teacher model, calibration corpus, lossType (`kl_logits|mse_hidden|both`), kdTemperature, loraRank, loraAlpha, targetModules, steps, learningRate, teacherQuant (`8bit|4bit`), studentQuant (`fp16|4bit`), mergedAtSave
- **`calibrationCorpora[]`** root extension — id, name, path, sha256, examples, tokens, distributionSummary
- **`priorMetricBaselines[]`** root extension — id, metric, prune config, evaluation results, samplesPath, outcome enum (`shipped|negative_baseline|superseded`), supersededBy reference, methodologyAnchor URL
- Extend **`expert-prune`** stage with optional `expertTensorLayout` enum (`auto|mlp-experts-unfused|block_sparse_moe-unfused|granite-moe-fused|deepseek-routed-shared`) defaulting to `auto`

**Verification:** the published alloys for the two §4.1.3.4 anchor artifacts validate against the new domain extension. The ad-hoc fields become first-class.

### Work item 2 — Continuum-side TS types from forge-alloy (~30 min)

**Repo:** `continuum`. **Files:** `shared/generated/forge-alloy/`, plus a small Rust crate at `workers/continuum-core/src/forge_alloy/` that owns the types via `#[derive(TS)]` macro per the canonical Continuum pattern.

- Define the universal core types in Rust with `#[derive(TS)]`, generate TS bindings into `shared/generated/forge-alloy/core.ts`
- Define the `llm-forge` domain extension types in Rust (same crate, separate module), generate into `shared/generated/forge-alloy/domains/llm-forge.ts`
- The Factory widget imports from `@shared/generated/forge-alloy/core` and `@shared/generated/forge-alloy/domains/llm-forge`

**Verification:** existing Factory widget code still compiles after the import paths swap.

### Work item 3 — Domain-aware Factory widget (~1 hour)

**Repo:** `continuum`. **Files:** `src/widgets/factory/`.

- `FactoryWidget.ts` gains a `domain` prop, defaulting to `llm-forge` for backwards compat
- `PipelineComposer.ts` filters its registered stage editors by the active domain
- Stage editors for `expert-activation-profile` and `compensation-lora` get added under `src/widgets/factory/stages/` following the existing element pattern (`PruneStageElement.ts` etc)
- `ExpertPruneStageElement.ts` extended with `expertTensorLayout` selector and `importance JSON ref` field
- New top-level editors for `CalibrationCorpusEditor.ts` and `PriorMetricBaselineEditor.ts`

**Verification:** authoring an alloy in the Factory widget that uses all four new stage types produces JSON that validates against the new `llm-forge` domain extension. The output is byte-equivalent to a hand-authored alloy of the same content.

### Work item 4 — Backwards-compatibility regression test (~30 min)

**Repo:** `continuum`. **Files:** `src/widgets/factory/test/` or new `jtag` command.

- Test that loads the published `qwen3-coder-30b-a3b-compacted-19b-256k.alloy.json` from HF, round-trips it through the new Continuum-side type definitions, and asserts byte-equivalent output (or semantically-equivalent if field ordering changes)
- Same test for `olmoe-1b-7b-compacted-5b.alloy.json` and `qwen2.5-coder-7b-compacted.alloy.json`
- Same test for every alloy in `forge-alloy/examples/`

This is the regression gate. **No work item merges until all of these pass.**

### Work item 5 — Documentation refresh (~30 min)

**Repo:** `continuum`. **Files:** `docs/architecture/FORGE-ALLOY-SPEC.md`, `docs/architecture/FACTORY-PIPELINE-UI.md`.

- Update `FORGE-ALLOY-SPEC.md` with a "Domain Extensions" section that points at `llm-forge.json` as the canonical ML domain
- Note that the flat-stages list at the top of the spec is now the contents of the `llm-forge` domain
- Document the four new stage types and two new root extensions
- Update `FACTORY-PIPELINE-UI.md` to reflect the domain-aware widget structure
- Reference this refactor doc as the canonical explanation of the architectural shift

---

## What's NOT in this proposal (out of scope)

### Sentinel-ai internal plugin work (blocked, owned by separate session)

For sentinel-ai's `alloy_executor.py` to actually execute the new stage types (`expert-activation-profile`, `compensation-lora`) on every MoE family the schema can express, sentinel-ai needs an internal plugin/dispatch refactor:

- A `MoeFamilyPlugin` interface (Python ABC) with concrete plugins per family — `Qwen3MoEPlugin`, `OlmoePlugin`, `MixtralPlugin`, `PhiMoEPlugin`, `GraniteMoEPlugin`, `DeepseekV2Plugin`
- Auto-detect from `config.architectures` + module-tree probe, or honor the alloy's explicit `expertTensorLayout` field
- Stage handlers in `alloy_executor.py` that dispatch to the right plugin per stage
- A `VisionSafetyPlugin` for the VL forge work Kash already started in `scripts/vision_safety.py`
- Backwards-compat regression test that re-runs the existing forge path on `qwen3-coder-30b-a3b-compacted-19b-256k`, `olmoe-1b-7b-compacted-5b`, and `qwen2.5-coder-7b-compacted` and asserts bit-identical output

**This work belongs to a sentinel-ai-side session, not this one.** Until it lands, the schema additions in Work items 0–5 are forward-compatible: Continuum can emit alloys with the new stage types, sentinel-ai will fail to execute them with a clear error (`unknown stage type X`), and once the sentinel-ai plugin work lands the same alloys start executing successfully without any Continuum-side change.

### Non-ML domains (deferred)

Building actual `ticketing` or `photo-provenance` domain extensions is deferred until the ML refactor proves the mechanism works end-to-end. Once Work items 0–5 are merged and verified, adding a new domain is documented as: write a `domains/<id>.json` schema fragment, declare the stage types, register engines, build a Factory widget variant. No further core refactor needed.

### Migrating existing models off `llm-forge` to a different domain

Not a goal. `llm-forge` is the canonical ML domain. Splitting it further (e.g. `vision-forge`, `audio-forge`) is a future refinement once we have more empirical data on what stages cluster together.

---

## Decision points needed before starting

I need explicit greenlight on three things before opening any PRs:

1. **Greenlight on the domain registry mechanism shape** — specifically: `domains[]` array on the alloy root, schema fragments under `forge-alloy/schema/domains/<id>.json`, validator loads them dynamically. If you want a different shape (e.g. inline domain declarations, URL-referenced domains, or something else), tell me before I start.

2. **Greenlight on `llm-forge` as the domain id for the existing ML stages**. If you want to call it something else (`continuum-llm`, `cambrian-ml`, `model-forge` to match the README's `0x01` Type Byte phrase, etc), tell me before I start. The id is sticky once an alloy uses it.

3. **Greenlight on the regression-test-blocks-merge rule**. No work item merges until the round-trip regression test passes on all 3 published `continuum-ai/*` artifacts AND every alloy in `forge-alloy/examples/`.

After signoff on those three, I execute Work items 0–5 in sequence and ship the refactor as one or two PRs (forge-alloy + continuum). Sentinel-ai stays untouched throughout.

---

## Why this matters in one sentence

**Forge-alloy's universal-blueprint-with-pluggable-domains design is the moat that lets Continuum become the Factory for any cryptographically-attested data transformation pipeline, not just AI model forging — and the current ML-locked spec is forfeiting that moat one ad-hoc field at a time.** This refactor reclaims it.
