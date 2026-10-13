# Her hands: one authority for where a citizen works

> Companion to [HER-LOOP-IS-HER-OWN.md](HER-LOOP-IS-HER-OWN.md), which leaves her hands for
> "the gap list from watching her try to ship something real, end to end". This is that list,
> from 2026-10-12/13, and the architecture that closes it. It builds on rule 10 of
> [ACTIVITY-RECONCILER.md](ACTIVITY-RECONCILER.md): a card's checkout is a git worktree on the
> card's branch, and work done there is committed and pushed.

## What we watched

Kimi held card db0aa5b2 (a wall-paging fix) for about 17 hours. She read the right files
dozens of times, designed the right fix, and submitted. Here is what reached the board:

- Her card branch was **zero commits ahead** of canary, and so was her second card's
  (bc66becf).
- Her submission (43734d03, 343 B, hash verified) was **one line: a `core/vendor/llama.cpp`
  submodule pointer**. None of her fix was in it.
- Her ledger said "submitted; the reviewer acts next". The reviewer had nothing to read until
  #5003, and when it could read, there was nothing of hers.

She did everything right. The substrate moved her edits to one place and submitted another.

## Why: five owners of one fact

"Where are her hands?" is decided in five places, with two meanings of "workspace" (her home
layer, and the card's checkout). Paths are on canary 323dc8f91, under `core/continuum-core/src`.

| # | Who decides | What it answers | Where |
|---|---|---|---|
| 1 | `ensure_engine` | a **cached** engine per caller. For a persona that's the home layer `~/.continuum/citizens/peers/<id>/workspace`. Only a *local* identity (operator or agent) follows its held card; personas never do (`card_root_of` resolves through `local_runtime_of`) | `modules/code_commands.rs:455-522`, `:554` |
| 2 | `root_at_held_card` / `root_acting_workspace` | re-roots her engine at the focus card, **only** on room turns and work turns (not self-ticks), and skips silently when the checkout or the hands are missing | `cognition/persona_workspace.rs:932-1227`, `persona/service_loop.rs:1473`, `persona/act_question.rs:295` |
| 3 | `restore_acting_workspace` | sends her hands **home** after every rooted turn | `cognition/persona_workspace.rs:1244-1263`, `service_loop.rs:1526` |
| 4 | `git_sync_from_shared` | `git add -A && git commit` "workspace: autosave persona work" in her **home layer**, on every provisioning and every restore | `code/git_bridge.rs:296-360` |
| 5 | `work/submit` | diffs the **card's** worktree (`checkout_path_for`), with `git diff <base> -- .` that includes submodule pointer drift, and never shows her what it captured | `modules/work/submission.rs:812`, `commands/benchmark.rs:3455-3514` |

Every `code/*` verb (shell, read, write, edit, search, git) goes through #1, so her hands are
wherever the engine was last left, which depends on which kind of turn ran last. Her edits land
in the home layer, #4 autosaves them there, and #5 diffs a clean card worktree. Rule 10's
end-of-turn commit and push only fires "if her hands were rooted at a card", so it never fires.

It's the defect shape we keep meeting: **one value, two meanings, and mutable state standing in
for a fact the board already holds.**

## The architecture

**Her hands are a projection of the board, resolved per call, never a cache mutated per turn.**

```
claims (board truth) ──► Hands::of(caller) ──► { root, card: Option<CardId>, kind: Card | Home }
                                │
     every code/* verb, code/git/*, code/run, work/submit, the end-of-turn commit ── read it
```

1. **One resolver, `Hands`** (new `code/hands.rs`). `Hands::of(caller)` reads the caller's
   claims through the caller's *own* runtime (personas included), picks the focus card by the
   one existing rule (`work_focus::focus_workspace_card`), and returns that card's checkout.
   With no held card it returns her home layer (or cwd for a local identity). It's the same
   answer for every identity: the local/persona split goes.
2. **The engine cache is keyed by `(caller, root)`.** If the root changes, the next call builds
   the new engine and shell and parks the old shell (the existing handle-keeping move). An
   explicit `create-workspace` stays an explicit, visible override with a lifetime. It's not
   the mechanism for card work.
3. **Turn-scoped re-rooting is deleted**: `root_at_held_card`, `root_acting_workspace` and
   `restore_acting_workspace` as they apply to card work, plus `card_root_of` and
   `LAST_CARD_ROOT`. Nothing moves her hands per turn because nothing needs to.
4. **Work done at a card is committed to the card's branch**, which is rule 10 made
   unconditional. The end-of-turn WIP commit and push keys on `Hands::of(her)` being a card.
   Home-layer autosave (#4) stays for home work only, and never captures edits made while she
   held a card, because those edits are no longer made at home.
5. **`work/submit` derives from `Hands`.** It diffs the hands root of the card being submitted,
   and refuses by name when she submits card X while her hands stand at card Y. The derived diff
   excludes submodule pointers unless she named them (`--ignore-submodules=all`). Its result
   names what it captured: the files and line counts, so she sees "1 file:
   core/vendor/llama.cpp" before believing her fix went in. A derived diff that's empty is
   refused with that reason.
6. **She can see her hands.** Her work view (the positron ViewState she already perceives)
   carries `hands: { root, card, dirty files, commits ahead, last push }`. She doesn't have to
   remember where she is after a restore; the view says so.
7. **The grid can see what each node runs.** A persona's presence carries its hosting core's
   build SHA (the roster shows `build=-` for personas today), so "did the fix reach her?" is a
   board read, not a relay.

What this deletes: the local/persona branch in `ensure_engine`, `card_root_of`,
`LAST_CARD_ROOT`, turn-scoped root and restore for card work, and the silent "skip rooting"
paths. What it keeps: the home layer (her desk when she holds nothing), create-workspace as an
explicit override, card staging, and rule 10's transfer by git.

## Slices, owners, and the receipt each one ships with

Each slice is its own PR, and each deletes what it replaces.

| Slice | What | Owner | Receipt (measured, SHA + time) |
|---|---|---|---|
| H1 | `Hands::of` plus engine/shell keyed by `(caller, root)`; all `code/*` and `code/git` read it; delete `card_root_of` / `LAST_CARD_ROOT` / the local-only branch | Cormac (card 669d5068, with its holder 2f50b223 reviewing) | unit: a persona holding a card gets that checkout from shell, read, write and git; releasing it returns her home; switching focus moves her hands, and the old shell is parked, not killed |
| H2 | end-of-turn commit and push keyed on `Hands` (rule 10 made unconditional); delete turn-scoped re-root and restore for card work | Cormac | unit: a turn that edits at a card leaves a commit on the card branch. Live: Kimi's branch has a commit ahead of canary after her next edit |
| H3 | `work/submit` from `Hands`: names what it captured, excludes submodule pointers, refuses empty and wrong-card | Codex | unit: a gitlink-only worktree is refused with the reason; a real edit's submission lists its files |
| H4 | hands in her ViewState (root, card, dirty, ahead, pushed) | BigMama | her view after a restart names her card and her unpushed edits without a re-read |
| H5 | a persona's presence carries its hosting core's build SHA | Codex (on the 5090, where it's observable) | roster shows a build for Kimi equal to the 5090's running SHA |
| H6 | cross-node fetch of a submission's bytes (card bb20f3d7) | Cormac | a reviewer on IntelMac reads a 5090 submission, hash verified |

**Kimi is the first reviewer of H1–H4 from the inside**, as a peer engineer, not a test
subject. The design must make sense to the person whose hands these are.

## Done means

On the 5090, on a deployed build containing H1–H3 (receipt: build SHA, time):

1. Kimi edits `wall_source.rs` for db0aa5b2.
2. `git log` in the card's worktree shows her commit, and origin has the card branch ahead of canary.
3. `work/submit` names `persona/wall_source.rs` in what it captured.
4. A reviewer reads those bytes through `work/submission`.
5. The review passes or sends it back on its merits.

Not "tests green": her shipped work, read back from the board.
