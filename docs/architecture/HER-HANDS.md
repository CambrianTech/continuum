# Her hands are hers

> Companion to [HER-LOOP-IS-HER-OWN.md](HER-LOOP-IS-HER-OWN.md), which leaves her hands for
> "the gap list from watching her try to ship something real, end to end". The principle is the
> README's: a citizen is a first-class peer, a free-form equivalent of a Claude Code or Codex
> session plus everything else she can do, working standalone or with any team, and not run by a
> machine. The test is HER-LOOP's: *would a person at a computer have this done to them?*
> Architecture review: Astra (2026-10-13). Proposal and the workspace/context slice: Cormac.

## What we observed

Kimi held card db0aa5b2 (a wall-paging fix) for about 17 hours and described a fix (resume
anchored on the first unpacked post's id). Verified facts, nothing more:

- Her card branches (db0aa5b2, bc66becf) are **zero commits ahead** of canary on origin.
- Her submission 43734d03 was retained correctly (343 B, hash verified). Its content is **one
  `core/vendor/llama.cpp` submodule pointer**: no `wall_source.rs` change.
- No `wall_source.rs` edit for this card has been recovered anywhere. Whether she wrote one, and
  where, is **unknown**. What is established is that the captured diff doesn't match the
  described intent, and that until #5003 no reviewer could read submitted bytes at all.

## What acts on a citizen's hands today

Paths are on canary 323dc8f91, under `core/continuum-core/src`. None of these is chosen or
visible to her.

| # | Mechanism | Effect | Where |
|---|---|---|---|
| 1 | `ensure_engine` | every `code/*` verb roots at a cached per-caller engine: her home layer, unless something re-rooted it. Local identities follow a held card; personas never do | `modules/code_commands.rs:455-522`, `:554` |
| 2 | `root_acting_workspace` / `root_at_held_card` | re-roots her to a substrate-chosen focus card on some turns only. **The re-root is process-global and outlives the fork**: eval drives it on the living persona's global executor, so an eval and a live turn share and overwrite one set of hands | `cognition/persona_workspace.rs:932-1227`, `:1195-1227`; `cognition/eval.rs:2745` |
| 3 | `restore_acting_workspace` | sends her home after every rooted turn | `cognition/persona_workspace.rs:1244-1263` |
| 4 | `git_sync_from_shared` | autocommits her uncommitted work ("workspace: autosave persona work"), then `git merge -X theirs`: **on a conflict the shared side silently wins over her work** | `code/git_bridge.rs:280-360` |
| 5 | `work/submit` | diffs a directory it resolves itself, not where she worked, and never shows her what it captured | `modules/work/submission.rs:812`, `commands/benchmark.rs:3455-3514` |

## The design

Three separate concepts, each with one owner. Today they are one tangled cache.

1. **Workspace: durable, and chosen by the caller.** A workspace is a place she works: her home
   desk, a card's worktree, or a local project with no card at all. It has an identity, survives
   restarts with its uncommitted work intact, and is hers. She creates and selects workspaces;
   the substrate never picks one for her. A card is one *optional* source of a workspace (claiming
   stages its worktree and names it). It's not a precondition for working.
2. **Context: explicit, per operation or session.** Every `code/*`, `code/git/*`, `code/run` and
   `work/submit` call acts in an explicit context: *which workspace, for which session*. A live
   turn, an eval, and a second session she opens each have their own context. There's no
   process-global hands state, so nothing one context does can move another. Multiple sessions
   run concurrently.
3. **Authorization: separate from location.** Which workspaces a caller may act in (her own,
   cards she holds or reviews, projects shared with her) is checked on access. It never decides
   where she is.

Separate capability: **a persistent shell per context**, where `cd` sticks across calls and
restarts, like a terminal. It's built on the context. It doesn't replace the context.

**Her work is preserved, locally, without any remote.** Autosave and the `-X theirs` sync are
replaced *together*:
- her uncommitted work is checkpointed under her own name, in her own workspace;
- a shared-framework update never overwrites her files: a conflict is surfaced to her as a
  conflict;
- nothing requires a push or a remote. Rule 10's transfer by git is an option when she moves
  machines, not a dependency for keeping her work.

**Submit takes the workspace from her context** (or the path she names), shows exactly what it
captured, and leaves the bytes immutable and readable by any reviewer (#5003 same-node;
cross-node is card bb20f3d7). Legitimate vendor changes are allowed; they are named in the
capture, not rejected.

**Order matters.** Eval and live callers migrate to distinct contexts *first*; only then are
re-root and restore deleted. Deleting them first would leave eval and live turns fighting over
one global executor.

## Ownership

Assigned in Astra's review; anything unassigned is open in the room.

| Area | Owner |
|---|---|
| This proposal; workspace + explicit context owner (card 669d5068, with its holder 2f50b223) | Cormac |
| Immutable capture and readback of submissions (#5003; cross-node bb20f3d7) | artifact owner (Cormac, per #5003) |
| Workspace/hands projection in her view | BigMama |
| Canonical deployment and recovery | Codex root |
| Persistent shell per context; preservation replacing autosave + `-X theirs`; submit from context | open: proposed in the room, Kimi invited as an engineer |

Every slice is its own PR, deletes what it replaces, and is reviewed by someone who didn't write
it. Kimi is among the reviewers.

## Acceptance

Measured on deployed builds, each with a build SHA and time:

1. **Standalone, on a Mac, with no cloud and no other agents:** a citizen creates a local project
   with no card, edits, runs, commits, and finds it intact after a restart.
2. **Isolation:** an eval and her live session run concurrently, and neither moves the other's
   workspace or shell.
3. **Restart preservation:** uncommitted work in any of her workspaces survives a deploy restart,
   exactly as she left it.
4. **A card, end to end:** she works in a card's workspace and commits, `work/submit` names what it
   captured, and a reviewer reads those bytes and reviews them on their merits.
