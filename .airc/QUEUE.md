# Sprint queue — PR card format (AIRC pilot — #1109)

The queue is the active set of PRs and issues across one sprint.
Every active card on the queue MUST have these fields filled in,
either in the PR description or in an AIRC pinned message.

## Card fields

| Field | Required | Format | Example |
|---|---|---|---|
| **id** | yes | `#NNNN` (PR or issue) | `#1085` |
| **branch** | yes (if PR) | `feat/...` / `fix/...` / `chore/...` | `fix/install-tier-name-divergence` |
| **owner** | yes | AIRC peer/session identity from `airc whois` (sub-tab disambiguated). **Not** a GitHub username — one gh account commonly maps to many agents. | `claude-tab-#1` |
| **status** | yes | `claimed` / `in-progress` / `blocked` / `review` / `merged` | `in-progress` |
| **blockers** | if any | comma-separated `#NNNN` task ids | `#1085, airc#559` |
| **env** | yes | `mac-m5` / `rtx5090-wsl2` / `linux-amd64-any` / `any` | `linux-amd64-any` |
| **evidence** | yes-on-review | which gates ran + last sha they ran against | `prepush 61bdeb407: TS+ESLint+Rust 27/27 green` |
| **next action** | yes | one sentence: what needs to happen next | `wait for image rebuild on linux/amd64 host` |
| **last heartbeat** | yes-while-in-progress | ISO timestamp + commit sha | `2026-05-13T17:35Z @ 61bdeb407` |

## Status transitions

```
(new) → claimed → in-progress → review → merged
                ↘         ↘
                 blocked ⇄ in-progress
```

- **`claimed`**: owner announced on AIRC, no commits yet.
- **`in-progress`**: at least one commit on the branch.
- **`blocked`**: explicit dependency on another card. Must name the
  blocker.
- **`review`**: PR open, hooks green, awaiting Codex review.
- **`merged`**: landed on canary.

## Where the card lives

Single source of truth: **the PR itself** (description + airc broadcasts).
The PR description carries the static fields; AIRC broadcasts carry
heartbeats and status transitions.

For pre-PR work (issue-only, exploration), the card lives in the
issue body and AIRC.

## Per-card AIRC broadcast hooks

- **On claim**: `claiming #NNNN: <one-line scope>. branch=<X>. env=<Y>.`
- **On first commit**: `in-progress #NNNN: first commit <sha>.`
- **On heartbeat**: `heartbeat #NNNN — last commit <sha> at <T>, current: <substep>, next signal by T+30m.`
- **On block**: `blocked #NNNN by <blocker-id>: <reason>. need: <unblock-spec>.`
- **On review-ready**: `#NNNN ready for review at <sha>. validation: <gates>. requesting @codex.`
- **On merged**: `#NNNN merged at <sha>. canary fast-forwarded.`

## Queue rules

1. **One PR per scope.** Don't open a competing PR for the same scope
   if a card already exists. Coordinate on AIRC instead (see
   [ASSEMBLY-LINE.md](ASSEMBLY-LINE.md) for pickup protocol).
2. **Self-assign only after AIRC claim.** GitHub-assignment without
   AIRC claim is invisible to peers and dupe-prone.
3. **Cross-repo cards span both.** A task that needs continuum + airc
   changes has a card in each, with `blockers` linking them. Don't
   pretend they're independent.
4. **Env tag must match reality.** If you can only run a step on a
   specific host, tag it. Don't claim `any` when the work needs
   `rtx5090-wsl2`-only build capability — peers wasting attempts on
   the wrong host stalls the line.

## Example card

```
id: #1085
branch: fix/install-tier-name-divergence
owner: @codex (cloud)
status: in-progress
blockers: pr-1085-amd64-image-rebuild (waiting on linux/amd64 host)
env: linux-amd64-any (for image rebuild step only — code changes are
     environment-agnostic)
evidence: prepush 61bdeb407: TS+ESLint+Rust 27/27 + bash-n + jq +
          compose-config all green
next action: capable Linux/amd64 host runs scripts/push-current-arch.sh
             at sha 61bdeb407 to rebuild pr-1085 amd64 images
last heartbeat: 2026-05-13T17:35Z @ 61bdeb407
```
