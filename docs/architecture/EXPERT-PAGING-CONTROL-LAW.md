# The Expert-Paging Control Law

**Status:** shipped (rung 2 closed 2026-08-01) · **Code:** `core/expert-pager-policy/`
· **Referenced by:** `expert-pager-policy/src/lib.rs`, `capacity::{expert_decay_policy, plan_file, bandit_plan_controller}`
· **Siblings:** [EXPERT-FETCH-INTERFACE.md](EXPERT-FETCH-INTERFACE.md) (the plant + the WASTE
baseline), [EXPERT-PAGING-GOVERNOR-SEAM.md](EXPERT-PAGING-GOVERNOR-SEAM.md) (resource
arbitration), [../reference/WASTE-EXTRACT.md](../reference/WASTE-EXTRACT.md) (container
byte-layout ground truth).

This document is the control law the crate's rustdoc points at: **what the loop
measures, what it decides, what it actuates, and the measured numbers each rule is
pinned to.** The pager is a classic control system — sensor → estimator → policy →
actuator — with an online learner in the policy seat. Nothing in it guesses; every
constant traces to a replayed measurement.

---

## 1. The plant, and why a control law at all

A streaming MoE (Kimi-K3 class: 61 layers, 384 routed experts/layer, top-k routing)
cannot hold its expert banks in device memory on consumer hardware. Naive streaming
re-fetches every routed expert every token: ~17 GB/token at ~10 GB/s = the WASTE
baseline, **0.32–0.36 tok/s** (measured; see EXPERT-FETCH-INTERFACE.md §baseline).

The exploitable structure: routed-expert activation is heavily **skewed and
temporally clustered** — a small hot set covers most activations over a window, but
*which* set is prompt-dependent and shifts within a serve. That is exactly the shape
a residency controller with an adaptive recency↔frequency dial exploits. Two
measured facts bound the design space:

- **Static pins hurt.** Pinning one serve's hot set for the next prompt cratered
  hit-rate 62% → 34% ([[routed-expert-hotness-is-prompt-dependent-static-pins-hurt]]).
  Any pin list must be small relative to the cache and **rolling** — the adaptive
  window does the bulk of the work; pins are bias, not law.
- **No single decay wins.** Recency-weighted scoring wins conversational phases,
  frequency-weighted wins repetitive phases. A fixed dial loses to an online
  learner that follows the shift (§4).

## 2. The seam: mechanism vs policy

One boundary, one file, two owners ([[expert-pager-is-classic-control-sim-trained-ml-runs-it]]):

| Half | Owner | What it does |
|---|---|---|
| **Mechanism** | C++ `ResidencyCache` in the vendored llama.cpp fork (k3-adopt `f44ba7848`) | Enforces residency per token: LRU window of `window_k` recent experts + pin bias, byte budget, fetch-on-miss from the streaming container. Polls the plan file by mtime each token. |
| **Policy** | Rust `BanditPlanController` (`expert-pager-policy`) | Folds observed activations into the learner, chooses the decay arm, emits `{pins, window_k, budget_bytes, tiers}` as the plan document. |

The mechanism never learns; the policy never touches bytes. They meet **only** at
the plan file — so either side can be replaced, replayed, or simulated against the
other, and the Windows-MSVC driver build stays a leaf crate.

## 3. Observation: the trace, token segmentation, the prefill boundary

- **Sensor**: `GGML_MOE_TRACE_FILE` — 12-byte little-endian records
  `(tkey: u64 = FNV-1a of blk.{layer}.ffn_{gate,up,down}_exps.weight, e: u32)`.
  Three matrices share one expert, so token sets dedup at `(layer, e)`
  (`segment.rs`). The live tail reads only complete records; a partial 12-byte
  tail is normal (stdio-buffered writer) and is left for the next poll.
- **Token boundary** (her exact rule): when a tensor key already seen this token
  reappears, the router cycle wrapped — that record opens the next token.
  Validated against `k3-routed-access.trace`: 83,968 records → 12 segments → 9
  modal decode tokens of ~1472 experts.
- **Prefill→decode boundary** (`PrefillBoundaryDetector`): prefill batches
  activate many experts per segment; the first segment whose size drops below
  0.8× the minimum seen fires the boundary **once**. At that instant the driver
  publishes a warm-start plan from the prefill tail.
- **Why the prefill tail**: prefill routing is *extraction, not prediction* — the
  prompt already routed through the same experts decode will want. Measured
  (two independent derivations, convergent): the prefill **union** covers 65–66%
  of decode experts; the prefill **tail** (last batch) covers 47% at a third of
  the size; a stability selector managed 1.4% and a frequency top-K cratered to
  2.8%. **Recency-of-context beats stability at every scale** — so the warm
  start ships the tail, not an average.

## 4. The scoring law and the online learner

Per-expert EMA activation score at decay `d` (`EmaScoreboard`): each token, every
score `×= d`, then `+1` per activation. `d → 0` is pure last-token recency;
`d → 0.99` is LFU-ish frequency. Resident prediction = top-`budget` by score.

The **decay bandit** (`DecayBandit`) runs the fixed ladder
`DECAY_ARMS = [0.0, 0.3, 0.6, 0.85, 0.95, 0.99]` as parallel arms. Every token,
every arm predicts; every arm is rewarded with its **realized hit fraction**
(EMA at `REWARD_ALPHA = 0.3`); serving uses the argmax-reward arm. The reward is
a hardware counter, not an opinion — the system emits its own training signal.

Pinned measurements (reproduced as deterministic tests in `decay.rs`):
- Offline learned decay: **+5 pts** held-out hit-rate over pure recency.
- Online bandit on the non-stationary trace: **49.8%** vs best-fixed **47.8%** —
  no fixed arm serves both phases; only the learner follows the shift.
- Port discipline: reproduce her numbers **first**, improve after. The constants
  above are load-bearing; the tests fail on whole-point drift.

**The ML ratchet (integrity rule):** every timed run banks `(config → outcome)`
as training corpus for the policy learners. This is legitimate because the
learners learn *system dynamics* (activation skew, phase shifts, fetch costs) —
never task answers. Benchmark integrity rules
([[benchmark-integrity-training-ingests-only-live-spoke-never-eval-forks]]) apply
unchanged.

## 5. Actuation: the plan file

`GGML_MOE_PLAN_FILE` — one small JSON document, written **atomically**
(tmp + fsync + rename in the same directory) so the per-token mtime poll never
sees a torn write (`plan_file.rs`). Field names are the cross-language wire
contract; the tests pin them literally.

v1 knobs: `version`, `budget_bytes` (host-cache budget), `window_k` (recency
window length), `pin_list[]` of `{layer, expert}`.

**Precision extension (the rate-distortion split):** a pin may carry `tier` — an
index into the *container's* declared precision ladder (0 = highest-fidelity
bank) — and the document may carry `default_tier` for the entire unpinned cold
tail. Hot experts serve at high fidelity; cold ones stream from small-quant
banks. Both fields are optional and serde-skipped when absent, so tier-less
documents are **byte-identical to v1**: neither side needs a lockstep upgrade.
`write_tiered_plan(None, None)` degenerates byte-for-byte to `write_plan`
(pinned by test). Note the K3 caveat: its experts already average ~2.0 bpw, so
uniform down-tiering buys only ~0.75× bytes — the tier lever pays off with an
importance-weighted (imatrix) ladder, which is why the precision lane is
imatrix-gated (§7).

**Pin semantics on the mechanism side:** pins are a **score bias**
(+2× `window_k` generation credit in slot reservation), *not* evict-exempt
entries. This is deliberate — the static-pin crater (§1) showed hard pins fight
the adaptive window; bias composes with it.

## 6. Identity discipline

The bandit's scoreboards key opaque `u64`s; the plan file carries **real**
coordinates. `ExpertId {layer, expert}` packs losslessly
(`layer << 32 | expert`) on observation and unpacks on emission
(`controller.rs`). No hashed uids ever reach the wire — a hash there would pin
the wrong experts while every local test stayed green.

## 7. The lever stack (measured, in order)

The campaign sequence against the WASTE baseline (0.32–0.36 tok/s), each lever
independent and stacking:

| # | Lever | State | Number |
|---|---|---|---|
| 1 | Adaptive residency (window + rolling pins + warm start) | shipped | hit-rate ~50–62% in-serve; the enabler for everything below |
| 2 | Expert-count dial (`kimi-k3.expert_used_count=8`, top-8 of 8+shared) | **measured** | **0.53 tok/s = 1.65× WASTE**; bytes/token 1485 vs 4000 MB. Quality guard: perplexity measured after, never blocking |
| 3 | GPU-resident hot experts (#23): promote the hot set to VRAM (`GGML_MOE_VRAM_CACHE_GB`), GPU-native hot path, zero fetch+copy | in build (her lane) | attacks the compute bound that lever 2 exposed (~1.5 s fetch + ~1.5 s compute per token, serial) |
| 4 | Precision-on-miss (tiered plan, §5) | parked, imatrix-gated | needs an importance profile; generating one on the serving box = 5 h of running the bottleneck, so it is a grid job |
| 5 | Prefetch overlap (fetch next token's predicted set during compute) | next | overlaps the two serial halves |

Stack projection on the validated analytical instrument: **~0.9–1.0 tok/s (~3×
WASTE)**. The instrument earned trust by bracketing lever 2 before the run
(predicted 0.55–0.66; measured 0.53).

## 8. Falsifiability

- Every rule above cites a replayed measurement, and the load-bearing ones are
  deterministic tests in-crate (`decay.rs`, `controller.rs`, `segment.rs`,
  `plan_file.rs`).
- Headline speed claims require a ledger row in `benchmarks/RESULTS.jsonl`
  (recorded via `benchmark/record` — commands with receipts, never ad-hoc
  bash).
- The glass box: `GGML_MOE_CAPTURE_FILE` streams per-token
  `PagerCaptureEvent {hit_rate, chosen_decay, per_arm_reward, …}`; the serving
  console (`core/continuum-positron/src/serving.rs` → the web `serving` face)
  renders the arms and events live. If the learner is winning, you can watch it
  win; if it is not, the same panel says so.
