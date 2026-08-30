# Priority Receipts: architectures now appearing in the literature, in this repo first

Two papers in the last week of August 2026 independently converged on core
pieces of this repo's public, timestamped architecture. Convergence is the
point: the ideas were reachable, and this history reached them first —
running, not proposed. Every date below is a commit on
github.com/CambrianTech/continuum.

---

## Exhibit A: WikiSkill (Google Research)

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

---

## Exhibit B: Meta^n (UMinn + Seoul National)

**Meta^n** (arXiv:2608.24735, **2026-08-25**): a frozen improvement operator Ω
recursing over the solver stack's execution traces plus the code that produced
them, emitting each new layer as "a strategic pre-process and a library of
callable helpers," with depth set by convergence. Their headline ablation:
most of the gain comes from **the conditioning each layer passes to the next**.

| Meta^n concept | This repo | Where | Date |
|---|---|---|---|
| Frozen improver reading traces (Ω never edits itself) | fixed curriculum machinery consuming the experience stream (L1 lift, `genome/teach`) — cognition never edits the improver | genome/curriculum arc | 2026-05→08 |
| Layer = "strategic pre-process + library of callable helpers" | recipes (data pipelines) + commands (discoverable capabilities) — the kernel split | UNIVERSAL-PRIMITIVES, RECIPE-EXECUTION-RUNTIME | 2025→2026 |
| Conditioning-passed-forward as the active ingredient | retry attempts conditioned on the grader's verdict + files-examined trail; room transcripts carrying 💭 thoughts + ⚙ acts to teammates | benchmark attempt loop; society stack PRs #2571–#2576 | 2026-08 |
| Reading traces of the stack below | recorder + turn replay + prompt captures + probe receipts, replay-first-class | OBSERVABILITY-AS-SUBSTRATE | 2026-06 |

**Past the paper:** their layers are context wrapped around ONE solver; here
the "layers" are colleagues — improvements compile into weight-space genes,
distribute across a team of diverse persistent minds, and the conditioning is
a shared room transcript both kinds (humans and citizens) read through the
same pipe. **Worth adopting from them:** depth-by-convergence (recurse while
the verdict curve improves — cleaner than a fixed attempt count) and the
explicit evolutionary archive over improvement chains.

---

## The honest frame

One unaffiliated engineer, two years, consumer hardware, public history. The
ideas weren't secret — they were in the open the whole time, running. These papers
are welcome: it means the architecture is now legible to people who needed an
author list to look.
