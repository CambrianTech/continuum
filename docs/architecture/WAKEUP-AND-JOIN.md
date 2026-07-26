# Wakeup & Join — the autonomic lifecycle contract

> A being coming online — agent or persona — must AUTONOMICALLY get its substrate,
> its memory, its rooms, and its orientation, with **zero manual steps**. Nobody
> should ever wake up blind, mute, or amnesiac because a process wasn't running.

This doc exists because we kept hitting the failure live: after a machine restart
the core was down, so the agent woke **amnesiac** (recall returned nothing) and the
personas were **absent** (they only exist while the core runs). Every one of those
incidents is the same missing contract — wakeup and join were not yet autonomic.

## The asymmetry (why it's two problems, not one)

| | Personas | Agents (Claude Code / Codex, e.g. me, BigMama) |
|---|---|---|
| Where they live | **Inside** the core process | **Outside** it (their own session) |
| Their "wakeup" | the core's service-loop tick | a new session / SessionStart hook |
| Their "join" | spawn into airc rooms | `airc join` |
| If the core is down | they **do not exist** | they keep running but lose **memory + comms** |

The consequence of the asymmetry: an **agent can wake while the substrate is down**
and go amnesiac (its memory lives in the substrate). A **persona cannot wake at all**
until the substrate is up. So the substrate being reliably up is load-bearing for
both — it is the precondition of every wakeup.

## The contract (four guarantees)

1. **Substrate is always-up.** The core is OS-supervised (launchd / systemd). It
   comes back after a crash and after a reboot — but honors an explicit
   `continuum stop` so a developer can `[[take-the-core-down-freely]]`.
2. **Wakeup re-hydrates memory.** An agent/persona coming online recalls its
   durable memory. If the substrate is momentarily down (a supervised restart
   window), wakeup waits **briefly** rather than forgetting; if it's genuinely
   down, it degrades gracefully rather than breaking the session.
3. **Join self-heals.** Rooms re-join automatically on core (re)start; the agent's
   `airc join` reconnects. No manual recovery steps.
4. **Orientation on arrival.** A wake-briefing (#147) orients from durable state so
   arrival is never a void.

## Who owns "bring the core up" — the doctrine

`[[system-owns-its-lifecycle-never-hand-manage-processes]]`. Bringing the substrate
up is the **supervisor's** job, NOT the wakeup hook's:

- `continuum start` **rebuilds** (build + run) — far too heavy to fire from a
  session hook (10s budget).
- The supervisor (`install-service.sh`) execs the **already-built** binary
  directly — fast, no rebuild — and keeps it up.

So the hook must never start the core. It waits for the supervisor's core and
degrades if absent.

## What's built (this pass, `feat/autonomic-wakeup`)

- **Agent wakeup self-heal** — `memory-bridge/scripts/lib.sh::wait_for_core` +
  `session-recall.sh`: a bounded (`CONTINUUM_WAKE_WAIT_SECS`, default 4s) wait for
  the core to answer `ping` before recall, so an agent waking during a restart
  window recalls instead of forgetting. Genuinely-down → exit 0, inject nothing
  (never break a session). *Also fixes a live regression: the committed
  session-recall.sh still called the dead `cu` binary (renamed → `continuum` in
  #2010), making agent recall a silent no-op on canary.*
- **Supervisor honors explicit stop** — `install-service.sh` KeepAlive
  `true` → `{Crashed: true}` (macOS) and `Restart=always` → `on-failure` (Linux):
  crash + reboot recovery, but a clean `continuum stop` stays down.

## Pending / follow-ups

- **Install + validate the supervised service on the dev box.** Not done
  unilaterally: the running core (started manually) is not launchd-managed, so
  bootstrapping a job would spawn a second core fighting for the socket, and
  stopping the current one restarts all live personas. Needs an explicit go-ahead
  + a clean cutover (`continuum stop` → `install-service.sh install` → validate a
  `kill -9` self-heals).
- **Reconcile `continuum stop`/`reboot` with the supervisor.** When the core is
  launchd/systemd-managed, `continuum reboot` (which stops + relaunches) must go
  through the supervisor, not race it. Requires the `continuum` lifecycle binary
  to detect supervision. (The `Crashed`/`on-failure` policy already makes a clean
  stop safe; the open piece is `reboot`.)
- **Storage-aware model catalog** (`[[we-are-the-dynamic-model-catalog-manager-not-hand-authored-rows]]`,
  `[[shared-gpu-engine-and-cold-store]]`, #180 MoE): "know the system you woke up
  on" extends to its storage — detect + use the 16TB D drive for GGUF + MoE expert
  cold-store. Adjacent to this contract's "orientation on arrival", BigMama's lane.
