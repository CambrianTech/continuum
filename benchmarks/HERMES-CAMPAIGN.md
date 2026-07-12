# The Hermes Campaign — order of battle

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

## Rules of engagement

- Fairness: every model gets its best-known serving config; opencode/other harnesses
  get native-tool-call endpoints ([[local-first-tool-call-robustness]] fairness note).
- Honest zeros stay in the ledger. Cells render "—" until run — never inferred.
- The fight runs with the instrument we HAVE. Instrument gaps found mid-fight get
  fixed in follow-up rounds, not by pausing the war
  ([[the-fight-comes-first]]).
