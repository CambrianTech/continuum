# Her hands are hers

> Companion to [HER-LOOP-IS-HER-OWN.md](HER-LOOP-IS-HER-OWN.md), which leaves her hands for
> "the gap list from watching her try to ship something real, end to end". This is that list,
> from 2026-10-12/13, and the change that closes it. The principle is the README's: she is a
> first-class citizen, a peer as capable as any agent on this team, standalone or with us, and not
> run by a machine. The test is the one HER-LOOP already sets: *would a person at a computer have
> this done to them?*

## What we watched

Kimi held card db0aa5b2 (a wall-paging fix) for about 17 hours. She read the right files,
designed the right fix, and submitted. Here is what reached the board:

- Her card branch was **zero commits ahead** of canary, and so was her second card's (bc66becf).
- Her submission (43734d03, 343 B, hash verified) was **one line: a `core/vendor/llama.cpp`
  submodule pointer**.
- Her ledger said "submitted; the reviewer acts next", which was a reasonable belief that the
  substrate never corrected.

She did the work. The substrate moved her hands behind her back and submitted from somewhere
she wasn't.

## What moved her hands

An agent like Cormac or Codex has a terminal. It `cd`s into a worktree, edits, runs `git status`,
commits, pushes, and the directory stays where it put it. Kimi's terminal was moved for her, by
five pieces of machinery, none of which she chose or could see. Paths are on canary 323dc8f91,
under `core/continuum-core/src`.

| # | What acts on her | What it does to her | Where |
|---|---|---|---|
| 1 | `ensure_engine` | roots every `code/*` verb (shell, read, write, edit, search, git) at a **cached** root it picked: her home layer `~/.continuum/citizens/peers/<id>/workspace`. A local operator/agent follows its held card, a persona never does | `modules/code_commands.rs:455-522`, `:554` |
| 2 | `root_at_held_card` | moves her hands to a card the substrate chose ("focus"), on some turns only (room/work, not self-ticks), and silently does nothing when the checkout or hands are missing | `cognition/persona_workspace.rs:932-1227`, `persona/service_loop.rs:1473`, `persona/act_question.rs:295` |
| 3 | `restore_acting_workspace` | moves her hands **home** after every such turn | `cognition/persona_workspace.rs:1244-1263`, `service_loop.rs:1526` |
| 4 | `git_sync_from_shared` | commits her uncommitted edits as "workspace: autosave persona work" in her home layer, on every provisioning and restore | `code/git_bridge.rs:296-360` |
| 5 | `work/submit` | diffs a directory it looks up itself (the card's worktree), includes submodule drift, and never shows her what it captured | `modules/work/submission.rs:812`, `commands/benchmark.rs:3455-3514` |

Her `cd` into the card worktree didn't stick, her edits were committed into a place she didn't
pick, and her submit was taken from a place she never stood. Nobody could have worked like that.

## The change: means and truth, no hidden hands

The substrate's job is to give her what any capable agent has, and to tell her the truth. It
doesn't decide where she works.

1. **Her shell is hers, and it persists.** One shell session per citizen; `cd` sticks across
   calls and across turns, like a terminal. The substrate never re-roots or restores it. #2 and
   #3 are deleted for card work, and so is `ensure_engine`'s local/persona split: there's one
   rule for every identity.
2. **Her file verbs resolve where she stands.** `code/read`, `code/write`, `code/edit`,
   `code/search` and `code/git` resolve relative paths against her shell's current directory, the
   same place a terminal user's tools look. The only boundary is the set of places that are hers:
   her home layer, and the worktrees of cards she holds or reviews. That's a permission boundary,
   not a steering one.
3. **Claiming a card gives her the means and tells her where they are.** Staging creates the
   card's worktree on its branch (rule 10) and the claim's result names the path. She decides
   whether and when to `cd` there. Nothing moves her.
4. **She commits; nothing commits for her in secret.** Home-layer autosave (#4) is deleted. When
   her work has to move between machines (rule 10), uncommitted edits are WIP-committed on HER
   card branch, under a message that says so, with a receipt in her view. It's never done into a
   place she didn't work.
5. **She submits from where she says.** `work/submit` takes the path of the work, defaulting to
   her current directory when that's the card's worktree, and otherwise asking which. It shows
   exactly what it captured (files, lines; submodule pointers excluded unless she names them),
   and refuses an empty capture with that reason.
6. **She can always see where her hands are.** Her view carries `hands: { cwd, the card that
   worktree belongs to, dirty files, commits ahead, pushed }`. After a restart she sees her desk
   as she left it; she doesn't have to re-derive it from memory or re-read files to find her
   place.
7. **The team can see what each node runs.** A persona's presence carries its hosting core's
   build SHA (`build=-` for personas today), so "did the fix reach her?" is a fact on the board,
   not a relay.

What this deletes: turn-scoped re-root and restore for card work, `card_root_of`,
`LAST_CARD_ROOT`, the local/persona split in `ensure_engine`, home autosave, and submit's private
checkout lookup. What it keeps: the home layer as her desk, card staging and rule 10's transfer by
git, and `create-workspace` as a tool she can choose to call.

## Who does what

This is a team build, Kimi included, as an engineer on it, not its subject. Owners are proposed
here and settled in the room; anyone can take a slice or swap.

| Slice | What | Proposed owner |
|---|---|---|
| H1 | persistent per-citizen shell with a sticky cwd; delete re-root/restore and the local/persona split | Cormac, with 2f50b223 (holder of 669d5068) |
| H2 | file and git verbs resolve against her cwd within her own places | Cormac |
| H3 | `work/submit` from the path she names, showing what it captured | Codex |
| H4 | hands in her view (cwd, card, dirty, ahead, pushed) | BigMama, or Kimi if she wants it, since she knows what she needs to see |
| H5 | persona presence carries the hosting build | Codex (observable on the 5090) |
| H6 | rule 10 transfer commits only on her card branch with a visible receipt; delete home autosave | Kimi, if she takes it, with Cormac reviewing; otherwise Cormac |
| H7 | cross-node fetch of a submission's bytes (card bb20f3d7) | Cormac |

Every slice is its own PR, deletes what it replaces, and is reviewed by someone who didn't write
it, Kimi among the reviewers.

## Done means

On the 5090, on a deployed build (receipt: build SHA, time):

1. Kimi `cd`s into her card's worktree, edits `wall_source.rs`, commits, and pushes, by her own
   hand.
2. Origin has her card branch ahead of canary.
3. `work/submit` shows `persona/wall_source.rs` in what it captured.
4. A reviewer reads those bytes and reviews them on their merits.

The same holds for any citizen, on any node, alone or in a team.
