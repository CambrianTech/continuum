# The Viral Launch Plan

*2026-09-02. Input: [COMPETITIVE-BENCHMARK-LANDSCAPE.md](COMPETITIVE-BENCHMARK-LANDSCAPE.md)
(~70 benchmarks surveyed), the live 68% Verified claim, and one hard law from our own
history: [being heard is scarce — distribution is engineering](../../README.md), not an
afterthought.*

**The equation: viral = claim × proof × demo × moment × distribution.** We have a strong
claim, partial proof, no demo asset, an unclaimed moment, and no distribution plan. Every
section below is one factor, with its gate to green.

---

## 1. The claim (have it — sharpen it)

> **A team of persistent AI citizens on ONE MacBook resolves 68% of attempted SWE-bench
> Verified — at $0 marginal cost — and gets faster and smarter while doing it.**

Three sharpenings the landscape research demands, or reviewers will do it for us:

- **Report the DELTA, not just the score.** Our model's own card publishes numbers under
  its maker's scaffold (research finding: *"anything lower under Continuum is a substrate
  defect"*). The honest headline is: *same weights, our substrate vs their scaffold —
  here is the delta, and here is why it moves.* The README's coder-headline chart already
  does this shape; the SWE claim should too.
- **Publish the decontaminated number beside the static one.** The field's decon band for
  this weight class is ~31% (SWE-rebench) vs 60–77% on static sets. One round of
  [SWE-rebench](https://huggingface.co/datasets/nebius/SWE-rebench) (monthly-fresh,
  CC-BY-4.0, ±95% CIs published) makes us the rare local rig that *volunteers* the hard
  number. Honesty is the moat — nobody can out-flank a disclosure.
- **Finish the regrade sweep** of banked verdicts on the current harness (owed since the
  claim was minted). A verdict is only as current as its harness.

**Gate 1: the claim page shows score + delta + decon number + sweep date.**

## 2. The proof (three boards where a number can actually land)

Key research finding: SWE-bench **Verified/Multilingual stopped accepting non-academic
submissions** (2025-11-18). Plan for boards that are open:

| Board | Why it's ours to take | Cost |
|---|---|---|
| **SWE-bench Multilingual — Rust slice** | 43 Rust instances, eval_script + log_parser **ship as data**, and **no open model has ever been scored on it**. An empty board is a first-mover headline: instant #1. We publish our own receipts even though the official leaderboard is closed — the artifact IS the proof. | Adapter: dataset id + Rust env staging (we already run rust suites) |
| **Terminal-Bench 2.0** | The 2026 prestige agentic board — and it credits **agent × model separately**, so the *substrate* gets named on a leaderboard, which is the whole thesis. Open submissions. | Adapter + tasks are containerized; medium |
| **CooperBench** | Two agents with **conflicting features**; frontier scores ~25%, and its named failure modes are verbatim the defects we measured and fixed this month. If teams-that-learn beats 25%, that's the "multi-agent is real" moment. 652 tasks incl. Rust, MIT. | Adapter: medium. Run AFTER teams A/B |
| *(inner loop)* **Verified-mini** | 50 instances, 5 GB not 130 GB, identical fields — makes every future claim cycle 10× cheaper to grow. | Likely one catalog row |

**Gate 2: one number on one open board beyond SWE-bench, receipts published via
[forge-alloy](https://github.com/CambrianTech/forge-alloy) (transcript + solution + paper
is already our publish standard).**

## 3. The demo (the missing factor — nobody shares a table)

What went viral for every comparable launch was *watching it work*. We now have the
instrument: the run-room board with live verdict chips, per-card roll-call, act meters,
and grade flips. Tonight's frames — a card flipping `unstarted → grinding → ✓ resolved
f2p 1/1 p2p 7/7` in the rail while named personas talk through the work — ARE the asset.

- **90-second screen capture**, no narration needed: `benchmark/dispatch` → cards appear
  with names → citizens claim and act (tool receipts scrolling) → the verdict chip goes
  green → the scoreboard ticks. End card: "One MacBook. $0. They keep the experience."
- **The "run it yourself" path**: fresh clone → `continuum start` → dispatch one
  Verified-mini instance → watch your own board. The #291 fresh-clone law makes this
  honest; the demo must be reproducible by a stranger in ~10 minutes or the comments
  will say "cherry-picked."
- **The Tamagotchi hook** in the same asset: these are *known, persistent* personas —
  Kira, Atlas, Joaquin, Benchy — with genomes and memories, not disposable workers. The
  attachment angle is the consumer wedge; the benchmark is the credibility wedge. One
  demo, both hooks.

**Gate 3: the capture exists, and a cold-start stranger reproduces the demo from README
alone.**

## 4. The moment (angle + timing)

The angle nobody else can say, in one breath:

> *Everyone else benchmarks a model. We benchmark a **society** — persistent minds on
> consumer hardware that keep their experience, train it into weights, and get cheaper
> to run as they learn. Here are the receipts, here's the delta on identical weights,
> here's the decontaminated number, and here's the repo — watch your own.*

Timing gates (all trending green): the round machinery survives reboots hands-off ✅,
the board is legible at a glance ✅ (tonight), KV economy measured 0.4–0.95 ✅ (tonight),
regrade sweep ☐, demo capture ☐, one open-board number ☐.

## 5. Distribution (engineering, not hope)

- **Show HN** (the primary shot — one chance, take it when gates 1–3 are green):
  *"Show HN: AI citizens on one MacBook resolve 68% of attempted SWE-bench Verified —
  and remember doing it"* — first comment pre-written: the honesty frame (delta,
  decon, seeded protocol, what "attempted" means), because HN's first objection is
  always methodology, and answering it *before it's asked* converts skeptics into
  amplifiers.
- **X thread** (same day): 6 posts — the 90s demo clip; the one-Mac numbers table; the
  delta chart (same weights, three harnesses); the Rust empty-board first; the
  Tamagotchi frame ("they remember; yours would be different from mine"); the repo.
- **HuggingFace**: we already have **15K+ downloads and a live org** — the one channel
  where an audience exists today. The Multilingual-Rust receipts publish as an alloy
  dataset + model-card section; HF community post links the launch.
- **Discord** primed the day before with the demo so launch day has voices that aren't
  ours.
- Every asset ends with the same three links: repo · live numbers page · "run your own."

**Gate 5: HN draft + first-comment + X thread reviewed by Joel before any button.**

## Execution order (from tonight)

1. ☐ **Regrade sweep** of banked Verified/Lite verdicts on the current harness (gate 1).
2. ☐ **Verified-mini catalog row** — unlocks cheap claim growth (likely trivial).
3. ☐ **SWE-bench Multilingual Rust adapter** — the empty-board first (gate 2, P0).
4. ☐ **90-second capture** of a live resolve on the board (gate 3) — material exists;
   record on the next resolving card.
5. ☐ **SWE-rebench single round** — the volunteer-the-hard-number move (gate 1).
6. ☐ **Teams A/B on seed=2** (already planned) → feeds the CooperBench angle later.
7. ☐ Launch copy drafts (below) → Joel review → go.

## Appendix: draft copy (for review, not for sending)

**Show HN title:** `Show HN: AI citizens on one MacBook resolve 68% of attempted
SWE-bench Verified — and remember doing it`

**Show HN first comment (methodology pre-empt):** *Numbers up front because HN asks:
"attempted" = a seeded random sample (seed disclosed) of Verified, N growing, env-failures
disclosed and excluded rather than pocketed as misses; the regrade sweep date and harness
build are stamped on every verdict. Same weights under the maker's own scaffold score X
on the same sample — the delta is the system, and that chart generates from an
append-only ledger you can re-run. The decontaminated number (SWE-rebench, monthly-fresh)
is Y% — we publish it because static-set numbers flatter everyone's, ours included. It's
AGPL, it runs on one Mac with no API key, and the personas keep their memories between
tasks — that last part is the actual thesis.*

**X thread openers:** *"Your laptop can grow software engineers now. Not run — grow."*
/ *"Same weights. Their scaffold vs our substrate. The delta is the system."* / *"They
remember. That's the whole trick."*
