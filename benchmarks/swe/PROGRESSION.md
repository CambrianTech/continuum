# SWE benchmark progression ledger — is their effort IMPROVING or STUCK?

**The question this file answers** (Joel, 2026-08-08: *"be careful to make sure
their effort through the benchmarks keeps improving and doesn't get stuck"*):
round over round, is the citizens' work moving up the effort ladder, or
repeating a failure signature? A resolve is the summit; the LADDER is the
metric — and a round whose failure shape matches the previous round's means
the lever between them didn't teach, which is its own finding.

**The effort ladder** (each rung a measurable behavior, not a score):
0. no valid environment (void — says nothing about the model)
1. reads/talks, no edits (0-byte patches)
2. lands real edits — wrong target or breaks the tree
3. lands on-target edits that break nothing but don't fix
4. repairs own collateral after the verdict names it
5. fixes the target test (RESOLVED)
6. resolves repeatably across instances

**Rules:** one row per round; infra-void rounds are recorded but EXCLUDED from
the capability read ([[a-zero-is-a-harness-claim]]); every row cites receipts
(probe stream + captures + patches). Append-only, updated when a round grades.

| Round | Build | Instances | Verdicts | Effort evidence | Rung | Bound by | Lever shipped after |
|---|---|---|---|---|---|---|---|
| pre-gate (n1–n8 era) | ≤4548 | 22840, 24066 | all 0 | VOID — era venv imported the grader's tree; her edits invisible to her own tests | 0 | anti-verification env | PYTHONPATH fix (#2194) + gold-through-her-hands gate |
| A (22840-n9, 24066-n6) | 4557 | 22840, 24066 | 0/3 + 0/3, all patches 0B, p2p intact | Benchy: right file, edit failed NOT-FOUND, cut off mid-recovery at 13 gens; Atlas: 2× one-read + generic essay settles | 1 | 12-act cap + empty-diff Speak settles | acts 12→32 + empty-diff re-drive (#2196) |
| B (24066-n7, 22840-n10) | 4566 | 24066, 22840 | 0/3 + 0/3, EVERY attempt a real patch | Atlas: 2003B in the EXACT gold function, right concept, wrong symbol broke 30 p2p, resubmitted identical ×3 (breakage hidden from her). Benchy: clean 851B — in a docstring | **2** | retry verdict hid p2p regression | regression-led excerpt (#2204) |
| C (24066-n8, 22840-n11) | 4566 | 24066, 22840 | VOID — 2h13m parked in lane acquisition, 0 generations | excluded from capability read | — | bounded+probed acquisition (#2207) + ready-gated launch |
| D (24066-n9, 22840-n12) | 4568 | 24066, 22840 | RUNNING | gens flowing within minutes of gated launch; first edit act by gen 6 | — | — | — |

**Stuck-detection, two axes (both instrumented as of 2026-08-08):**
- **Substrate-stuck:** zero generations while attempt is live → pulse board
  rows say "queued/no generations yet"; lane acquisition is bounded at 15 min
  with `benchmark.solve.phase` naming the parked step. A silent multi-hour
  park is now structurally impossible.
- **Learning-stuck:** the round's failure signature equals the previous
  round's *despite* the lever shipped between them (e.g. if round D's Atlas
  again resubmits an identical broken patch, the regression-led verdict
  failed to teach — that's a cognition finding, not a substrate one). Judged
  per row against the "Bound by" of the prior row.

**Reading as of round D launch:** monotone so far — rung 0 → 1 → 2 across
graded rounds, with each rung's binding constraint identified from receipts
and a lever shipped before the next round. Round D tests: does the regression
verdict convert Atlas's rung-2 collateral into rung-4 repair, and does
Benchy's target selection reach rung 3?
