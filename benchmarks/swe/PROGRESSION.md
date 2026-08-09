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
| D (24066-n9, 22840-n12) | 4568 | 24066, 22840 | 0/3 + 0/3 | Atlas: att1 0B/tree-intact, att2+att3 BOTH 1146B f2p 0/1 p2p 26/30 — resubmitted a same-size same-verdict patch DESPITE the regression-led excerpt naming her 4 broken tests (byte-identity unverifiable: patch payloads not persisted → #379). **LEARNING-STUCK on the repair axis** — the #2204 lever informed but did not teach. Benchy: best run on record — 76 gens, 22 edit acts, patches 2719B→11445B→10273B, p2p 39/40 held across ALL attempts (one consistent casualty), f2p 0/2 never flipped. Substrate flawless: lane_acquire <1ms, zero parks, lanes 2-serving all night | Atlas 2 (repeat), **Benchy 3** | Atlas: verdict-as-text doesn't reach the next attempt's behavior; Benchy: sustained near-target edits that never cross the fix line | next: verdict must land as STATE not prose (perception fact / forced diff re-read: "your patch is identical to the one that failed") + patch-sha receipts (#379) |

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

**Reading as of round D grade (2026-08-08):** the two citizens have
DIVERGED, which is itself signal. Benchy is monotone — rung 1 → 2 → 3 across
her three graded rounds (0-byte → docstring → sustained near-target edits
holding 39/40 p2p). Atlas hit the ladder's first genuine learning-stuck: the
regression-led excerpt (text in her next attempt's prompt) did not change
her behavior — first confirmed instance of "the lever informed but did not
teach." Per the two-axis rule that is a COGNITION finding, not substrate:
information delivered as prose lost to whatever momentum drives resubmission.
The next lever must land as STATE (a perception fact — "this patch is
byte-identical to the one that just failed 26/30" — or a forced re-read of
her own diff), which requires patch-sha receipts first (#379). Substrate axis
is fully green: bounded acquisition proved out end-to-end, zero parks.
