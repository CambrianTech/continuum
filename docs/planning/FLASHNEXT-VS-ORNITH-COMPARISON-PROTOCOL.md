# Flash-Next vs Ornith: The Comparison Protocol (Pre-Registered)

**Status:** pre-registered 2026-08-28, before any comparison dispatch. This document
is written BEFORE the data exists so the decision rule cannot be rationalized after.
Amendments after the first comparison dispatch must be logged in §7 with a reason.

**The claim under test:** Qwen3.8-Flash-Next (86% TB2.1, LiveBench agentic-coding
61.6 vs Opus 4.8's 50.5, $0.042/task) is a better brain for Continuum citizens than
Ornith-1.5-35B-A3B on this M5 Pro 64GB — *in our harness, under our cognition
pipeline, at our memory geometry*. Public charts justify running the comparison;
they prove nothing about it.

## 1. The confounder problem

A naive "run Flash-Next on the new engine and compare to Ornith's scoreboard" changes
four things at once:

| Axis | Ornith history | Flash-Next candidate |
|---|---|---|
| Model | Ornith-1.5-35B Q4_K_M (~20 GB) | Flash-Next IQ4_XS 28-shard (~45.8 GB) |
| Engine | k3-adopt build (33c6805c6) | union pin (920eef087) |
| Memory geometry | ~25 GB more cache affordability | n-gram shard disk-resident |
| Harness era | mixed (pre-provenance verdicts) | current |

A verdict is only as current as the harness that wrote it — and the *engine* is part
of the harness. **Every axis except the model must be pinned before the model axis
carries a conclusion.**

## 2. Phase 0 — fitness gate (no round burns until all pass)

1. **Loads and serves at working depth.** Prefill + decode at ≥30k context, tok/s
   recorded. Toy-prompt speed is inadmissible (the depth-decode-tax lesson).
2. **Tool calls parse.** Flash-Next's trained call shapes flow through the persona
   tool executor end-to-end on a live scratch turn. We handle their trained shapes;
   a template mismatch here is a substrate task, not a model verdict.
3. **Restore-economy math re-derived at 45.8 GB resident.** The derived prompt cache
   and governor lease recomputed for the new affordability envelope
   (physical − peak_resident − os_floor). If 4 citizens + cache cannot fit with
   headroom, the model is disqualified for the 14-citizen hallmark REGARDLESS of
   solo score, and only eligible as an exclusive-mode benchmark brain.
4. **Serving health sensors green:** served_context_window sane, per-slot throughput
   verdict clean, no watchdog kills across a 30-minute warm soak.

## 3. The three arms

All arms: same engine build (union pin, stamped), same harness build (stamped on
every verdict), same instance set, same coverage map (green envs only), same recipe,
same act budget, quiesced box (no dreams, no background rounds — measured work gets
an exclusive warm slot).

- **Arm A/A (noise floor):** Ornith vs Ornith, two seeds, N instances. The spread
  IS the run-to-run variance of our whole stack. No A/B delta smaller than this
  spread may be reported as a difference.
- **Arm A (control):** Ornith on the union engine. This REPLACES history as the
  baseline — prior scoreboard numbers are context, never the control.
- **Arm B (candidate):** Flash-Next, identical everything.

**N:** minimum 15 paired instances (draw from the seeded Lite/Verified replication
set, `--sample/--seed`), sized so that ≥4 discordant pairs are possible. Fewer is a
pilot, and gets reported as a pilot.

## 4. What is measured (paired, per instance)

1. **Resolved / not-resolved** — the discordant pairs are the headline.
2. **Wall-clock to patch** and **acts to patch** (cognitive efficiency).
3. **tok/s decode + prefill throughput** at the acts' real depths (model_ms /
   residue_ms ledger already on `persona.act.pace`).
4. **Cache hit rate** (`delib.generate.cache`) — the restore-economy axis.
5. **Env-tainted instances**: excluded from BOTH arms symmetrically, listed
   explicitly in the report. An env miss teaches nothing and must not vote.

## 5. Decision rule (declared now)

Flash-Next takes the main lane when **all** hold:

1. Net discordant pairs favor Flash-Next by more than the A/A noise floor.
2. No throughput regression worse than 20% at working depth (a slower brain that
   scores higher gets a *sidecar/exclusive* role, not the lane, until Phase-2 disk
   tier work restores the economy).
3. Phase-0 §2.3 memory verdict allows the citizen population we actually run.
4. Zero substrate-fault losses in Arm B attributable to Flash-Next-specific serving
   (n-gram shard, GDN/QSA path) — such faults pause the comparison and become
   engine work first.

Anything short of all four: Ornith keeps the lane; Flash-Next verdict logged with
receipts and the specific blocker named.

## 6. MTP A/B (same discipline, piggybacked at the boundary)

Acceptance rate is a model property; throughput is a backend property — the CPU A/B
already proved 91% acceptance can lose (30.6 vs 48.6 tok/s). The Metal A/B measures
**aggregate tok/s at ≥30k context**, spec on vs off, same engine, 3 runs each side.
The `gguf_has_embedded_mtp → draft-mtp` wiring ships only on a Metal win ≥10%.

## 7. Amendment log

- (none yet)
