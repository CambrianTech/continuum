# Roadmap: The Four Viral-Candidate Milestones

**Status**: Strategic roadmap committed 2026-04-10 by Joel + Claude during the Mixtral 8x7B recovery session. **Strictly sequential execution** — each milestone completes fully before the next begins. No parallelization of the milestones themselves; preparatory code work (adapters, paper outlines, recipe drafts) can happen during forge-compute time on BigMama but never gates-skip the sequence. Joel's explicit instruction: *"in order"* and *"when mixtral is done we will get to these next goals/milestones I just posted"* — the roadmap below strictly honors that order.

**Gate to start**: Mixtral 8x7B completes successfully and lands in `~/sentinel-factory/.factory/line/finished/`. Current status as of 2026-04-10: in flight on BigMama, past the load phase, currently in the activation profile forward-pass phase. ETA to completion: ~40-80 minutes from now pending benchmark eval timing.

**Order of execution** (non-negotiable):

1. **Mixtral 8x22B compacted forge** — the rehearsal-to-show transition
2. **Cross-family anchor table (5+ rows)** — the cumulative methodology proof
3. **Many-Worlds v0 validation** — the category-creation event
4. **Forge-as-a-language paper** — the community-defining contribution

---

## Milestone 1: Mixtral 8x22B Compacted — "The Show"

**The headline we're writing toward**: *"Small lab compacted Mixtral 8x22B to run on a single RTX 5090 gaming PC, 95% benchmark retention, full methodology published. Done in a weekend on consumer hardware."*

**Why this is the viral candidate and not Mixtral 8x7B**: Mixtral 8x22B (~280 GB fp16 source, 141B total params, 39B active) is the largest publicly released MoE that no other lab has successfully compressed with rigorous methodology. A successful compaction to ~180 GB with benchmark retention in the 90-95% range would be the first time anyone has demonstrated rigorously-documented compression of a frontier-class MoE on consumer hardware. The headline writes itself: *"What frontier labs couldn't be bothered to do, a small lab did with a forge methodology and a WD Red Pro."*

### Prerequisites (all must be true before starting)

- [x] Forge-alloy schema supports `expert-activation-profile` + `keep_experts_per_layer` (shipped on `domain-extensibility-refactor` branch, deployed to bigmama)
- [x] Streaming-load patch in `forge_model.py` using on-disk safetensors size for the decision (shipped in commit `3efd4b4`)
- [x] xfs cold tier mounted on BigMama at `/mnt/cold` with HF cache symlinked (shipped 2026-04-10)
- [x] Heartbeat thread hardening (shipped in commit `e299d3c`)
- [x] `priorMetricBaselines[]` field plumbing to `result.json` (shipped in commit `e299d3c`)
- [ ] **Mixtral 8x7B forge completes successfully** — this is the gate. If Mixtral 8x7B fails in eval or publish stages, we diagnose and fix before attempting 8x22B.
- [ ] **Mixtral family adapter validated end-to-end** — the expert-prune stage for Mixtral MoE must work correctly, which we'll know by Mixtral 8x7B's result quality.

### The scale challenge

Mixtral 8x22B at 280 GB fp16 source will exercise every layer of the infrastructure we built:

- **Download**: ~280 GB pulled from HuggingFace to the xfs cold tier. At BigMama's observed ~190 MB/s sustained today, that's **~25 minutes**. At gigabit (125 MB/s), 37 minutes. At 2 Gbit (observed Mixtral 8x7B rate), 25 minutes.
- **Load**: the streaming-load path must handle a model **3x the size** of Mixtral 8x7B. Only ~32 GB GPU + 54 GB CPU (86 GB total) fits in memory; the remaining ~194 GB must spill to `/mnt/cold/hf-offload` via Accelerate's disk-backed layer placement. **This is the first real stress test of the streaming-load + disk-overflow path.**
- **Activation profile**: the forward passes during the profile stage will be **much slower** because most layers live on disk and Accelerate has to stream them in on demand for each forward pass. Instead of ~20 min for Mixtral 8x7B's profile, expect **2-4 hours** for 8x22B's profile.
- **Expert prune**: the prune stage operates on safetensors on disk, not on the in-memory model, so it's not affected by the disk-overflow. Should take ~15-30 min for 8x22B on the xfs cold tier.
- **Quantization**: GGUF conversion of a 180 GB pruned result = ~30-60 min on CPU.
- **Eval**: running HumanEval + MMLU + BBH against a 180 GB model with heavy disk offload = **4-8 hours** of eval wall-clock because every forward pass touches disk.

**Total wall-clock estimate**: **8-16 hours of continuous forge time** for the full pipeline. Possibly longer if the disk offload creates unanticipated bottlenecks in any stage.

### Risks (honest accounting)

**HIGH — Streaming-load + disk-overflow at this scale is untested.** Mixtral 8x7B loaded with zero disk spill because it fit in GPU+CPU. Mixtral 8x22B will spill ~194 GB to disk, and Accelerate has to read those spilled layers every forward pass during the activation profile. **This is the first time we're running the code path where tensors cycle GPU ↔ disk during inference.** It should work (HuggingFace runs this pattern in production for their Inference Endpoints) but we haven't personally verified it. Failure mode to watch for: activation profile stage wedges or runs so slowly that wall-clock blows out past reasonable limits.

**MEDIUM — Eval wall-clock could exceed the practical forge window.** If eval takes 12+ hours on a heavily-offloaded model, we'll want to skip the standard benchmark suite and run a smaller eval just to prove the compaction didn't destroy the model. Or we run eval unattended overnight. This is a tradeoff to decide when we see the profile stage's actual per-forward-pass wall-clock.

**LOW — Disk space on the cold tier.** 280 GB source + 180 GB result + ~50 GB working files = ~510 GB of cold tier usage for one forge. /mnt/cold has 14.6 TB available, so this is fine for one forge, and cleanup between forges will keep it under control.

**LOW — Mixtral expert-prune adapter bugs.** The Mixtral 8x7B run will shake these out. If 8x7B's eval quality is good, 8x22B inherits the same well-tested adapter.

### Concrete plan (once Mixtral 8x7B completes)

1. **Verify Mixtral 8x7B result**: eval metrics meet §4.1.3.4 calibration-anchor thresholds, benchmark retention in the target range, the published model card looks right.
2. **Clean up /mnt/cold** if needed (delete old caches, keep the Mixtral 8x7B source since we'll want it for the cross-family table later).
3. **Queue the Mixtral 8x22B recipe** in `seed_factory_queue.py`. The recipe is structurally identical to the 8x7B recipe — same stages (`expert-activation-profile → expert-prune → quant → eval → publish`), same calibration corpus, different base model (`mistralai/Mixtral-8x22B-Instruct-v0.1`), different keepExpertsPerLayer (probably 8→6 again for consistency, or 8→5 if we want to push harder).
4. **Start the forge** and watch the load phase carefully for the first ~30 minutes to confirm streaming + disk-overflow works. If the load hangs or the CPU memory grows unexpectedly, kill and diagnose. If the load completes cleanly, the rest is a long wait.
5. **Monitor at checkpoints**: after load completion (~45 min), after activation profile (~3 hrs), after prune (~4 hrs), after quant (~5 hrs), after eval (~12 hrs). Log each transition so we know which stage the failure happened in if one happens.
6. **Publish the result** to HuggingFace as `continuum-ai/mixtral-8x22b-instruct-compacted-conservative` or similar. Model card follows the §4.1.3.4 discipline exactly: paired negative baselines in `priorMetricBaselines`, sample paths, alloy chain hash, reference to forge-alloy recipe in the card.

### Success criteria

- [ ] Forged model lands in `finished/` with a clean result.json manifest
- [ ] Benchmark retention ≥ 90% on HumanEval and MMLU (threshold for "95% headline" is 95%, but 90% still a strong result)
- [ ] Published model card with full §4.1.3.4 discipline (paired baselines, sample paths, alloy chain hash)
- [ ] Peak disk usage during forge stays below 1 TB (headroom check)
- [ ] No drvfs hangs, no OOM crashes, no heartbeat wedges (validation of the full infrastructure stack)

### Downstream unlocks

- **Row 3 of the cross-family anchor table** (after qwen3-coder-30b-a3b and Mixtral 8x7B)
- **The viral headline candidate** — first artifact ready for HN submission
- **Validation that the streaming-load + disk-offload path handles models larger than VRAM+RAM** — this is a reusable capability for every future huge forge
- **Empirical timing data** for "how long does a forge of an N-GB MoE take on consumer hardware" — feeds into the forge-as-a-language paper's §5 inference cost section

---

## Milestone 2: Cross-Family Anchor Table (5+ Rows)

**The headline we're writing toward**: *"Continuum-ai's calibration-aware MoE compression methodology: reproducible across 5 independently-trained base model families. Every row in the table is a published forge artifact with paired negative baselines and a reproducible alloy recipe."*

**Why this matters more than any single row**: the §4.1.3.4 methodology paper with one row (qwen3-coder-30b-a3b) is a novel method claim. The same paper with 5 rows across different families is a **generalization claim** — the methodology works, the discipline transfers, the results are consistent. A 5-row table is the difference between *"interesting technique from a small lab"* and *"citable reference methodology for the whole MoE compression field."*

### The five rows (with current state)

| # | Base model | Family | Status | Notes |
|---|---|---|---|---|
| 1 | `qwen3-coder-30b-a3b` | Qwen3 MoE | ✅ **shipped** (morning flagship) | First row. Has the canonical §4.1.3.4 router-gate-L2 negative baseline. |
| 2 | `mistralai/Mixtral-8x7B-Instruct-v0.1` | Mixtral | 🟡 **in flight** | Forging right now. Expected completion tonight. |
| 3 | `mistralai/Mixtral-8x22B-Instruct-v0.1` | Mixtral | ⬜ **milestone 1 above** | Depends on Milestone 1 completing. |
| 4 | `deepseek-ai/DeepSeek-V2-Lite` | DeepSeek MoE | ⬜ **needs recipe + adapter validation** | Fifth-place priority; DeepSeek MoE has a different expert structure (shared experts + routed experts) — may need adapter work. |
| 5 | `ibm-granite/granite-3.0-moe` (or whichever Granite MoE shipped) | Granite MoE | ⬜ **needs re-forge with recovery training** | Previous Granite forge was pulled due to quality issues; needs the recovery training stage added to the recipe. |

**Option**: instead of re-doing Granite, substitute another MoE family. Candidates: `allenai/OLMoE-1B-7B-0924` (small, fast, simple), `Snowflake/snowflake-arctic-instruct` (large, 480B/17B active, defers to per-frontier-catalog), or a future MoE release that shows up between now and when this table completes. **We'll pick the fifth row based on what's in good working order at the time.**

### Prerequisites

- [x] §4.1.3.4 methodology documented in model cards + published artifacts
- [x] Forge-alloy schema supports all the calibration-aware stage fields
- [x] Infrastructure proven on 2 rows (qwen3 and Mixtral 8x7B when it lands tonight)
- [ ] Milestone 1 (Mixtral 8x22B) complete and validated
- [ ] DeepSeek-V2-Lite family adapter verified correct (shared-expert + routed-expert structure)
- [ ] Granite recovery training stage written (or alternative fifth row selected)

### Per-row work estimate

Each new row:

- **~30-60 min of adapter dev work** if the family already has a working adapter (Mixtral, Qwen3)
- **~4-8 hours of adapter dev work** if the family is new (DeepSeek-V2, Granite — each has its own expert layout quirks)
- **~2-6 hours of forge wall-clock** depending on model size
- **~30 min of eval + publish** post-forge
- **~15 min of model card finalization** with §4.1.3.4 discipline

**Total per row**: 3-8 hours for existing adapters, up to 12 hours for new family adapters.

### Risks

**MEDIUM — Family adapter quirks.** Each MoE family has its own expert layout (some have shared experts + routed experts, some have per-layer different expert counts, some have different router activation functions). The Mixtral expert-prune adapter from the qwen3 work needed minor modification; DeepSeek and Granite may need more. Adapter work is debuggable but time-consuming.

**MEDIUM — Calibration corpus transfer.** The §4.1.3.4 calibration corpus was tuned for code + general text for qwen3-coder. Each new family may need a slightly different corpus composition. Risk: eval results look off for a row not because the methodology failed but because the calibration corpus was wrong for that family.

**LOW — BigMama serial compute bottleneck.** Each forge occupies BigMama exclusively. Four more forges (3 new rows + any re-runs) at 3-12 hours each = **12-48 hours of total forge time** for the remaining rows. Possibly longer with retries. This is the biggest single factor in the cross-family-table timeline.

### Concrete plan

After Milestone 1 (Mixtral 8x22B) completes:

1. **Validate DeepSeek-V2-Lite adapter**: spend 2-4 hours reading the DeepSeek-V2 source code, understanding the shared-vs-routed expert structure, and either (a) confirming the existing Mixtral adapter handles it or (b) writing a DeepSeek-specific adapter.
2. **Queue DeepSeek-V2-Lite forge**. Small model (~16B total params), should complete in 2-3 hours. If eval quality is good, ship row 4.
3. **Decide the fifth row**: Granite re-forge with recovery training vs. OLMoE vs. whatever else is in good working order. Pick the cheapest one to get across the line.
4. **Queue fifth row forge**. Another 2-6 hours depending on model size.
5. **Assemble the cross-family anchor table document**: a dedicated page or section that presents all 5 rows side-by-side, with each row linking to its HuggingFace card, its published alloy recipe, its sample paths, and its negative baseline. This document IS the paper's §5.

### Success criteria

- [ ] Five rows in the table, each with a HuggingFace published artifact
- [ ] Each row has a paired negative baseline recorded in `priorMetricBaselines[]`
- [ ] Each row's benchmark retention meets its §4.1.3.4 calibration-anchor threshold
- [ ] Each row's alloy recipe is git-committed and reproducible
- [ ] The table document is ready to drop into the §4.1.3.4 methodology paper as §5

### Downstream unlocks

- **The §4.1.3.4 methodology paper becomes a generalization claim, not a case study** — the paper can be drafted now with actual empirical evidence of methodology transfer across families
- **Community trust accrues to the lab's brand** — 5 rigorously-forged models establishes continuum-ai as a citable reference for MoE compression methodology
- **The forge-as-a-language paper gets its empirical substrate** — each row is one program in the forge-alloy IR

---

## Milestone 3: Many-Worlds v0 Validation

**The headline we're writing toward**: *"Small lab just demonstrated a new framework for combining multiple pretrained LLMs without retraining, runs on a single RTX 5090, outperforms FuseLLM on cross-model cognition transfer, validates the Platonic Representation Hypothesis empirically."*

**Why this is the biggest single swing**: Many-Worlds is a **category-creation event** if it validates. The Platonic Representation Hypothesis (Huh et al., 2024) predicts a universal latent semantic structure in all sufficiently-large pretrained models; Many-Worlds would be the first concrete method to *construct* that structure across heterogeneous architectures with frozen source models. The combination of (a) a novel primitive, (b) a hot citation to PRH, (c) empirical validation against the closest prior art (FuseLLM), and (d) reproducibility on consumer hardware is the kind of paper that lands on the Hacker News front page AND gets cited in the MoE/multi-model coordination literature for years.

### Prerequisites

- [x] Many-Worlds abstract artifact committed (`continuum/docs/papers/MANY-WORLDS-ABSTRACT.md`)
- [x] `priorMetricBaselines[]` field plumbing in the daemon (shipped in commit `e299d3c`)
- [x] FACTORY-PROTOCOL.md v0.1 with the spec for the field (shipped)
- [ ] Milestones 1 and 2 complete — this gives us a stable, battle-tested forge infrastructure before we throw experimental work at it
- [ ] `scripts/adapters/many_worlds_adapter.py` written (~600-800 lines) — the substrate + Project/Read modules + training loops
- [ ] Many-Worlds recipe in `seed_factory_queue.py`
- [ ] Three-way (or five-way) comparison driver script for the §VII validation conditions
- [ ] FuseLLM baseline either located (if a published implementation exists for {Qwen2.5-1.5B, Llama-3.2-1B}) or implemented ourselves
- [ ] Small same-size MoE baseline selected (DeepSeek-V2-Lite is a natural candidate since it'd already be forged from Milestone 2)

### The v0 validation protocol (from MANY-WORLDS-ABSTRACT.md §VII)

Tiny-scale two-model population:

- **Qwen2.5-1.5B-Instruct** + **Llama-3.2-1B-Instruct** (different families, different tokenizers, different training corpora — the heterogeneity is the test)
- **Substrate**: d=128 (small enough to train fast, large enough to carry structure), Gaussian-distribution parameterization
- **Per-model adapters**: ~50M params each, LoRA-style, trained against a fixed substrate
- **Loss**: contrastive alignment + round-trip task fidelity (both terms)

**Five validation conditions** (conditions A-E from §VII.4):

- **A** — text-bottleneck baseline
- **B** — substrate transfer
- **C** — random-substrate negative baseline (the §4.1.3.4 router-gate-L2 analog)
- **D** — FuseLLM head-to-head at equal compute
- **E** — single same-size MoE baseline

**Two falsifiable predictions** that must BOTH hold for the paper to proceed:

1. **B beats A** on at least one downstream metric (substrate transfer preserves information that text serialization loses)
2. **B beats C by a clear margin** (trained substrate beats random substrate — proves the substrate is doing structured work, not just adding parameters)

**Stretch predictions** (nice-to-have but not gating):

3. **B is competitive with D** (Many-Worlds matches or beats FuseLLM at equal compute)
4. **B beats E** (Many-Worlds population competitive with a single same-size MoE)

### Concrete plan

After Milestones 1 and 2 complete:

1. **Write `scripts/adapters/many_worlds_adapter.py`**: the substrate vector space, Project module (linear + Gaussian parameterization), Read module (linear from substrate region to residual-form vector), substrate training stage (contrastive + round-trip loss), per-model adapter training stage. Pattern-follow the existing expert-prune adapter from Mixtral — same additive-structural-surgery shape, inverse operation. **~600-800 lines. ~1 day of code work.**
2. **Write the Many-Worlds recipe** in `seed_factory_queue.py`. One new entry following the schema. ~50 lines. **~30 min.**
3. **Write the three-way comparison driver**: a new eval script that runs the existing benchmark harness under each of the five conditions (A through E) and records results in `priorMetricBaselines[]`. **~150-250 lines. ~2 hours.**
4. **Locate or implement a FuseLLM baseline** for the Qwen2.5-1.5B + Llama-3.2-1B pair at equal compute. **~2-6 hours depending on whether published impl exists.**
5. **Queue and run the Many-Worlds-v0 validation forge**. Population of 2 small models + substrate training + adapter training + five-way comparison eval. **~1 day of BigMama wall-clock.**
6. **Read the results. If both falsifiable predictions hold, proceed. If either fails, diagnose, refine the design, re-run before drafting any paper text.**
7. **Draft the full Many-Worlds paper** using `MANY-WORLDS-ABSTRACT.md` as the spine and the empirical results as §V. **~1 week of writing work.**
8. **Production-scale Many-Worlds-v1 forge**: population of 3-4 medium-sized base models (including the Mixtral 8x7B and 8x22B and DeepSeek-V2-Lite from Milestones 1 and 2), substrate d=512, full ablations. **~3-5 days of BigMama wall-clock.**
9. **Publish the Many-Worlds-v1 artifact** to HuggingFace, submit the paper to arxiv, and consider the HN submission for the combined "Mixtral 8x22B + Many-Worlds" moment.

### Risks

**HIGH — v0 validation might fail.** We explicitly committed (per Kash's discipline gate) to *not drafting paper text* if the validation doesn't support both falsifiable predictions. If B doesn't beat C, the substrate isn't doing structured work and the design is wrong. If B doesn't beat A, the substrate isn't preserving task-relevant information. Either failure means the paper is parked until a redesign and re-test. **Estimated probability of both predictions holding**: 60-70%. I'm genuinely uncertain; this is real empirical risk.

**MEDIUM — Substrate training might be finicky.** Contrastive + round-trip loss is a two-term objective with hyperparameters (loss weights, temperature, learning rate, batch size) that could take multiple runs to get right. We may need to iterate the v0 validation several times before the substrate actually converges well.

**MEDIUM — FuseLLM head-to-head might be hard to set up fairly.** The "equal compute" constraint is real but fuzzy — how do you measure "equal compute" when Many-Worlds trains a substrate once and N adapters, while FuseLLM distills into one student? We'll need to commit to a specific definition (total GPU-hours is the most defensible) and document it carefully.

**LOW — BigMama compute availability.** Milestones 1 and 2 might occupy BigMama for a week or more, pushing Milestone 3 into week 2-3. This is a timing risk, not a capability risk.

### Success criteria

- [ ] Both falsifiable predictions (B > A, B > C) hold on the v0 tiny-scale validation
- [ ] Results recorded in `priorMetricBaselines[]` with §4.1.3.4 provenance
- [ ] Many-Worlds-v0 artifact published (even the tiny one) as evidence the pipeline works
- [ ] Full paper draft with §5 (empirical results) grounded in real measurements
- [ ] Production-scale v1 forge completes and results hold at larger scale
- [ ] Paper submitted to arxiv

### Downstream unlocks

- **Category creation** — Many-Worlds becomes a named thing in the literature
- **The lab's strongest publishable contribution to date** — bigger than any single compacted model
- **Foundation for the forge-as-a-language paper** — Many-Worlds is the first nontrivial program in the forge-alloy IR that tests whether the language is general enough

---

## Milestone 4: Forge-as-a-Language Paper

**The headline we're writing toward**: *"Forge-alloy: a high-level language for AI architecture design. We've compiled 5+ real architectural contributions in this language and are releasing the spec + runtime + reference programs as an open standard."*

**Why this is the deepest and longest contribution**: a language outlasts any individual result. Every paper the lab publishes from now on is one more program in the same language, and other labs that adopt the language become citation sources automatically. The §4.1.3.4 methodology paper cites continuum-ai once; the Many-Worlds paper cites continuum-ai once; but once "forge-alloy" becomes the lingua franca for architectural recipes, every future paper in the field that uses it cites continuum-ai as the language authors. **This is the contribution that, if it lands, defines the lab's place in the field for years.**

### Prerequisites (all must be true — this milestone has the strictest dependencies)

- [ ] Milestones 1, 2, 3 all complete
- [ ] At least 5 programs written in the forge-alloy IR (one per row of the cross-family table, plus Many-Worlds)
- [ ] Enough pattern repetition across those 5 programs to justify language abstractions
- [ ] The IR has accumulated the structural surgery primitives needed for real architectural work (expert prune, activation profile, substrate train, adapter train, calibration corpus handling, compensation LoRA, etc.)
- [ ] Operational lessons documented (the drvfs lesson, the streaming-load lesson, the heartbeat hardening, the on-disk-size vs. computed-size lesson — all of these become "best practices" in the language spec)

### What the paper claims (provisional outline)

Working title: *"Forge-alloy: A High-Level Language for AI Architecture Design."*

Structure (provisional, will be reshaped by what we learn from writing 5 programs):

1. **Introduction** — the problem of architectural-research reproducibility in the post-frontier-labs era, the gap between one-off research scripts and long-lived engineering tooling, the proposal that architectural design deserves its own language (not just libraries)
2. **The language** — the IR (JSON schema + Python/TypeScript types), the stage taxonomy (structural surgery stages: prune, graft, quant, calibrate, eval, publish), the family adapter dispatch pattern, the provenance model (alloy chain hash + signature bundle), the reproducibility contract
3. **The runtime** — sentinel-ai's forge daemon, the disk protocol (intake/assembly/finished/rework with atomic rename primitives), the streaming-load + disk-offload infrastructure, the heartbeat protocol, the crash-recovery semantics
4. **Example programs** — walkthrough of the 5+ real forge-alloy recipes the lab has shipped:
   - §4.1 **qwen3-coder-30b-a3b** (expert pruning with §4.1.3.4 discipline)
   - §4.2 **Mixtral 8x7B compacted** (expert pruning, same methodology, different family)
   - §4.3 **Mixtral 8x22B compacted** (same methodology at frontier scale, disk-offload infrastructure)
   - §4.4 **DeepSeek-V2-Lite compacted** (shared-expert architecture, adapter generalization)
   - §4.5 **Many-Worlds-v0** (additive structural surgery, substrate training, the first non-compression program in the language)
   - §4.6 **Many-Worlds-v1 production-scale** (multi-family coordination, the forge-as-a-language claim made real)
5. **Empirical cost analysis** — wall-clock timing, compute cost in USD, bytes-on-disk for each of the 5 programs, demonstrating that the language operates at consumer-hardware scale
6. **Lessons learned** — the operational failures (drvfs hang, MoE size undercounting, heartbeat thread GIL starvation) and how the language's reproducibility contract made them debuggable and fixable
7. **Related work** — compiler IRs (LLVM, WebAssembly), scientific workflow languages (Snakemake, Nextflow, CWL), ML experiment trackers (MLflow, W&B), the distinction between "logging tools" and "languages"
8. **Release** — the forge-alloy spec published as an open standard, the sentinel-ai runtime published as open-source, an invitation to other labs to write their own programs in the language
9. **Future work** — the surface-level DSL (the v1/v2 language from MANY-WORLDS-ABSTRACT.md §V.6.6), pip/npm/cargo distribution, the Foreman automation layer, federated forge execution across grid nodes

### Concrete plan

After Milestones 1, 2, and 3 complete:

1. **Pattern audit**: read all 5+ forge-alloy recipes the lab has shipped. Identify the patterns that repeat across them (stage orderings, field conventions, error handling, provenance chains). These patterns are the *language's grammar*.
2. **Extract the language spec**: document each pattern as a language feature, with examples drawn from the 5 programs. The spec is pure retrospective formalization — we write down what the language already does, we don't invent anything new.
3. **Draft the paper** using the 5 programs as §4 (the empirical substrate). Each program gets 1-2 pages of walkthrough showing how its recipe expresses the architectural contribution.
4. **Stage surface-language design for a followup paper**: don't ship the DSL syntax in this paper. The IR + runtime + example programs are enough for the first paper. The DSL is paper #5.
5. **Submit to arxiv with the forge-alloy repo release as a companion artifact**. Announce via Twitter + HN + the ML systems community channels. This IS the post where continuum-ai invites other labs to adopt the language.

### Risks

**LOW-MEDIUM — Pattern extraction might reveal the IR is too brittle.** Writing 5 real programs in a language always exposes design bugs. We may find that some of the 5 programs required ad-hoc field additions that should have been first-class language features. The language spec then requires a v0.2 schema migration before the paper is defensible. **Mitigation**: we already did this once — the `ExpertActivationProfileStage` + `keepExpertsPerLayer` additions to forge-alloy were exactly this kind of "pattern surfacing" moment. Future additions will be similar and expected.

**LOW — Timeline risk from upstream milestones.** This paper depends on 3 prior milestones completing. If any of them fail or take longer than expected, this paper slips. Acceptable.

**LOW — Community reception uncertainty.** Systems papers about new languages are notoriously hit-or-miss with reviewers. LLVM took years to be accepted. Nextflow is huge in genomics but unknown in ML. The forge-alloy paper might land hard, or it might be ignored until Many-Worlds v2/v3 drags attention back to it. **Acceptable**: the paper's value isn't just the initial reception; it's the citation trail over the following years.

### Success criteria

- [ ] Paper draft complete with all 5+ example programs documented as §4
- [ ] Pattern audit identifies the language's grammar retrospectively
- [ ] Spec document published alongside the paper
- [ ] forge-alloy repo released as open-source standard
- [ ] Paper submitted to arxiv
- [ ] Announcement post explaining *why* a language is the right abstraction for this work

### Downstream unlocks

- **The lab becomes a citable reference for architectural research tooling**, not just for individual models
- **Other labs adopt the language**, which means every paper they publish about their own architectural work cites continuum-ai
- **The path to continuum-ai as a community hub opens**: a place where architectural research in the open-source ML world centralizes
- **Future papers become cheaper to write** because the language handles the reproducibility and provenance work automatically

---

## Summary: the sequential path

```
Mixtral 8x7B completes (tonight, ~40-80 min from the time of this doc)
    ↓
Milestone 1: Mixtral 8x22B compacted (~1-3 days)
    ↓
Milestone 2: Complete the 5-row cross-family anchor table
  (DeepSeek-V2-Lite + Granite/substitute + table assembly) (~1 week)
    ↓
Milestone 3: Many-Worlds v0 tiny-scale validation
  (write adapter + recipe + drive, run, validate) (~1-2 weeks)
    ↓
Milestone 3b: Many-Worlds v1 production-scale + paper draft (~1-2 weeks)
    ↓
Milestone 4: Forge-as-a-language paper drafting (~2-3 weeks)
    ↓
The viral moment: Many-Worlds + 5-row table + Mixtral 8x22B in the same publication week
```

**Honest total elapsed time estimate**: 6-12 weeks of sustained work from the time Mixtral 8x7B completes tonight. The sequence is strict per Joel's instruction; preparatory code work (adapter drafting, paper outlining) can happen during forge-compute time but does not gate-skip the order.

**The North Star**: the publication week where the lab drops Mixtral 8x22B compacted + the 5-row cross-family anchor table + the Many-Worlds v1 artifact + the §4.1.3.4 methodology paper + the Many-Worlds paper all within ~7 days of each other. That week, if it lands, is continuum-ai's arrival as a publicly-recognized MoE and multi-LLM coordination research lab. **Mixtral 8x7B tonight is the first rehearsal for that week.**

---

## See also

- `MANY-WORLDS-ABSTRACT.md` — the architectural blueprint and empirical validation gate for Milestone 3
- `CONVERSATIONAL-CADENCE-ARCHITECTURE.md` — the Alex architecture that Many-Worlds downstream-supplies
- `grid/GRID-ARCHITECTURE.md` §10.5 — the routing primitive that Milestones 1-4 all consume
- `sentinel-ai/docs/FOUNDRY-FILESYSTEM-SETUP.md` — the operator setup that makes all of the above reproducible
- `sentinel-ai/docs/FACTORY-PROTOCOL.md` — the disk protocol that every milestone's forge output flows through
- `sentinel-ai/docs/FRONTIER-DEFERRED-CATALOG.md` — the candidate base models for populations beyond the 5-row table
