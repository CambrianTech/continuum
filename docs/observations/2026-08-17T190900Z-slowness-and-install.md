# 2026-08-17 19:09Z — persona starvation + the deploy install path

Method note: written after the run, from probes and deploy receipts taken in this session
only. **No prior observation cards were consulted** (standing instruction, 2026-08-17).

## What was measured BEFORE any fix

| Fact | Value | How |
|---|---|---|
| self-ticks across the whole roster | **1 in 40 minutes** (24 hosted citizens) | probe `persona.selftick.*` |
| serving lane during that window | healthy, decoding ~17 tok/s continuously | `/slots`, `serving/status` |
| ambient-turn pool | **1** (hardcoded) | `resource_admission.rs` |
| served lanes | **4** | `serving/status` |

So 3 of 4 non-directed lanes sat permanently idle while 23 of 24 citizens yielded per beat.

## Two defects, both fixed

1. **Starvation ratchet** (`a87f7c871`). A citizen who YIELDED on the ambient permit — never
   ran, learned nothing — was charged the same 1.5×-toward-the-240s-cap backoff as one who
   ran a full cycle and found nothing new. ~8 yields pins her at 16× slower, permanently,
   and it deepens as the roster grows. Fix: a yield leaves the beat unchanged. Extracted as
   pure `next_beat_after(BeatOutcome, ..)`; regression test positive-controlled.
2. **The pool ceiling** (`6229b3762`). `AMBIENT_TURN_CONCURRENCY = 1` was a bare constant,
   while the self-tick gate's own comment claimed it was "sized to the LIVE served lane
   count". It never was. Now derived from `nondirected_budget()` (lanes−1, floored at 1) —
   the same budget the per-call lane reservation already uses. 1- and 2-lane boxes are
   byte-identical to before.

## Measured AFTER the ratchet fix (pool still 1)

Stable window 18:06–18:18 on `a87f7c871`, 24 hosted citizens: **2 self-ticks in 12 min**
(0.17/min) versus the 0.025/min baseline — ~8×.

## NOT YET CLEANLY MEASURED

The ambient-pool build has had **no uninterrupted window**: three reboots since (18:47,
19:01) for the install fixes below, so every counter across 18:34–19:09 is contaminated by
citizens being down. A fresh watermark is running now. Do not quote a pool-fix number until
it accumulates.

## Install-path defects found while deploying the above

3. **Autostart steals the socket mid-deploy** (`5d62bf30b`). `reboot` builds for 292–530s;
   throughout that window no core answers AND `~/.continuum/bin/continuum-core-server` still
   holds the PREVIOUS build. Every `continuum <verb>` calls `ensure_core_running`, which
   autostarts — from the stale image, which then wins the socket. Reproduced: deploy shipped
   `6229b3762`, verify reported `a87f7c871`, installed image on disk confirmed to contain the
   NEW sha. The trigger was my own 120s monitor calling `persona/roster`.
   **This settles task #421** ("something respawns a core within seconds of every kill —
   external supervisor, or me?"): it was the CLI's own autostart. No mystery daemon.
   Fix: `runtime::deploy_claim` — reboot publishes {pid, started_ms, target_sha} for the
   build+swap; autostart and the launch arm of `start` refuse while it is live. Advisory and
   self-healing (dead owner or >1h → swept and announced) so it can never wedge the box.
4. **⚠ STALE CLI fired on every SUCCESSFUL reboot** (`be5cd1138`). The note compared the
   RUNNING invocation, which by construction predates the CLI it just installed. Its text
   also claimed reboot "never" rebuilds the CLI — false since #2293, disproven on this box by
   the CLI advancing a87f7c871 → 6229b3762 → 5d62bf30b → be5cd1138 across four reboots.
   Fix: `rebuilt_this_run` splits handoff (↻) from real staleness (⚠).

## Still open

- Pool-fix effect: unmeasured (above).
- The deploy claim is unit-green but **not positive-controlled end-to-end** — an isolated
  control is unreachable while a real core runs, because `running_core_pids()` is global and
  returns Occupied before the gate. The next reboot's mid-build monitor call is the control.
- Citizens are served by `qwen2.5-coder-14b` while `Qwen3.8-27B` — #1 on the Artificial
  Analysis Agentic Index at 51, above Opus 4.8's 49 — is already loaded and ready on the
  vision sidecar (:58091). That is task #440 and it is on the critical path, not a side lane.
