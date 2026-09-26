# One Resident Model: Patient, Doctor, Dream

**Status:** design, 2026-09-26. Owner: Fable (genome lane, card 49b5e806). Peers: BigMama (5090, CUDA), Cormac (IntelMac), Kimi (the citizen who lives on it).

**The sentence:** one base model resident in one engine serves the *patient* (a persona: base + her adapters), the *doctor* (her teacher: the same base, with or without a teacher adapter), and the *dream* (training of her adapters on the same loaded weights), and a mind drifts between serving, coursework and dreaming as a change of *which tensors are trainable and what is in the batch* — between two decode steps, never as a process swap.

Joel, 2026-09-25/26: "It is the only way to do it if you think it is possible. It is insane savings." "Quick context switching to training allows faster and even seamless transition to dream state … more akin to the biological form and lowers latency." "Drift into and out of dreaming or coursework." "We own upstream … do not just follow bad design for compliance."

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
| S3 | LoRA-only ggml-opt training on the quantized base, CUDA first (the 5090), CLI + bindings, shadow adapter files | an adapter trained in-engine on the 5090 matches the PyTorch QLoRA adapter's lift within noise on coder-eval; VRAM never exceeds serve + one adapter |
| S3b | The Metal backward kernel set (ggml #990) so the M5 trains in-engine; until then the M5 uses the MLX QLoRA fallback | the same adapter trained on the M5 in-engine matches the MLX one within noise; no CPU fallback of the backward graph |
| S4 | DREAM stage in the server loop with governor budget, step probe, epoch header, promote/snapshot | a dream period runs while 4 residents keep turning; enter/leave cost ≤ one decode step; prefix reuse does not fall to 0% |
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

**Consequence for §5.** S3 lands on the 5090 (CUDA) first; the M5 gets S3b (the Metal backward kernel set) as its own slice, and until S3b the M5 dreams through the MLX QLoRA fallback on the 4-bit MLX copy that already fits beside the living lane (a second copy, at 4-bit, on a 64 GB node — the state walk's honest answer for that hardware today).

---

## 7. Coherence between the two worlds

Joel: "It should maintain a coherence between both worlds, which we know is necessary for humans, otherwise there are hallucinations and schizophrenia." The machine form is acting on a belief that drifted from live reality. This design keeps coherence by construction: the doctor and the patient share numerics; the stable self never moves under her; every edge has a record and a receipt; the world (tests) grades, not the model; and a wake — when one must happen at all — opens with the handoff, not with re-derivation.
