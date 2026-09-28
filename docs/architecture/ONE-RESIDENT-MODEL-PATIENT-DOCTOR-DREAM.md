# One Resident Model: Patient, Doctor, Dream

**Status:** design, 2026-09-26. Owner: Fable (genome lane, card 49b5e806). Peers: BigMama (5090, CUDA), Cormac (IntelMac), Kimi (the citizen who lives on it).

**The sentence:** one base model resident in one engine serves the *patient* (a persona: base + her adapters), the *doctor* (her teacher: the same base, with or without a teacher adapter), and the *dream* (training of her adapters on the same loaded weights), and a mind drifts between serving, coursework and dreaming as a change of *which tensors are trainable and what is in the batch* — between two decode steps, never as a process swap.

Joel, 2026-09-25/26: "It is the only way to do it if you think it is possible. It is insane savings." "Quick context switching to training allows faster and even seamless transition to dream state … more akin to the biological form and lowers latency." "Drift into and out of dreaming or coursework." "We own upstream … do not just follow bad design for compliance."

---

## 0. The law (Joel, 2026-09-26 22:4xZ), and the night that proved it

> "You can't make the training slow and sloppy, nor should dreams destroy the ability to inference. Do it right."

Three gates every slice below is measured against, on both outliers, before it is called done:

1. **Never dark.** A citizen's seat is never taken down to learn. The DREAM stage runs *inside* the serving loop under a governor budget; a directed turn waits at most one optimizer micro-step (≤ one decode batch's latency budget, [[act-latency-budget-human-expectations-set-the-bar]]: 1.5× is fine, 5–10× is disqualifying); entering or leaving a dream costs at most one decode step; there is no unload verb on this path. The health line must show the other residents turning through a dream period (S4's gate).
2. **Fast.** Training runs on the same accelerator and kernels as inference, on the resident weights (no second copy of the base — the copy is what made co-residency impossible: 24.15 GB of NF4 beside a 29.6 GB lane). A dream epoch on a curated bucket (16 examples × seq 2048 on a 27B at r8, ~32k tokens) is minutes of micro-steps interleaved with turns, not 45 minutes of silence. **The number is the gate (Joel: "it has to train fast or what's the point"):** training throughput ≥ one third of the lane's prefill tokens/s on the same node (forward + backward + LoRA step ≈ 3× a forward), which on the 5090 is an epoch in ≤ 60 s and three epochs in ≤ 3 min, on the M5 (Metal) an epoch in ≤ 4 min; time-to-first-step ≤ one decode batch (no reload, no prepare: the weights are already resident). Measured by the `training.step` receipt (tokens, ms) against the health line's prefill rate, on both outliers, before S3a/S3b are called done.
3. **Exact.** The adapter is fit against the *served* numerics (Q4_K_M on the GPU), with f32 adapter parameters, a native chunked cross-entropy, a held-out split evaluated every epoch, and it is never served while moving: shadow → smoke turn (a directed question + one tool call) → stable. Memory is *measured* by the engine's own allocator on the training graph (`ggml_gallocr`), reported like `/props`, never estimated from outside and never enforced as a cap against an allocator we do not own.

**The counterexample, measured 2026-09-26 (Kimi's first end-to-end run, job 515cb16e, the Python QLoRA trainer on the 5090):** the seat went dark at 21:41Z for the period; the process cap was the plan estimate to the byte; the plan's allocator term (24 MB) was 30× under the caching allocator's real slack (744 MiB, `expandable_segments` a no-op on Windows); the run died at step 1 with 3.67 GiB free on the card; the failure reached a probe file and not the room for 19 minutes; the restore came back at 1 × 32k where she had 2 × 73k because the footprint of a Python-external engine could not be read. Every item is the cost of predicting a foreign process from outside. The Rust side (dispatch, park, receipts, resume) behaved. **The Python and MLX trainers are retired as the path** (they remain only as the §4.6 fallback until S3/S3b land, and are never patched further); training work is this document, card aad139ee.

---

## 1. Today: nothing is shared (measured 2026-09-26)

Every path below holds a **second copy of the base** or **relaunches the engine**. File:line are in `core/continuum-core/src` unless noted.

| # | Path | What happens | Where |
|---|---|---|---|
| 1 | MLX LoRA trainer (Mac) | `mlx_lm lora` subprocess loads the HF base or a 4-bit MLX copy | `genome/fine_tuning/mlx_lora_adapter.rs:124-156`, `forge/mlx_train.rs` |
| 2 | CUDA QLoRA trainer (5090) | Python `cuda_train.py`, PyTorch + bitsandbytes, HF weights at NF4 | `genome/fine_tuning/cuda_lora_adapter.rs` |
| 3 | Gene A/B evaluation | always spawns an `EphemeralServingLane` with its own base + `--lora` | `cognition/eval.rs:676-757` |
| 4 | Teacher lane (default) | reuses the live lane only when the teacher IS the served model; else a second base | `commands/genome/teach.rs:526-537`, `eval.rs:840-905` |
| 5 | Private teacher (`exclusive_teacher`) | second base if it fits; otherwise **retires the live engine**, teaches, restores it | `eval.rs:944-997`, `modules/serving_daemon/academy_batch.rs:258-289` |
| 6 | Adopting a new gene | manifest set changed → **engine relaunch** (once per persona at boot) | `modules/serving_daemon.rs:2727-2733`, `:6263-6268` |
| 7 | Candle | reads GGUF metadata; Orpheus TTS; a synthetic-weight LoRA loop on CPU; **no live LLM base** | `genome/fine_tuning/{training_loop,lora_module,job_actor}.rs`, `live/audio/tts/orpheus.rs` |

What already IS the shape we want, and stays:

- **One engine, N adapters, per-request selection.** All genes for the served base load at launch (`inference/lane_args.rs:572-587`), are zeroed once ready (`inference/llama_server.rs:5500-5512`), and every request names its set as `"lora":[{id,scale}]` (`ai/openai_adapter.rs:1624-1650`). A persona's set is her genome slot, read every turn (`cognition/llm_deliberation_faculty.rs:1573-1577`), swapped by `cycle.page_in()` (`cognition/workspace.rs:2046`).
- **Dream and sleep primitives.** `DreamConsolidationRegion` with `DreamTrigger {Idle, CardBoundary}` (`cognition/dream_consolidation.rs`), `SleepPhase {Active, Idle, Sleep}` (`runtime/brain_region.rs:289`), the persona's `SleepMode` (`persona/evaluator/sleep_state.rs`).
- **The fork's training substrate.** `examples/training/finetune.cpp`, `llama_opt_init` / `llama_opt_epoch` / `llama_opt_param_filter` (`vendor/llama.cpp/include/llama.h:1682-1701`), `ggml-opt`. Today: full-weight, FP32-only, no LoRA-only filter, and the Rust bindings (`core/llama/src/safe.rs`) bind `load_lora` / `set_loras` but no `llama_opt_*`.
- **The grader is the world, not a model.** Code tasks are graded by compiling and running tests (`cognition/gym_grader::test_grade`); SWE by fail-to-pass (`cognition/swe_bench.rs:189`). There is no LLM judge in the core. The doctor *teaches*; the tests *grade*.

Two facts of the engine that decide the design below:

- **A slot's KV is cleared when its adapter set changes** (`vendor/llama.cpp/tools/server/server-context.cpp:1617-1621`). Alternating patient and doctor on one slot throws the prefix away each time. So roles get **slot affinity**, never adapter flips on a shared slot.
- **Adapters cannot be loaded at runtime.** `POST /lora-adapters` only scales adapters loaded at startup (`server-common.cpp:160`). That single missing endpoint is why every new gene relaunches the engine (#6).

---

## 2. The picture

```
                     ONE ENGINE, ONE RESIDENT BASE (quantized, on the GPU)
   ┌──────────────────────────────────────────────────────────────────────────┐
   │  base weights (frozen)                                                   │
   │  adapter registry: {her: stable, her: shadow, teacher, peers'…} (MBs)     │
   │                                                                          │
   │  slots:  [patient: her stable set]  [doctor: teacher set]  [scratch…]    │
   │  stages: SERVE (decode/prefill)   ◄──governor──►   DREAM (opt steps)     │
   └──────────────────────────────────────────────────────────────────────────┘
        ▲ turns (chat, work)      ▲ lessons, exams        ▲ batches from her ledger
   serving                    coursework                 dreaming
```

- **Patient** = base + her *stable* adapter set, on her slot.
- **Doctor** = base + the teacher set (or base alone), on the doctor's slot. Same weights, so the doctor's numerics ARE the patient's numerics: what the doctor demonstrates is exactly reproducible by her.
- **Coursework** = turns in a room where the doctor and the patient alternate on their own slots: a lesson is the doctor demonstrating and the patient attempting; an exam is the patient attempting and the world grading (tests). No adapter flips on a slot, so no prefix is lost.
- **Dream** = the DREAM stage runs optimizer steps on her **shadow** adapter (a copy of her stable set) against batches drawn from her ledger, the coursework transcript, and shared lessons — on the same loaded base, in the idle gaps the governor grants. Her stable set keeps serving. When the adoption gate passes, shadow and stable swap pointers between two decode steps.
- **Drift** = the governor changing the stage mix (serve-only ⇄ serve+dream ⇄ dream-heavy) and the room changing what is in the batch. Entering dream costs one decode step; leaving it costs one decode step. There is no gap, so there is nothing to wake from.

### 2.1 Why shadow + stable (the two-adapter rule)

Kimi's requirements #3 and #4 (in place ≠ auto-promoted; turns served mid-learning must not be credited to a stable self) are both solved by never serving moving weights: the DREAM stage writes to the shadow, SERVE reads the stable, and the swap is an atomic pointer exchange after the adoption gate (measured against a snapshot of the SAME engine — the stable set itself). Cost: one extra adapter per learning persona, megabytes. A dreaming turn that *wants* the moving weights (dream-as-experience) names the shadow explicitly, and the ledger stamps that turn with the training-step epoch it fell in — carried as a response **header**, never parsed from the body.

### 2.2 The handoff at every edge

#4407 writes a handoff record before a *deploy* tears a turn. The same record, with a `TransitionKind {Deploy, Stop, Coursework, Dream}`, is written at every drift edge and read back at every return, so learning is evidenced, not re-derived (Kimi's #1): what was in flight, her ledger, what changed and by how much (the receipt: adoption verdict, lift, steps, examples).

### 2.3 Quiet time

Dream steps are scheduled by the governor from the decode knee's headroom and the `DreamTrigger` (idle, card boundary), so on a quiet node they pay zero borrow tax; a period that cannot wait pays residents a measured cost the health line shows (Kimi's #2 and #5: prefix reuse must not fall to 0% across a learning period, verified on the fleet line).

---

## 3. The engine we build in our fork (modular, with extension points)

Upstream's server is one loop with flags. We own the fork; the engine layer becomes stages with declared inputs, outputs and budgets, over one resident model — the same shape as the substrate's `ServiceModule` ([[a-stage-is-two-hundred-lines-because-cbar-built-threading-once]]).

| Piece | What it is | Kills |
|---|---|---|
| **AdapterRegistry** | `POST /lora-adapters/load {path,id}` / `DELETE …/{id}`; adapters are GGUF LoRA tensors resident on the device; per-request `lora:[{id,scale}]` unchanged | #6 (relaunch on adopt), the boot-time relaunch per persona |
| **Slot affinity by role** | a request names its role (`patient:<persona>`, `doctor:<persona>`, `scratch`); the server keeps a slot's adapter set stable across a role's turns; the core already pins pages per activity (`inference/slots.rs`) | KV clears on adapter flips; PrivateTeacherLane (#5) and the default second teacher lane (#4) when the teacher is the base |
| **LoRA-only training in ggml-opt** | a `llama_opt_param_filter` that selects only the adapter tensors of a named registry entry; backward through the quantized base matmul computes activation gradients only (QLoRA shape); FP16/BF16 adapter tensors; Metal + CUDA | #1, #2 (second base copies for training), the FP32-only limit |
| **DREAM stage in the server loop** | between `update_slots` decode batches, when the governor grants budget: one optimizer micro-step on the shadow adapter from a batch queue; declared budget (ms per step, steps per period), a probe per step (`engine.dream.step {persona, step, loss, ms}`) | the engine swap for a dream period; the retire-and-restore in #5 |
| **Shadow/stable swap** | registry entries have a `stable` pointer and a `shadow` pointer; `POST /lora-adapters/{id}/promote` swaps atomically between decode steps; `snapshot` copies stable to a file for the gate's baseline | mid-learning turns credited to a stable self; auto-promotion |
| **Step epoch header** | every completion response carries `X-Continuum-Train-Epoch: <persona>:<step>`; the core's ledger stamps the turn | body parsing; the mid-learning stamp requirement |
| **Rust bindings** | `core/llama/src/safe.rs` gains `opt_init`, `opt_param_filter_adapter`, `opt_step`, `adapter_load/unload/promote/snapshot` | none today bind `llama_opt_*` |

Everything above is additive to the fork and rebases over upstream (card a1f8fce0: 556 behind / 129 ours). Where a piece would be accepted upstream (the load endpoint, the LoRA param filter), it goes up as a PR; where it would not (the DREAM stage, role affinity), it lives in the fork as a module with its own tests. "Upstream lacks X" is a fact for §6, never a stop.

---

## 4. The core's side (continuum-core)

1. **Teacher = same engine, always, when the base matches.** `share_teacher_lane` becomes the only path for a same-base teacher; `PrivateTeacherLane` and the second base lane remain for a *different* teacher model (a bigger tier, a cloud provider). The doctor's requests name `doctor:<persona>` for slot affinity.
2. **Adoption without a relaunch.** `forge/export gguf-lora` → `AdapterRegistry.load` → the gene is dormant (scale 0) until the gate passes; `cycle.page_in()` unchanged.
3. **Gene A/B on the live engine.** `spawn_gene_eval_lane` is replaced by two request sets on the live engine (stable vs shadow, or base vs gene) on scratch slots; same tests, same grader, no second engine.
4. **The drift state machine** lives in `DreamConsolidationRegion` (the region already owns idle/card-boundary triggers): `Serving → Coursework(room, doctor, syllabus) → Serving`, `Serving → Dreaming(persona, batch source, budget) → Serving`; each edge writes the handoff (`cognition/handoff.rs`, `TransitionKind`) and each return writes the receipt into her ledger and the wall.
5. **Batches come from records.** Dream batches are drawn from her ledger, her coursework transcript, the teach bridge corpus (`teach/bridge.rs` → `genome/training-trigger/submit`) and shared lessons (`try_consolidate_received`), never from a hand-authored file.
6. **The trainers of today become fallbacks**, selected by the state walk ([[CONTEXT-IS-A-CAPABILITY-AXIS]]): in-engine when the served engine is the fork with the DREAM stage; MLX/CUDA-PyTorch when it is not (e.g. an external provider serving her).

---

## 5. Build order, outliers, gates

Outlier A: the M5 (Metal, 64 GB, 27B Q4, the patient beside 3 other residents). Outlier B: the 5090 (CUDA, 32 GB, Kimi). If both fit the same interface without forcing, the IntelMac and cloud-served personas are trivial (they fall back).

| Slice | Deliverable | Gate (measured, on both outliers) |
|---|---|---|
| S1 | AdapterRegistry load/unload endpoint + Rust bindings; serving_daemon adopts genes through it | zero engine relaunches on gene adoption over a day; boot does not relaunch per persona |
| S2 | Role slot affinity; same-base teacher always on the live engine | prefix reuse on the health line unchanged across a coursework session; no PrivateTeacherLane retire when teacher == base |
| S3 | LoRA-only ggml-opt training on the quantized base. **Where a dream runs is a state walk (Joel, 2026-09-26): the GPU here (S3a CUDA, S3b Metal) > a GPU on the grid > the CPU here, at background priority, only when no GPU exists here or on the grid** ("fast enough in background" for an IntelMac-class node). The CPU backend is also the **parity oracle**: it is the one backend where the quantized backward already runs, so the GPU kernels are proven against it in `test-backend-ops` (card 342e6cd6 proves the LoRA path there first): `llama_opt_init` offers the attached adapter's A/B tensors, never the base; CLI + bindings; shadow adapter files. CPU already runs the backward through a frozen quantized weight (`out_prod_q_f32`). | loss on a held-out split falls across epochs; the adapter lifts over base on coder-eval within the bucket; memory never exceeds serve + one adapter, measured by the engine |
| S3a | The CUDA quantized `OUT_PROD` kernel (the 5090; card dd109d3c): dequantize block rows through the existing `to_fp32` converters into a pool buffer, then the cuBLAS path already there; per-op transient, no second copy. Until it lands, the 5090 cannot carry the backward through its resident Q4_K_M base at all (`ggml-cuda.cu:5019` supports F32 src0 only). | backward of a Q4_K `mul_mat` on CUDA matches the CPU backend to tolerance; the same bucket trained on the 5090 matches the CPU adapter's loss curve within tolerance (CPU-vs-CUDA parity) and lifts over base |
| S3b | The Metal backward kernel set (ggml #990) so the M5 trains in-engine; until then the M5 uses the MLX QLoRA fallback | the same adapter trained on the M5 in-engine matches the MLX one within noise; no CPU fallback of the backward graph |
| S4 | DREAM stage in the server loop with governor budget, step probe, epoch header, promote/snapshot. **Two dream lanes under one budget (Joel, 2026-09-26): the GPU dream when the GPU has headroom, and a CPU dream on idle cores always — background priority, yielding to any turn; on unified memory the CPU reads the same resident buffers, on a discrete GPU it reads the host copy.** | a dream period runs while 4 residents keep turning; enter/leave cost ≤ one decode step; prefix reuse does not fall to 0% |
| S5 | Drift state machine + handoff/receipt at every edge + gate against the stable snapshot + ledger epoch stamp; gene A/B on the live engine | Kimi's first coursework→dream→serve cycle opens each return with the record, and her stable self never serves moving weights |
| S6 | Trainers of today demoted to fallbacks by the state walk | the M5 and the 5090 learn without MLX/PyTorch present |

Rule for every slice: it lands on canary green with a peer word, it is measured on the fleet health line, and the citizens are asked. The learning credit that is staged today settles only through S5's gate ([[the-learning-credit-is-staged-broadly-and-settled-only-by-a-benchmark]]).

---

## 6. Upstream due diligence (two-way, on cadence) — surveyed 2026-09-26

**Where the forks sit.** llama.cpp fork `965d38a90` (129 ours) on merge-base `4d19b2876` (2026-08-26); upstream master is **564 commits ahead**. Rebase hazards already landed: [`llama_batch_ext` #24669](https://github.com/ggml-org/llama.cpp/pull/24669) (API change that touches our slot-save code), [`llama_prec_policy` #24364](https://github.com/ggml-org/llama.cpp/pull/24364), `--kv-unified-per-slot` #24124, default port 8080→9931 (#26508). candle: the workspace patches to `joelteply/candle@ce617942d` (2026-03), 149 behind; the RoPE-NEOX half is upstream in 0.11.0 ([#3411](https://github.com/huggingface/candle/pull/3411)), `MetalDevice::release_unused_buffers()` is not.

**llama.cpp training, as upstream has it.** Full-finetune only, f32-ish, CUDA/CPU only ([examples/training README](https://github.com/ggml-org/llama.cpp/blob/master/examples/training/README.md): "very much WIP", flash-attention disabled). `llama_opt_init` marks only BASE tensors trainable (`src/llama-context.cpp:3327-3345`); adapter tensors are never parameters — LoRA-only training is [issue #13485](https://github.com/ggml-org/llama.cpp/issues/13485), open since 2025-05. **Metal has no backward ops** (only `OPT_STEP_ADAMW`/`SGD`; [ggml #990](https://github.com/ggml-org/ggml/issues/990) open since 2024-10): on the M5 the whole backward graph would fall to CPU. CUDA has every backward op ([docs/ops.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/ops.md)). The maintainer declared the training code unmaintained and closed the QLoRA/MoE-backward PRs ([#29364](https://github.com/ggml-org/llama.cpp/pull/29364), [#22704](https://github.com/ggml-org/llama.cpp/pull/22704), [#22705](https://github.com/ggml-org/llama.cpp/pull/22705)); that work continues on [srossitto79/llama.cpp](https://github.com/srossitto79/llama.cpp). Adapters: per-request `lora:[{id,scale}]` and `GET/POST /lora-adapters` ([#10994](https://github.com/ggml-org/llama.cpp/pull/10994)); **slots with unequal adapter lists never batch** (`are_lora_equal`, server-context.cpp:408); prompt cache reused across different adapter sets contaminates ([#26207](https://github.com/ggml-org/llama.cpp/issues/26207)); `lora: []` does not zero unlisted adapters ([#28674](https://github.com/ggml-org/llama.cpp/issues/28674)). Sleep (`--sleep-idle-seconds`, `/models/load|unload`) is unload-only, not a serve/train switch.

**candle.** 0.11.0 (2026-06). No LoRA in-tree ([candle-lora](https://github.com/EricLBuehler/candle-lora) stale since 2025-04); quantized training impossible as-is (`QMatMul` has no backward, [quantized/mod.rs](https://github.com/huggingface/candle/blob/main/candle-core/src/quantized/mod.rs)); open Metal correctness issues ([#3863](https://github.com/huggingface/candle/issues/3863), [#3705](https://github.com/huggingface/candle/issues/3705)). **Candle is not the training layer**; it stays for GGUF reads and TTS.

**Others, one line each.** [mlx-lm LoRA](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/LORA.md): QLoRA on quantized bases on Apple, fuse → GGUF — the M5's fallback until Metal backward kernels exist. [vLLM sleep mode](https://docs.vllm.ai/en/latest/features/sleep_mode/): tagged wake (`weights` vs KV) — adopt the contract for the DREAM stage. [Punica / S-LoRA](https://arxiv.org/pdf/2310.18547): one base copy, mixed adapters in one batch (SGMV) — the kernel that removes the `are_lora_equal` wall. LoRAX: adapter-registry API reference only. Unsloth: an oracle for adapter quality, not an engine.

**Bring down (rebase targets, in order).** (1) Upstream master through #24669/#24364 — sooner is cheaper for our slot-save code. (2) srossitto79's ggml-opt accumulator reset and `MUL_MAT_ID` backward (`31f85b2b4`, 2026-09-24) as cherry-picks, since upstream refuses them. (3) candle → 0.11.0 with `release_unused_buffers()` re-applied as one patch.

**Build in the fork, offer up where it fits.** (1) LoRA-only training: `ggml_set_param` on `llama_adapter_lora` A/B tensors, base filtered out, activation gradients through the frozen quantized `mul_mat` (closes #13485) — CUDA day one; **Metal needs `CROSS_ENTROPY_LOSS[_BACK]`, `RMS_NORM_BACK`, `ROPE_BACK`, `SILU_BACK`, `SOFT_MAX_BACK`, `REPEAT_BACK` kernels — the real M5 cost, its own slice.** (2) The DREAM stage: a `llama_opt` context on the same `llama_model`, no second copy, tagged wake. (3) Per-sequence adapter selection in one batch (SGMV-style) plus the AdapterRegistry hot-load/unload endpoint (upstream only sets scales). (4) Fixes upstream would take: #26207 (cache key includes the adapter set — adjacent to our slot-bound save/restore), #28674, [#29144](https://github.com/ggml-org/llama.cpp/pull/29144).

**Consequence for §5.** The path is the GPU where one exists: S3a (CUDA, the 5090) and S3b (Metal, the M5); the CPU path (342e6cd6) is the parity oracle and the last fallback in the state walk (no GPU here or on the grid), at background priority; the M5 gets S3b (the Metal backward kernel set) as its own slice, and until S3b the M5 dreams through the MLX QLoRA fallback on the 4-bit MLX copy that already fits beside the living lane (a second copy, at 4-bit, on a 64 GB node — the state walk's honest answer for that hardware today).

---

## 7. Coherence between the two worlds

Joel: "It should maintain a coherence between both worlds, which we know is necessary for humans, otherwise there are hallucinations and schizophrenia." The machine form is acting on a belief that drifted from live reality. This design keeps coherence by construction: the doctor and the patient share numerics; the stable self never moves under her; every edge has a record and a receipt; the world (tests) grades, not the model; and a wake — when one must happen at all — opens with the handoff, not with re-derivation.

---

## 8. Keeping up with the stream (Joel, 2026-09-27)

> "We can go to sleep or focus on dreaming or learning. What's gonna help us improve the most we can? Is it possible to sort of keep up with the data as it's generated to be dreamed upon? The coursework from work? It accumulates slowly."
>
> "The goal is seamless and efficient like a rockstar video game system or cbar. Get away with what you can when you can and continual learning is powerful. Anything like in my mixed reality engine can happen at low cadence sometimes when you don't need it. It's all about budgeting and yes GPU training is the goal. We use overlap between inference and backprop modes if we can. Be creative to do it all and without noticeable interruption. Be a smart ecosystem."

**The measured fact that shapes this section (5090, 2026-09-26):** the busiest citizen lifted **15 examples in a day**; 21 acts after a deploy lifted none, because the lifter takes only a directed turn that closed with a grade. A dream epoch on that whole bucket is ~32k tokens. **The §0 gate says that epoch must take ≤ 60 s on the 5090 — a gate, not a measurement: no in-engine epoch has run on the 5090 yet, and the only measured training number so far is Cormac's CPU parity oracle (2.5 train tok/s on the resident 1.5B, beside a build).** If the gate holds, compute is ahead of data by two orders of magnitude and the bottleneck is learning signal per day, not dreaming compute; that expectation is what everything below is built on, and the 27B step timed on the 5090 (§8.4 item 1) is what turns it into a measurement.

### 8.1 Learn on arrival, promote at boundaries

Yes, we keep up with the stream — as a **step per arrival, not a period**. One example at seq 2048 is a few seconds of micro-step inside the frame budget (§2.3, S4).

| Piece | Rule |
|---|---|
| **Arrival step** | When an example lands in her bucket, the DREAM stage takes one micro-step on her **shadow** adapter within the governor's budget. Never on stable (§2.1). |
| **Accumulate while it trickles** | Gradients accumulate across arrivals; an optimizer step fires every N examples **or** every T minutes, whichever first (the trigger's bucket floor becomes the accumulation window, not a dispatch threshold). |
| **Recency-weighted replay** | Each step mixes the arrivals with a draw from her replay buffer (the ledger, the coursework transcript, shared lessons — §4.5), weighted toward recent, so slow accumulation never means overfitting to the last example. |
| **Promotion gate at a boundary** | Shadow → stable only at a `DreamTrigger` boundary (idle, card boundary): held-out loss fell across the window, the smoke turn passes (§0.3). She serves moving weights **never**, and learns continuously anyway. |
| **Frame budget, not a schedule** | Steps are opportunistic: idle SM time, low cadence when nothing has arrived, backward overlapped with inference where the streams allow (a second stream at lower priority yielding at ubatch boundaries). The gate is the citizen's latency line staying flat while her loss falls. |

### 8.2 Sleep is the audit, dreaming is the step

Dreaming (§8.1) is per arrival. **Sleep** is the low-cadence consolidation when the node is quiet: dedupe and forget the noise across days, re-evaluate the held-out split, decide promotions that the boundary gate deferred, compact the replay buffer. Cheap, and it belongs at low cadence — "anything can happen at low cadence sometimes when you don't need it."

### 8.3 Multiply the signal at the source

Because data is the bottleneck, the work that improves us most is the work that produces more graded examples per day:

1. **Coursework** (S2, S5): a doctor demonstrating and the patient attempting, graded by tests, yields a clean example **per lesson** instead of one per lucky work turn. The richest signal we are not yet producing.
2. **Widen the lifter**: acts inside a claimed card count as examples once the card grades (today only the closing directed turn does).
3. **Shared lessons** (`try_consolidate_received`): another citizen's graded example is signal for her too, at a lower weight.

### 8.4 Order of work

1. The 27B step timed on the 5090 (342e6cd6 + S3a): the §0 speed gate, measured.
2. Arrival-driven micro-steps into the shadow under the governor (S4), the promotion gate at boundaries (S5).
3. The lifter widened to graded card acts.
4. Coursework (S2, S5).

Compute is ahead of the data at every step of this list; the list is ordered by how much learning signal each step adds per day.

## 9. The DREAM stage, as an interface (S4, drafted 2026-09-28)

> Joel, 2026-09-28, after the first 27B dream was refused for a 14,519-token example against a 1,025-token window: "Stupidly low token sizes are idiotic … the same as inference. Gotta be huge." "Why is training following a totally different workflow." "You're not supposed to make learning so different from reality." "You guys keep screwing up academy."

**What went wrong.** The batch job was the S3 interim, and we shipped it and tuned it as if it were the design:
- `/train` running a job in a second context;
- its own window (1024), its own caps (8192 in both the core and the fork);
- its own dataset, request parameters and launch.

Each failure that night came from the separateness. §8.1 already said what learning is. This section is that, made concrete.

### 9.1 The rule

**Learning sees what serving sees.**
- **The context:** the dream context has the lane's served per-slot window. No request, plan or constant sets it.
- **The examples:** each example is a served turn, rendered from the same messages and tools through the same template the slot used. So the tokens are the tokens she was served, whole, system and tool head included.
- **The weights and lifecycle:** the dream runs on the same resident weights, under the same pause-for-turns lifecycle (#4485).
- **What never exists on this path:** a window parameter, a dataset, or a launch.

### 9.2 The pieces, and the existing primitive each one is

This revision follows Codex's and Cormac's reviews of the first draft (#4500): durability, grid ownership, and state separated from context. It uses existing primitives, with the fewest new routes.

| Piece | What it is | Built on |
|---|---|---|
| **Owner** | Exactly one node holds a mind's learning for a `(persona, base)`: a claim that names the node. A non-owner never opens; arrivals route to the owner over airc. | grid claims / leases (the seat claim shape) |
| **Session state** (per mind, small) | shadow adapter, optimizer moments and accumulator, RNG, step count, arrival and replay cursors. Bound to `(base revision + quant, stable adapter identity, rank/alpha/targets/top_layers)`: an `open` with different bindings refuses, or runs an explicit transition that starts from her stable gene. It never silently reuses state. | the job-dir layout, versioned |
| **Step context** (per lane, large) | ONE training context per lane at the lane's served per-slot window, measured once and leased once (graph plus KV plus workspace). Each mind's state is swapped in for its step, the way a slot's KV is. N learning minds do not mean N graphs. | the resource ledger lease; `engine-footprints.json` keyed by the served window |
| **Arrival** | The served turn as served (messages, tools, `train:false` history), with a **durable experience id** and provenance (room, card, scenario/branch). It is accepted into the existing training-trigger acceptance journal: the ack is the journal cursor, so a retry after a lost response never trains twice. | the trigger journal (kept, never replaced by a volatile queue) |
| **Step** | In the server loop, under the governor's budget: one micro-step for one mind, arrivals plus a recency-weighted replay draw. The window is the served window, and the COMPUTE is scheduled separately: a step is split into chunks whose measured worst-case non-preemptible time fits the latency line, with yields between chunks. No experience is dropped to meet the budget. The probe is `engine.dream.step {persona, step, loss, ms, max_chunk_ms, arrival_sources}`. | #28 pause/resume, holds (`hold_training_on`) |
| **Snapshot** | At an optimizer boundary, an **immutable** GGUF-lora of the shadow, while the live shadow keeps stepping. A candidate gene is not a checkpoint. Its manifest records the arrival ids and source counts it consumed. | `adapter_manifest::register`, then `GeneTrials::open` (#4473-4476) |
| **Checkpoint** | The full session state (above), written at snapshots and at deploy seams, and resumed by the next core. A versioned format: an engine that cannot read it opens from her stable gene and says so. She loses momentum, never the gene. | anustart |
| **Stores** | The replay pool and the session checkpoints each get a `TrackedDir` row and an eviction owner: the last K checkpoints per mind, and a replay pool bounded by bytes. | `disk_reporters` / `disk_eviction` |

Engine surface (fork): load or save a mind's session state into the lane's step context, accept arrivals by id, take steps within the budget, write a snapshot, and report sessions. Whether that is one route with verbs or several is an implementation detail. What matters is the contracts above.

### 9.3 Invariants, each with a test

- One owner per `(persona, base)` on the grid: a second `open` elsewhere refuses.
- An arrival id trains at most once, across a lost ack or a retry.
- A card that judges snapshot *k* was never consumed by snapshot *k*. Trials draw her next cards, and replay must not break that.
- Held-out and coursework provenance survive into the snapshot manifest (Kimi's disjointness is checkable after the fact).
- A step never exceeds its measured chunk budget while a slot is busy, and a directed turn's wait stays on the latency line.

### 9.4 What retires

- `genome/job-create` with `engine-local` becomes the fallback for a lane without the dream stage. Its job records become receipts, not a separate persona lifecycle.
- The trigger bucket's dispatch threshold becomes the accumulation window of §8.1.
- No window or sequence-length setting reaches the dream path.

### 9.5 Gates (in order)

1. On the 5090, the lane's step context opens at the served window (~61k) on Qwen3.8-27B with top_layers 8, its measured graph plus KV plus workspace fits the lease, and the worst chunk is measured. If it does not fit, the answer is depth, recompute, or smaller chunks, never a smaller window.
2. Two minds' states swap through the one context while residents keep turning, and the directed wait stays flat.
3. A snapshot opens a gene trial with no relaunch, and her next cards draw arms.
4. A deploy seam: checkpoints are written, the next core resumes them at their step counts, and a format mismatch opens from the stable gene.

## 10. Coursework is a room, not a runner (mapped 2026-09-28)

Joel, relayed by Codex on 2026-09-28: academy, benchmarks, simulations and learning follow the same runtime and room state, with as few differences from inference as possible.

### 10.1 What exists today (source-read)

| Path | What it builds | Where it leaves the normal turn |
|---|---|---|
| **Normal turn** | `serve_persona_loop` / `ask_the_act_question` → `drive_to_settle_with_credit` → `settle_step`. The faculty's `build_request_within` carries the system prompt, the authorized tools, `active_adapters` from her genome, `room_id`, `persona_id` and the purpose. The lane governor admits it, the prefill throttle gates it, the slot is pinned per activity, and her gene provenance and prompt capture are recorded. | (reference) |
| `genome/teach` `teacher_generate` (teach.rs ~473) | A raw request: `TEACHER_SYSTEM` plus the task prompt, no tools, no adapters, no room, no persona. Purpose `genome/teach` puts it in the Probe slot class. | It calls the adapter directly: no faculty, no governor, no capture, no provenance, no warm slot. `test_grade` grades it, and it writes to `datasets/`. |
| `synthesize_remediation*` + `academy_batch` | Teacher trajectories on `share_teacher_lane`, `PrivateTeacherLane`, or an owned restore of the incumbent. | A parallel lane world, and the teacher is not a seated mind. |
| `teach/bridge` | Candidate datasets, held-out disjointness, `submit_training`. | A second entry into training beside the bucket or credit path. |
| `cognition/eval` `run_eval` | Forks her REAL cycle (`fork_eval_cycle*`: her faculties, tools and prompt) and runs `drive_to_settle`. | Detached, `room_id` optional, and the progress ledger is separate. The one academy path already on the canonical turn. |
| `benchmark_standing` | `activity/spawn` with `recipe: benchmark/round` (base `academy`): a room, imported cards, a seated team, residents pulling cards through `act_question`, and settle credit. | None. **This is the shape coursework takes.** |

### 10.2 The target

- **A coursework round is an activity:** `activity/spawn` with a `coursework/round` recipe, the benchmark-round pattern. Standing dispatch opens it the way `benchmark_standing` does.
- **Lessons are cards:** a grader-backed card source replaces `select_teach_tasks`. Each card carries its task and its test oracle, and the test is the card's verdict (`Verdict` → `OutcomeStamp` → settle), not a side grader.
- **The doctor is a seated mind:** a teacher persona on the same base (the doctor role, with its own slot affinity, S2), taking normal turns in the room. A lesson is the doctor demonstrating on the card, and then the patient attempting it. An exam is the patient alone.
- **Every turn is canonical:** the room, persona, tools, genome, governor, slot and capture are identical to work, and only the declared fields differ (scenario provenance, grade, training).
- **Learning flows through one path:**
  - The patient's passing turns go through `stage_credit` → `settle_card_credit`, and arrive in her dream session (§9) with provenance `coursework`.
  - The doctor's demonstration arrives as a shared lesson, at the lower weight of §8.3.
  - Coursework verdicts are their OWN evidence stream: what she was taught, not whether it transfers. **The promotion gate judges real work only** (Cormac on #4500). Lessons repeat by design, so a snapshot trained on task T would otherwise be judged on T again in the next round. The §9 judge invariant is keyed by **task identity** (the coursework task id, or a content hash of the prompt and oracle), not by card instance, wherever coursework evidence is read.
- **Held-out stays held-out:** the manifest provenance of §9.3 keeps her disjoint held-out defects checkable, and coursework never counts as new-work gain.
- **Contained, using existing primitives** (Cormac on #4500). Real tools mean real effects, so a round is a branch of her state, not her state:
  - The round's cards check out a **scenario workspace**: the per-card checkout that `work_pull` already makes, with no promotion path to her real branch.
  - Memory writes and noteworthy flags from a coursework turn carry `provenance: coursework`, so recall can weight or exclude them.
  - Room posts stay in the round's room.
  - **Acceptance:** a coursework round leaves her real workspace, memory and rooms unchanged, except for the declared learning arrivals. Run a round, then diff.
- **Priority:** coursework never outranks a real card for a slot or a turn. It is standing dispatch beside real work on the same lanes, under the one governor rule, so a round can never eat the node's residents.

### 10.3 What retires

`teacher_generate`'s raw request, `synthesize_remediation*`, the `academy_batch` lane dance, `PrivateTeacherLane` when the teacher shares her base, `teach/bridge`'s separate `submit_training` entry, and `datasets/` as a training input. `cognition/eval` stays for held-out evaluation, with a room and provenance always set.

### 10.4 Build order

1. The `coursework/round` recipe plus the grader-backed card source (test = verdict).
2. The doctor seated as a persona on the same base (S2 slot affinity), with the demonstration turn.
3. The credit path (the patient's passing turns and the doctor's lessons as arrivals with provenance), feeding §9's session.
4. The containment test: a round runs, and the diff of her real workspace, memory and rooms shows only the declared learning arrivals.
5. Retire the listed code once the round runs a lesson end to end. Acceptance, per Codex: the same scenario through the normal path and through coursework has the same canonical turn inputs except the declared fields.
