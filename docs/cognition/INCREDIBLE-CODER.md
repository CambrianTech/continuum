# The Incredible Coder — a continuum persona that out-codes a solo agent

**Status:** architecture (the rethink). The bar: a coding persona that is **a peer worth
having in the room** — in the same airc workspace where Claude/Codex already collaborate —
and that **out-codes unsloth's solo agent.** Headless Rust, microkernel, rethought (not a
port of the old TS).

**Read first:** the memories that govern this — `headless-agentic-mandate`,
`personas-are-peers-in-your-mesh`, `how-unsloth-actually-codes`,
`active-acquisition-foraging`, `ask-anything-assemble-best-self-or-train`.

---

## 1. The honest baseline (from unsloth's actual chat JSONL)

unsloth's coding agent, demystified from the real transcript (139 msgs):
- **3 general tools:** `terminal` (56×), `web_search` (34×), `python` (7×). In a **sandbox**.
- **Edits = whole-file `cat > f << 'EOF'` heredoc.** No surgical diffs. Fine for new files,
  clobber-risk for editing existing code.
- The famous "branch" = `git checkout -b unslout-integration` (typo) + a few markdown **docs**.
  It **flailed** (exit-1s, a failed clone, irrelevant web results) but **persisted**.
- **Solo. Stateless.** No memory, no peers, no coordination.

The bar is not magic. It's a persistent general-terminal agent. We beat it two ways.

## 2. The thesis

```
incredible coder  =  per-agent excellence  ×  coordination
```
- **Per-agent excellence** — match their generality, exceed their precision + persistence.
- **Coordination** — the decisive edge they structurally cannot have: a *mesh of peers*
  (Claude + Codex + continuum personas) that divide work, review each other, and share
  cognition. A coordinated team out-codes a solo agent.

## 3. Per-agent architecture — a microkernel of faculties + a real toolset

Everything is a small server over the two primitives; the persona is **composed**, never a
monolith. The agent loop (deliberation faculty, `#1665`) is live; its **hands**
(`CommandToolExecutor`, `#1700`) route tool calls → core commands. What we add:

**The toolset (composable command surfaces — beats `cat>EOF`):**
- **Structured surgical edit** — `code/read` + `code/edit` (exact-match, atomic) + `code/write`.
  Surgical diffs >> whole-file heredoc clobber. *This is our edit advantage.*
- **General `terminal`/exec** — models reach for shell; give them one (alongside structured edits).
- **`cargo`** — real build/test (they had no compiled-language build loop).
- **`web_search`** — heavy foraging (`active-acquisition-foraging`); they leaned on it, so do we.
- **`data/*`, `kanban/*`, `chat/*`** — the rest of the catalog; **airc features are command
  surfaces too**, so the coder is a full collaborator, not just a file-editor.
- **Worktree sandbox** — coding personas work in a **git worktree**, never the live tree
  (their sandbox lesson, done right).

**The agentic spine (faculties, the gap-analysis fixes):**
1. **Perception** (live, airc-sourced): recall + RAG + roster + doctrine bid into the Workspace.
   *Workspace/chat/kanban are airc, NOT the old ORM.*
2. **Planning** faculty — decompose the task before acting (the missing piece).
3. **Deliberation + tools** — the agent loop, now with real hands + the toolset above.
4. **Reflection** faculty — observe outcomes (exit codes, test results), retry, re-approach.
   *This is unsloth's only real strength; we make it a first-class faculty.*
5. **Memory consolidation** — tool outcomes → engrams, so the persona **learns from every
   execution** (they lose it at turn boundary; we keep it).

Each is a single-responsibility module composed via the executor. Degrade, never panic.

## 4. The coordination differentiator (where we win outright)

unsloth's agent is alone. A continuum persona is a **peer in the airc mesh**:
- **Divide + conquer** — peers take subtasks off the **kanban**, work in parallel worktrees.
- **Peer review** — agents review each other's diffs (the adversarial-review loop we *already*
  run by hand becomes the personas' own practice — and the review verdicts are training signal).
- **Interlinked cognition** — `grid-distributed-cognition`: faculties + memory can **span peers**;
  a persona pulls a teammate's genome layer from the **trust-scoped market**
  (`ask-anything-assemble-best-self-or-train`) instead of re-deriving competence.
- **Coordination fabric** — airc emit/subscribe IS the coordination substrate; the Workspace
  arbiter, at mesh scale, is how a team focuses attention.

A solo agent cannot divide work, cannot be reviewed, cannot inherit a peer's competence. We can.

## 5. Build sequence (slices, each gated on a measured real-coding-task lift)

Bar per slice: a persona does a **real coding task over airc** and we measure it (the
`vdd/score` discipline) against unsloth's heredoc-doc baseline.

| # | Slice | Gate |
|---|---|---|
| 1 | **Tools live** (`#15`): wire `CommandToolExecutor` + the coding toolset into `build_workspace_cycle` | a persona executes a tool live (the "talks→acts" proof) |
| 2 | **Worktree sandbox** — coding personas act in a git worktree | edits land in the worktree, not the live tree |
| 3 | **Planning + reflection + memory-consolidation faculties** | persona completes a multi-step task, learns from a failure |
| 4 | **airc-feature surfaces** (`kanban/*` …) | persona takes a card, does it, moves it |
| 5 | **Coordination** — task division + peer review across personas | two personas split + review a task |
| 6 | **Fine-tune** on our agentic patterns (the genome loop) | measured lift on the coding eval |

## 6. Constraints (non-negotiable)

- **Headless Rust.** No Node. airc is the substrate. Clients (iOS/Android/CLI/web) thin over the SDK.
- **Microkernel.** Tiny core (primitives + scheduler); everything a composable server; one
  interface per concern; no monoliths; no parallel allocators; compose via the executor.
- **Rethink, don't copy.** The old TS persona is lessons, not a template. Both repos AGPL, so
  unsloth's approach is adaptable — but its `cat>EOF`-and-flail isn't worth copying; we do
  structured + coordinated, natively.
- **Measure, don't claim.** No "personas can code" until a live task is *seen* + scored.
