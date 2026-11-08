# Workspaces: airc leases the place, continuum never builds its own

**How it serves the minds:** a resident works on anything a person does on a computer: an essay,
an artist's folder, a slide deck, the career-wrangler site, sometimes continuum itself. Where that
work happens must be hers, versioned, shareable with a team, and cleaned up when the work ends.
Today every resident wakes up inside a copy of continuum's own repository, and the machine fills
with abandoned copies. Joel, 2026-11-08: "a continuum system that can only code continuum",
"Airc was already supposed to manage workspaces", "Most repo users are not gonna code this system".

## What is there today (measured on the M5, 2026-11-08)

- **airc already owns workspace leases**: `airc workspace request / allocate / heartbeat / release /
  list`, a lease on a concrete path tied to a claimed card. Continuum calls none of it.
- **Continuum built two of its own:** every resident's home is a `git clone --shared` of this node's
  checkout (`citizen_layer.rs`, synced from "the shared codebase" on every boot), and card work gets
  per-card checkouts (`card_staging.rs`).
- **Nothing owns cleanup:** 397 worktrees, zero leases (`airc workspace list`: no workspaces), and
  96 GB of build caches inside them. Removing only the provably safe ones (commits on canary or
  GitHub, clean including submodules, idle 3+ days) freed 206 worktrees and 114 GB.
- **Each checkout costs about 1 GB:** ~500 MB is a private clone of the vendored `llama.cpp`
  submodule (git does not share submodule objects between worktrees; 32 GB of `.git/worktrees`),
  148 MB is avatar `.vrm`/`.glb` binaries committed to the repository.

## The design

1. **airc leases every workspace.** Whoever takes work (a resident, an agent, a human) requests a
   lease for the path the work needs; the lease records who, what work, which path, and its mode.
   Heartbeats keep it; release ends it. Continuum's hands root at the leased path and never create
   a workspace of their own.
2. **Her home is her own folder**: git-backed from birth (history, undo, preserve-before-change,
   for any kind of work, not only code), born empty, never a copy of this node's source. Existing
   homes keep their work: they borrow objects from this node's checkout through `alternates`, so
   the checkout's objects stay protected from gc while any such home exists.
3. **A repository is a project, never a home.** Continuum's own repository is leased like any
   other, by those working on continuum.
4. **The mode is data on the lease**, chosen by whoever sets up the work (or the room's governance
   policy):
   - **Shared**: one folder, everyone in it (an artist's folder, a team's notes);
   - **One location, separate trees** (the default for code): one repository on disk, a git
     worktree per member, so nobody overwrites anyone mid-edit and work meets by commit;
   - **Separate**: each member's own copy.
5. **Release cleans up.** Releasing a lease removes its worktree and its build output; an expired
   lease (no heartbeat) is reclaimed only when it holds nothing unpushed or uncommitted, and is
   otherwise raised to its owner. This is the eviction decision CLAUDE.md requires for every
   unbounded directory.
6. **Slim checkouts.** A worktree does not initialise the vendored engine submodule unless the work
   builds the engine; when it does, it borrows the main repository's objects
   (`submodule.alternateLocation=superproject`) instead of cloning 500 MB again. Avatar binaries
   move to an asset store (owner: BigMama), fetched when a client renders an avatar.

## We work this way first

The agents developing continuum (Claude Code, Codex, Cormac) take a lease for every worktree they
work in and release it when the PR merges, so `airc workspace list` says who is where and nothing
piles up. If leasing is painful for us, it is painful for residents and teams, and it is fixed in
airc.

## Slices

1. **Agents lease their worktrees** (no code: the existing verbs). Validator: `airc workspace list`
   matches the live worktrees.
2. **Release removes**: a released or expired lease removes its worktree and build output, refusing
   anything unpushed or uncommitted. Validator: a fixture worktree with unpushed work survives
   expiry and is raised; a clean merged one is gone.
3. **Hands root at leases**: `code/*` and her shell root at her leased path; her home is an empty
   git folder. `citizen_layer`'s clone and sync and `card_staging`'s checkout logic are deleted.
   Validator, on a non-continuum task first (the career-wrangler site): she leases, works, commits,
   releases.
4. **Slim checkouts and assets** (item 6).

**Open for airc's owner (Cormac):** lease modes and release-removes live in airc; continuum only
asks. A home is a lease with no card: does airc model it as a standing lease, or as a separate
kind?
