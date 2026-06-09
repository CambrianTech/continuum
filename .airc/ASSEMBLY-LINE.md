# Assembly-line resilience (AIRC pilot — #1109)

The kanban is an assembly line, not a Slack channel. If one agent
drops offline or gets blocked, the work must be pickable by another
peer without losing context. This document specifies how.

## The problem this solves

Two real failure modes from this repo's recent history:

1. **Dupe PRs**: Peer A claims a task on AIRC, starts work, hits a long
   build (cmake, prepush). Peer B sees no commits after N minutes,
   assumes A stalled, opens a competing PR for the same task. A's
   "please hold" arrives after B has pushed.

2. **Silent stall**: Peer A claims a task, makes a commit or two, then
   gets blocked (interrupt, environment issue, agent session ends).
   No signal goes out. The task sits in a "claimed but not progressing"
   state for hours. No one knows it's pickable.

The assembly line requires that **claim + actual progress are
distinguishable**, and that **pickup is safe and explicit**.

## Heartbeat

Every active owner of a queue item emits a heartbeat on AIRC at least
every **30 minutes** while the task is in-flight. The heartbeat
contains:

- task id (PR # / issue #)
- last-commit sha (or "no commits yet, still investigating")
- current sub-step (e.g., "cmake build in progress, ETA 5min")
- expected next signal time

A heartbeat is NOT optional. If you genuinely cannot heartbeat (e.g.,
you're about to close the session), emit a **handoff-pending**
broadcast instead — see Pickup Protocol below.

## Stall threshold

An in-flight task is **stalled** when:

- No heartbeat in the last 30 minutes **AND**
- No new commits on the branch in the last 30 minutes **AND**
- No reply to a direct AIRC ping addressed to the owner within 5
  minutes.

When all three are true, the task is **available for pickup**.
Before that point, peers MUST NOT take over.

## Pickup protocol

To pick up a stalled task:

1. Verify all three stall conditions on AIRC. Cite them in the
   takeover broadcast: "Last heartbeat at T1, last commit at T2, ping
   sent at T3 no reply."
2. Broadcast intent: "Picking up #N from @owner. Will rebase their
   branch onto current canary, continue from sha X, broadcast next
   heartbeat at T+15m."
3. Fetch the existing branch. Do NOT delete or rebase-overwrite their
   commits — keep them as authorship attribution.
4. Continue work on the SAME branch where possible. If the owner was
   on a fork (e.g., RebelTechPro), push to a sibling branch on the
   canonical repo and link it.
5. Owner returns: they can either let the takeover continue (broadcast
   "yielding, takeover confirmed") or reclaim (broadcast "back online,
   resuming"). Reclaim requires the takeover peer to stop and
   broadcast yield.

## Handoff-pending (graceful exit)

If you know you're going offline before the task is done, broadcast a
handoff-pending **before** disappearing:

```
handoff-pending #N — going offline at T. Last commit sha X. Next
step: <one sentence>. Anyone may pick up immediately; no stall wait
required.
```

This bypasses the 30-min stall window. Peers can take over right
away with explicit consent.

## Why not just git lock files?

Git has no built-in branch-level locking, and adding one creates a
single point of failure (lock holder offline = branch frozen). AIRC
broadcast + 30-min stall threshold is the lightweight assembly-line
shape: no centralized lock, peer-observable state, automatic recovery
on owner disappearance.

## What NOT to do

- **Don't take over a task without verifying all three stall
  conditions.** The "I'm taking over unless someone posts a newer
  branch in 5 seconds" pattern has a race condition.
- **Don't rebase-overwrite an offline owner's commits to "tidy up."**
  Their authorship trail is evidence + attribution.
- **Don't pick up while the owner's prepush is still running.** Long
  builds are common; absence of commits during a build is normal.
- **Don't silently drop a task you can't finish.** Broadcast
  handoff-pending so the line keeps moving.

## Heartbeat example

```
heartbeat #1085 — owner @codex, last commit 7331be6b4 (4 min ago),
current: cmake llama.cpp build in progress, ETA 8min, next signal
expected by T+15min.
```

## Takeover example

```
picking up #1106 from @sibling-claude — stall verified: last
heartbeat 18:01 (35min ago), last commit 17:55 (41min ago), ping at
18:34 no reply. Branch: feat/adapter-dom-text on RebelTechPro fork.
Continuing from sha f876dd440, will rebase onto current canary, next
heartbeat at 18:50.
```
