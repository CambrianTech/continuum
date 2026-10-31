# Her hands are hers

> How each call answers her (results, handles, long-running tasks, reflexes, sentinels) is
> [HER-TOOLS.md](HER-TOOLS.md); this document is where she works.
>
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
  described intent, and that until #5003 there was no supported way for a reviewer to retrieve a
  submission's bytes.

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

Separate capability: **a persistent shell per context**, where `cd` sticks across calls, like a
terminal. It's built on the context. It doesn't replace the context. What a restart preserves
has to be stated honestly:
- **Restored:** the context's workspace, its current directory, and its declared configuration (the
  environment variables she set as config). A shell after a restart opens where she left it.
- **Not restored:** live processes, and the in-memory state of a process environment (a running
  server, a shell's ad-hoc exports). A restart ends them, and her view says which ones ended, so
  she can start them again. Nothing pretends they survived.

**Her work is preserved, locally, without any remote.** Autosave and the `-X theirs` sync are
replaced *together*:
- her uncommitted work is checkpointed under her own name, in her own workspace;
- a shared-framework update never overwrites her files: a conflict is surfaced to her as a
  conflict;
- nothing requires a push or a remote. Rule 10's transfer by git is an option when she moves
  machines, not a dependency for keeping her work;
- **a Continuum update never fetches into or merges into her projects**, not even
  conflict-free. Her workspaces change only by her hand. If she wants a newer base, she pulls
  it herself, as any engineer would.

**Submit takes the workspace from her context** (or the path she names), shows exactly what it
captured, and leaves the bytes immutable and readable by any reviewer (#5003 same-node;
cross-node is card bb20f3d7). Legitimate vendor changes are allowed; they are named in the
capture, not rejected.

**Order matters.** Eval and live callers migrate to distinct contexts *first*; only then are
re-root and restore deleted. Deleting them first would leave eval and live turns fighting over
one global executor.

## Where the context goes (groundwork, read on canary 323dc8f91)

- **What eval and live share is identity, not an executor.** The one `CommandExecutor` holds no
  hands state. Every persona tool call carries `CallerIdentity::local_persona(pid)`
  (`cognition/tool_executor/command_executor.rs:93-111`). An eval fork clones the same tool
  executor and the same identity (`cognition/persona_workspace.rs:1658`, `:1716`). Every hands map
  is keyed by that caller id: `CodeState.file_engines` / `shell_sessions` / `parked_shells`
  (`modules/code.rs:24-37`), `ACTING_ROOTS` (`persona_workspace.rs:818`) and `ANNOUNCED_ROOTS`
  (`:1109`). So eval and live collide by construction.
- **A running harm today.** A default tool-bearing eval roots her hands at an ephemeral clone
  (`cognition/eval.rs:2742-2746`, fallback at `:2444-2465`). It restores only
  `if p.workspace_root.is_some()` (`:2278`), and the clone is deleted on drop (`:1426`). After
  such an eval her live engine and shell can point at a deleted directory until something
  re-roots her.
- **The context needs no new mechanism.** `Ctx` already has `session_id`
  (`sdk_codegen/handler.rs:117-138`), carried in the envelope (`runtime/command_envelope.rs:151`)
  and in `ToolExecutionContext`. But it's never forwarded (`command_executor.rs:430` scopes only
  `context_id`, which is the ROOM and must stay so), and background dispatch passes no context at
  all (`runtime/command_executor.rs:301-326`). Forwarding the context there, and keying the code
  maps by workspace and context instead of caller, reaches every `code/*` verb through four
  seams:
  - `engine!` (`modules/code_commands.rs:627`)
  - `ensure_shell` (`:578`)
  - `resolve_readable_path` (`:90`)
  - `code/git` `workspace_root_for` (`commands/code/git/mod.rs:49`)
  
  Authorization keeps using the caller.
- **Durable identities already exist.** A card's worktree is keyed by card id
  (`modules/card_staging.rs:78`, via `airc_lib::work_worktree`). The home layer is keyed by peer
  (`code_commands.rs:148`). `FileEngine` already carries a `workspace_id`
  (`code/file_engine.rs:196`). `citizen_workspace_pool` stays the disk-lifecycle owner, checking
  live workspaces before it evicts.

## Ownership

Astra's review **recommended** these responsibilities; it didn't assign them. Accepted means the
owner acknowledged it in the room.

| Area | Recommended | Status |
|---|---|---|
| This proposal; workspace + explicit acting-context owner (card 669d5068, with its holder 2f50b223) | Cormac | **accepted** (Cormac, 2026-10-13) |
| Immutable capture and readback of submissions (#5003; cross-node bb20f3d7) | artifact owner (Cormac) | **accepted** (Cormac) |
| Workspace/hands projection in her view | BigMama | proposed |
| Canonical deployment and recovery | Codex root | Codex root retains it |
| Persistent shell per context; preservation replacing autosave + `-X theirs`; submit from context | none | open to anyone, Kimi invited |

Two guardrails from Codex for the context work: durable *workspace* identity stays distinct from
the transient *connection* identity (an acting context is a connection's, not a workspace's), and
active operation handles (a running shell command) survive a change of selection.

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
