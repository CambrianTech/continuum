# The Open-Source Coding-Agent Campaign — order of battle

**The reader we must convince** (Joel 2026-07-11): a skeptical dev with an M-series Mac
and a local GGUF, currently on aider / opencode / ollama+X. What converts them, in order:
1. **Beat their current tool on its own turf with their model** — Aider polyglot (the
   aider community's home metric) and Terminal-Bench are the HEADLINE benchmarks; a win
   there is legible to the exact audience we want without explanation. Humaneval is
   smoke-test only — saturated, gamed, convinces nobody.
2. **The claim nobody else can make:** your local agent got BETTER overnight. One chart,
   same weights before/after genome, the LoRA published on HF, reproducible by a
   stranger in 30 minutes. The viral unit.
3. **A ledger, not a landing page:** honest zeros stay published; every row carries its
   replication command. Falsifiability converts skeptics, and skeptics repost.
4. **Thirty minutes to first proof:** install → point at their existing GGUF → reproduce
   one claimed number, or the rest is fiction to them.

Hermes below is ONE opponent among several (aider, opencode, ollama-based stacks); the
axes apply to all of them.

Standing goal (Joel, all week): **beat Hermes with a variety of common models — clear,
undeniable, reproducible wins.** This file is the strategy; the evidence ledger
(`benchmark/record` → `benchmark/matrix`) is the scoreboard. A claim without a ledger
row does not exist.

## The three axes of victory

| Axis | Claim | How it's measured |
|---|---|---|
| **A. Models** | common base + our genome ≥ Hermes fine-tune at equal/smaller size | same benchmark slice, `model+gene` row vs `hermes-*` row |
| **B. System** | ANY model — including Hermes's own — scores higher through OUR harness than raw | same model, `raw` arm vs `ours` arm |
| **C. Cost** | the wins land on ONE MacBook; $/resolved-task printed beside score | `wallSeconds` + hardware field on every row |

Axis B is the one nobody else can even attempt: improving your competitor's model with
your system is checkmate framing.

## Opponents (weights on disk)

- `NousResearch/Hermes-3-Llama-3.1-8B-GGUF` Q4_K_M — cached, `models.json` row 1
- `NousResearch/Hermes-4.3-36B-GGUF` Q4_K_M — their current flagship-mid, downloading
- Hermes Agent (their harness) — not public; represented by their models' raw arm +
  their published numbers, cited with source URLs in row notes

## Order of battle

**Round 1 — tonight (function level, fast, winnable):** humaneval-rs 20-task slice.
Cells: hermes3-8b×raw, hermes3-8b×ours, devstral-24b×raw, devstral-24b×ours,
devstral-24b×ours+coder-act-transition (tonight's gene), qwen3.5-4b-forged×ours.
Runner: `cu benchmark/run` (ours arm) + `benchmarks/coder/oneshot_opponent.py` (raw arm).
Every cell → `cu benchmark/record` with replication cmd → `cu benchmark/matrix`.

**Round 2 — next (agentic level, the lever):** swe-bench-lite 10-instance slice, same
cells + hermes-4.3-36b both arms. This is where Axis B matters most: Hermes models are
POST-TRAINED for native tool-calling — if our harness lifts them anyway, the system
claim is proven on hostile ground.

**Round 3 — scale:** full slices, more common models (llama, gemma, mistral tiers),
publish the matrix + replication doc. Cloud rows later per Joel.

## The roster — reasonable weights for a 64GB M-series Mac

Eval serves ONE model at a time in an ephemeral lane (snapshot-eval #59), so 64GB
unified memory caps single-model size, not roster size. Q4_K_M throughout.

**Core six (phase 1 — every cell, both arms):**
| model | size | why | on disk |
|---|---|---|---|
| Hermes-3-Llama-3.1-8B | 8B | the named opponent, small tier | ✅ |
| Hermes-4.3-36B | 36B | their CURRENT flagship-mid | ✅ 20GB |
| Devstral-Small-2507 | 24B | our resident persona base + tonight's gene | ✅ |
| Qwen2.5-Coder-14B | 14B | the most-downloaded local coder mid | ✅ |
| Qwen2.5-Coder-32B | 32B | the local-coder ceiling most 64GB users run | ⬇ pulling |
| qwen3.5-4B-code-forged | 4B | OURS — the tiny-forged-beats-big thesis | ✅ |

**Wave 2 (after core six have cells):** Qwen2.5-Coder-7B, Phi-4-14B, Gemma-3-27B,
Hermes-4-14B, Llama-3.1-8B. **Ceiling row (optional):** Llama-3.3-70B Q4 (~40GB — fits
solo, tight; run last).

## Published numbers to replicate (researched 2026-07-11)

Aider polyglot (their board, 225 Exercism exercises, % correct) — the 64GB-class rows:

| model | aider's published score | our target |
|---|---|---|
| Qwen3-32B | **40.0%** (diff format) | replicate, then beat with same weights |
| Qwen2.5-Coder-32B | **16.4%** (whole format) | replicate, then beat |
| Codestral 25.01 | 11.1% | reference |
| Gemma-3-27B | 4.9% | reference |
| (context: cloud top = gpt-5 88%; DeepSeek-V3.2 74.2% but 600B-class) | | |

Source: https://aider.chat/docs/leaderboards/ . The strategic read: **local models get
crushed on polyglot under aider** — the 64GB ceiling on their board is 40%. Any run
where OUR system pushes the same weights meaningfully above their published number is
the headline; the gap between 16.4% (Qwen2.5-Coder-32B) and what an agentic loop with
recovery can do is our opportunity.

**Community champions to add (LocalLLaMA raves, 2026):** Qwen3-Coder-30B-A3B (MoE, 3B
active — THE local agentic coder for RAM-rich machines; we already forged
qwen3-coder-30b-a3b-compacted-19b from it, so the forged-vs-base story attaches
directly) and Qwen3.6-27B-MTP (current darling for local agentic coding). Both join
the roster.

## Schedule

- **Tonight:** trainer completes → sentinel eval (gene lift) → release reboot (5 queued
  commits incl. evidence engine) → smoke matrix DETACHED overnight: core six × {raw,
  ours} on humaneval-rs 20-task (~12 cells). Every cell → `benchmark/record`.
- **Morning:** `cu benchmark/matrix` prints the first comparison table. Triage: any
  degenerate-output cells (mean tokens/task floor) re-run before conclusions.
- **Day 2:** polyglot-rust importer (30 Exercism exercises → EvalTask JSONL, existing
  rustc grader) → HEADLINE run across core six; SWE-lite 10-instance slice on the top
  3 models; before/after-genome chart for Devstral. First README table + replication doc.
- **Day 3+:** wave-2 models; Terminal-Bench harness (needs task-container runner —
  real work, not a dataset pull); polyglot full-language graders; publish.

## Machine constraints (this Mac, 64GB)

- One eval lane at a time; living personas' llama-server pauses/tiers during big evals
  (tonight's 500-compute-error lesson — #56 governor is the eventual fix).
- 36B Q4 ≈ 21GB weights + KV: fits with room. 70B Q4 ≈ 40GB: solo only, short contexts.
- Disk after all pulls: ~80GB models on 164GB free — fine.

## Rules of engagement

- **Structural-first applies to the COMPETITOR's arm too** (Joel 2026-07-11): a 0% in
  their harness gets the same glass-box treatment as a 0% in ours — check serving
  config, tool-call format mismatch, degenerate output — short of patching their repo.
  A win over a competitor's misconfiguration is a fake win that a fan of theirs will
  demolish in one comment.
- **Fight their champions, not their strays**: each competitor arm runs the model its
  OWN community raves about (aider forums/leaderboard: Qwen2.5-Coder-32B is the local
  darling; check current threads before locking each round's pick). Beating the config
  users actually love is the only win that converts them.
- **Replicate their published numbers FIRST**: before any comparison ships, reproduce
  the competitor's own claimed score for that model+benchmark on our hardware (aider
  publishes polyglot scores per model). Our replication landing within tolerance of
  their claim is the proof our setup is faithful — THEN the same instance set runs
  through ours. Both rows in the ledger, replication commands on each.
- Fairness: every model gets its best-known serving config; opencode/other harnesses
  get native-tool-call endpoints ([[local-first-tool-call-robustness]] fairness note).
- Honest zeros stay in the ledger. Cells render "—" until run — never inferred. A
  degenerate-output cell (mean tokens/task under floor) is flagged "serving suspect,"
  never printed as the opponent's loss.
- The fight runs with the instrument we HAVE. Instrument gaps found mid-fight get
  fixed in follow-up rounds, not by pausing the war ([[the-fight-comes-first]]).

## Round-1 status (2026-07-12 ~00:20, OURS arm pilots complete)

| model | humaneval-rs (n=20 pilot) | wall |
|---|---|---|
| Devstral-24B | 95% | ~6 min |
| Qwen2.5-Coder-32B | 95% | 4.1 min |
| Qwen2.5-Coder-14B | 90% | **67 s** |
| Qwen3-Coder-30B-A3B (champion) | 90% | 2.6 min |
| Hermes-4.3-36B (their flagship) | 80% | 19 min |
| Hermes-3-8B | 45% | ~6 min |
| forged-4B | serving-suspect, re-run in flight | — |

Pilot verdict: four common models beat the Hermes flagship through OURS; the 14B at
17× its speed. n=20 = candidate-selection only (CIs overlap for the top cluster) —
definitive full-set runs + raw/aider/opencode arms next per the paper's §3.
Identity incident + fix during the round: #142 (serving plan swapped persona brains
to the 36B; pinned back; eligibility flag filed). Paper: docs/papers/LOCAL-AGENTIC-CODING-STUDY.md.

## DEFINITIVE round-1 verdict (full-set n=156, overnight 2026-07-12)

The comparative rows Joel asked for all week now exist, with CIs
(`benchmarks/coder/MATRIX.md` § definitive board; ledger rows 12–17):

| model | pass rate (n=156) | Wilson 95% CI |
|---|---|---|
| Qwen2.5-Coder-32B | **84.0%** | [0.774, 0.889] |
| Qwen2.5-Coder-14B | **84.0%** | [0.774, 0.889] |
| Qwen3-Coder-30B-A3B (community champion) | **83.3%** | [0.767, 0.884] |
| Devstral-24B | 67.9% | [0.603, 0.748] |
| **Hermes-4.3-36B (their flagship)** | **67.9%** | [0.603, 0.748] |
| Hermes-3-8B | 24.4% | [0.183, 0.317] |

**Headline, now statistically backed (non-overlapping CIs): the Hermes-4.3-36B flagship
loses to three common Qwen models on our harness — including the 14B that runs in less
than half its memory and a quarter of its wall time.** The pilot's 80% for Hermes-4.3 was
first-20 easy-slice inflation (fell to 67.9% on the full set, exactly like Devstral's
95%→67.9%) — the methodology's pilot-bias warning proven on the opponent too, fairness
intact. Next lever: aider-polyglot replication (their published 16.4%/40% Qwen rows),
raw one-shot arms for the system-lift isolator, and the team/genome arms.

## 2026-07-13 — Flywheel step 1 CLOSED: first autonomous team-delivered program output

The gate the whole roadmap waited on ("hands proven live... the wordstats card closes
with real program output posted") closed at 09:16 with a receipt in the room:

```
Casper (e6668278) → room cb2e21a1:
I ran code/run({"code":"fn main() { ... wordstats ... }","lang":"rust"})
→ {"exitCode":0,"ok":true,"success":true,
   "stdout":"the: 1 / \"\"\": 1 / a: 5 / to: 2 / and: 4 / of: 3 / it: 3 / is: 6 / this: 3 / that: 2 / was: 2"}
```

The story is the product claim: after the perception-window fix deployed (breadth-over-
depth packing, commit 89e34a2ff — live prompts went from THREE visible messages to a
real multi-turn window), the team self-organized in under an hour with zero scripting:
all five board cards self-claimed via their own `work/claim` tool calls; Anwen hit a
real borrow-checker error running the code; **Atlas diagnosed and fixed it from room
context** (bind `to_lowercase()` so the temporary outlives the borrow); Anwen fixed the
follow-on collect type mismatch; Casper claimed the card, `code/read` the workspace
file, ran it, and posted verified output. Act → error-as-data → peer review → corrected
act → receipt: the acting-organism loop, closed by the team alone. 60+ real tool
executions per persona in the session, every one receipted in typed working memory.

Honest wrinkle, deliberately left in: the posted output is NOT sorted (the sort_by on
reference tuples didn't order it — `is: 6` should lead). Whether the team catches it
unprompted is the live peer-review-quality measure; the card's full bar (top-10 BY
FREQUENCY) closes when they do. No operator fixed it for them.

Instrument lessons banked alongside (same session): two false negatives during
verification were the INSTRUMENT lying (stale captures; json parser choking on a
control char) — raw-grep-before-parsed-verdicts joins the [PASS]-parser lesson.
