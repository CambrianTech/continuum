# Priority Receipts: the WikiSkill architecture, in this repo first

Google Research published **WikiSkill** (arXiv:2608.27454, **2026-08-27**): agent
workspaces split into immutable execution traces, an active skill layer, and a
persistent knowledge wiki between them — with the finding that recording
accept/reject history where the optimizer can read it lifts accuracy ~12 points.

This is convergent validation, not copying — and convergence is the point: the
architecture was independently reachable, and this repo's public, timestamped
history reached it first. Every date below is a commit on
github.com/CambrianTech/continuum.

| Layer (WikiSkill terms) | This repo | First commit | Date |
|---|---|---|---|
| Consolidation from experience ("skill evolution") | *"why not let the ais dream with us?"* | `0cd68dfd3` | **2025-06-18** |
| Curriculum architecture (the skill layer's teacher) | Continuum Academy v0.2.0 | `a8c134ce0` | 2025-06-03 |
| Multi-agent skill recipes | multi-persona + academy training recipes | `64ca72ca8` | 2025-10-10 |
| The persistent middle layer ("wiki") | typed Engram + admission membrane | `f6c25bfa0` | 2026-05-13 |
| Belief invalidation (what a wiki can't do) | *"explicitly refute the stale belief"* | `ec0423d3e` | 2026-07-13 |
| Their named untested boundary: skill SELECTION | gene routing by embedding distance | (genome arc, Aug 2026) | 2026-08-22 |

## Where this repo is past the paper

1. **Skills as weights, not prompt text.** WikiSkill's skill file is context the
   model re-reads (and can lose in a long window). Here skills compile into LoRA
   genome layers — knowledge the window cannot evict.
2. **Selection is built and learned.** The paper's own limitations section:
   skills are handed to the agent; selection is never part of the test. Gene
   routing by embedding distance IS learned selection, in tree.
3. **Invalidation edges.** A wiki accumulates; a causal memory graph can
   *un-know*: beliefs carry provenance, review refutes them, and dependents are
   reachable. (The citation graph's missing retraction propagation, fixed at
   mind-scale.)
4. **The rejection ledger** (their strongest finding) maps onto existing seams —
   forge gate, belief review, consolidation admit — as one engram per decision:
   issue #2565.

## The honest frame

One unaffiliated engineer, two years, consumer hardware, public history. The
ideas weren't secret — they were in the open the whole time, running. The paper
is welcome: it means the architecture is now legible to people who needed an
author list to look.
