# Restore Economy & Team Scale — the path to 10–14 personas on one Mac

*2026-09-02. Scoped by a REAL attempt this session, not a guess: spawned 8 personas
on a 5-lane M-series core, waited 5+ min, and residency stayed at 4 while inference
capacity was consumed. This plan is what stands between today and the advertised bar.*

## Why this exists (the north star)

The claim is **best score AND speed on a MacBook, via collaboration** — and the
headline demo is **14 personas on an M1** ([[live-media-plane-restoration-bar-14-personas-on-an-m1]],
[[the-north-star-best-score-and-speed-on-a-macbook-via-collaboration]]). Teams are
the wedge we believe raises both score and speed over a lone solver. So running
10–14 personas *through* a 5-lane core — not merely owning 14 identities — is a
load-bearing feature, not a nicety.

## What works TODAY (the achievable-now floor)

Validated live this session (build `0b418bd47`):
- Deterministic completion edge (`PASS: done` → conclude), self-tick held-work loop,
  fast cadence, held-work visibility, backlog guard — all committed + tested.
- **Teams ≤ lanes run genuinely concurrently**: 4 personas' generations interleave
  through continuous batching (adapter semaphore = lanes, ambient permit = lanes-1).
- Scores climbing on this Mac: swe-bench-lite 58%, verified 67%, mini 100%.

So a team of **≤ 5** works now. The gap is everything above that.

## What BLOCKS 10–14 (measured, in dependency order)

1. **Birthing is slow / not on-demand.** `persona/spawn --count=8` produced zero
   `persona:born` in 5 min; instances trickled 4→5→6. A team you cannot stand up in
   seconds is not a team you can demo. Fix the keypair-ceremony + seed pipeline to
   mint+seat a citizen fast (batch the crypto, defer the avatar, seat the service
   loop before the cosmetic tail).
2. **Concurrent residency caps near lane count (~4–5).** The substrate runs about as
   many active service loops as it has lanes; 10–14 are never concurrently resident.
   The advertised bar therefore requires **ROTATION**, not more simultaneous
   residents: N personas time-share L lanes, paged in/out.
3. **KV paging is FIFO in the fork.** llama-server's single task thread processes
   slot save/restore/erase one-at-a-time (interleaved with decode). This session
   fixed save-vs-decode (a save no longer defers behind a live decode, fork
   `878db9784`) but NOT save-vs-save: read-only saves could overlap; restores of
   distinct sequences could pipeline. At 14-on-5 the switch rate is high and serial
   paging dominates — exactly the "pays greatly at 10+" intuition.
4. **Held-work starves under contention.** Measured: 22 prefills / 1 work.gate in a
   10-min window while 8 personas birthed + a 32-round backlog re-said kickoffs.
   Productive held-work must not lose its turn to background/birthing/message noise.
5. **Serving-concurrency controls are scattered.** Adapter semaphore (lanes), ambient
   permit (lanes-1), nondirected sub-cap, KV slot lease, task-queue FIFO — each solved
   locally. Like ServiceModule collapsed the lifecycle, these want ONE governed
   pattern so a new concern inherits "how many run, who yields, how state pages."

## The arc (slices, each lands green + measured)

**S0 — Measure where the switch time goes (no code; gates the rest).** On a 5-lane
core, force paging (6+ pinned activities) and decompose one persona-switch:
save ms, restore ms, FIFO wait (queued behind another slot op / decode), re-prefill
if the restore missed. Land the numbers in this doc. Do not build S3 before S0 says
serial paging is actually the cost (the "measure before leaning" discipline).

**S1 — Fast birthing.** Mint+seat a citizen in ~seconds: batch/parallelize the
keypair ceremony, seat the service loop first, defer avatar/card cosmetics. Acceptance:
`persona/spawn --count=8` reaches 8 resident-or-ready in < ~30s.

**S2 — >lane residency via rotation (the restore economy core).** A residency
governor that admits N > lanes personas and rotates them: the least-recently-active
citizen's KV pages OUT (save), the returning one's pages IN (restore), through the KV
slot lease already in `inference/slots.rs`. Held-work drives the rotation (whoever is
due to work pages in). Acceptance: 10 personas cycle through 5 lanes, each getting
turns, no citizen starved > a bounded window.

**S3 — Fork paging concurrency (un-FIFO).** In the vendored fork: let read-only slot
SAVES run without serializing behind each other / behind decode (a reader path), and
pipeline save(evictee)+restore(returner) for a switch. Gated by S0. TDD in the fork's
`test_slot_save.py` (extend the live-slot test). Acceptance: aggregate switch
throughput scales with lanes, not 1/task-thread.

**S4 — One governed serving/paging pattern.** Collapse the scattered controls (S5 in
the findings) into a single lease/admission concern at the lowest trait — the CBAR
"code it once" move. New concerns inherit governed concurrency + paging.

**S5 — Held-work priority under contention.** Reserve capacity (or a priority tier)
so productive held-work is never starved by birthing/background/message noise — pins
the S4 policy against the 22-prefill/1-gate failure.

## Acceptance (the advertised claim, as numbers)
- Stand up a **14-persona team** on the M-series core in seconds (S1).
- All 14 get turns through 5 lanes via rotation; **switch latency measured** and
  bounded (S0/S2/S3).
- A **team round** completes faster AND at ≥ the score of a lone solver on the same
  seeds — the collaboration thesis, on the Mac.
- Held-work never starves (S5): work.gate fires continuously while cards are held.

## Standing rules
- Deploy via `continuum reboot` + SHA verify; measure with named probes before
  claiming a slice; TDD each slice (unit + the fork's python harness). Feature-branch
  commits free; main merge needs Joel. Reuse existing primitives (`SlotRegistry`,
  `ThroughputLease`, `HoldLease`, `ServiceModule`) — never a parallel one.
- See also `plans/TEAM-PROOF-PROTOCOL.md` and the fork branch
  `continuum/kv-live-slot-save`.
