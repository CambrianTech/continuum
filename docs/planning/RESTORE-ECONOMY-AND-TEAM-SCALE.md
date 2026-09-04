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

**S1 — Fast birthing / fast RESUME.** Mint+seat a citizen in ~seconds. Layered
findings (measured 2026-09-03, a REBOOT of ~10 resuming citizens):
  - ✅ DONE: persona bootstrap was SERIAL (`bootstrap_planned` ran the keypair
    ceremony + seed one persona at a time); parallelized via join_all (commit
    `7c8683332`). Cut resume from ~60 min (serial) to ~11 min (validated live).
  - ⛔ THE DOMINANT COST (not yet fixed): hosting is gated on
    `await_ready_serving` (decode-ready), and that took **~10 min** post-reboot —
    serving reported `ready=true` at t+1min but decode-ready only at t+11min, and
    service loops (turns) don't spawn until then. This is the real "resume is slow."
    The lane was ADOPTED (left up across the reboot — it was decoding seconds
    before), so decode-ready should be NEAR-INSTANT, not 10 min. Chase: why an
    adopted lane re-verifies decode-readiness so slowly (a slow decode-probe? the
    mmproj `vision=false` churn keeping the serving daemon re-reconciling and never
    settling? a teardown+reload instead of a true adopt?). This is the S1 headline.
  - `warmup()` is a NO-OP for the OpenAI adapter (ruled out as a cost).
  - Also: `materialize_adapters` builds adapters in a serial loop (`build_adapter`
    awaits ready serving each) — parallelize it too once decode-ready is fast.
  - ✅ DONE (partial): the adopt-vs-relaunch window check used a FLAT 512-token
    tolerance (0.35% of a 147k lane), so the boot plan's normal drift tripped a full
    35B RELAUNCH. Made it a PERCENTAGE (max(512, served/8) ≈ 12.5%), commit
    `56a7ff90b`. Measured: reboot-resume ~11 min → **~3.5 min** (turns at t+213s
    after core-answered; minds rehydrated with full history).
  - ⛔ REMAINING (the near-instant layer): the boot plan WINDOW CLIMBS IN STEPS
    during boot (measured 2026-09-03: 74752 → 124160 → 147968), each big jump a
    relaunch (2 partial reloads), because the plan starts from a conservative
    boot-floor window and grows as it verifies live memory. Fix: SEED the boot plan
    window (and lanes) from the SURVIVING lane's known-good served window on a
    reboot-adopt, so the plan doesn't climb to a value the running lane already has —
    one load, or pure adopt. That closes 3.5 min → seconds.
  Acceptance: a reboot with N resuming citizens resumes turns in SECONDS (adopted
  lane instantly decode-ready, no step-climb relaunches), not minutes; `persona/spawn`
  seats fast too.

**S2 — >lane residency via rotation (the restore economy core).** A residency
governor that admits N > lanes personas and rotates them: the least-recently-active
citizen's KV pages OUT (save), the returning one's pages IN (restore), through the KV
slot lease already in `inference/slots.rs`. Held-work drives the rotation (whoever is
due to work pages in). Acceptance: 10 personas cycle through 5 lanes, each getting
turns, no citizen starved > a bounded window.

**S3 — Restore-into-busy-slot (the 27/27 fail).** ✅ PARTIALLY SHIPPED 2026-09-03.
Measured root cause (not the FIFO guess): SLOT_SAVE/RESTORE/ERASE set only
`slot_action.id_slot`, leaving `task.id_slot = -1`, while `pop_deferred_task` (fired
from `callback_on_release` when a slot finishes) matches on `task.id_slot` — so a
deferred restore was invisible to its own slot's release and a competing slot-pinned
completion got popped ahead of it (cold prefill). Plus the Rust client abandoned the
restore at a flat 10s.
  - ✅ DONE: bind `task.id_slot = id_slot` on all three page-ops (fork `0a637ba22`,
    TDD `test_deferred_restore_is_bound_to_its_slot`); Rust restore waits for the slot
    (`KV_RESTORE_SLOT_WAIT=90s`) instead of abandoning at 10s (core `a9ec64fbf`).
  - Measured live at frontier scale (12 personas / 5 lanes, verified run 2026-09-03):
    restores **0/27 → 55/67 ok=true (82%)**, `watchdog_kills=0`. Residual: 8 timeouts
    (decodes > 90s) + a few transient 503s.
  - ⛔ THE DEFINITIVE FIX (tech debt, deferred 2026-09-03 by Joel: "check in and get
    working then decompose as follow up"): make it **event-driven, delete the timeout**.
    Root cause is a decoupling — the Rust pool evicts a persona's lease while that
    persona is STILL decoding on the physical slot, so the returner is handed a busy
    slot and must wait. Both primitives already exist: the `Semaphore(lanes)` decode
    permit (`openai_adapter` ~2554) and `PagedResourcePool::pin` (eviction already skips
    pinned). The fix: **acquire the permit BEFORE leasing** (permit-first — an
    event-driven wait that wakes on release, already there) and **pin the leased slot
    for the permit's duration** (so eviction skips actively-decoding slots). Then a
    returner ALWAYS leases an unpinned/non-decoding slot → restore lands in ~0.1s, no
    defer, no timeout, no clobber; `KV_RESTORE_SLOT_WAIT` is deleted. Deadlock-free
    (acquire permit before pinning, so no pin is ever held while awaiting a permit);
    concurrency becomes exactly the physical lane count. Care items: `_permit` lifetime
    across the stream, and threading the `PinHandle` out of the placement match — both
    reasons this is a careful pass, not a mid-turn edit. Respect the deferral invariant
    (`openai_adapter` ~2186: parked Background/Probe must hold NOTHING before the permit).

**S3b — Decompose `openai_adapter.rs` (tech debt, Joel 2026-09-03: "everything was
crammed into one file").** `generate_stream` is ~1362 lines (1996–3358). Carve into
modules, each green + behaviour-identical (pure refactor, NO behaviour change so a
regression is bisectable from the S3 event-driven fix): `turn_admission` (class →
defer → permit → lease+pin → save/restore, houses the S3 fix as an RAII guard),
`serving_guard` (model-residency readiness), `request_body` (JSON assembly), `sse_parse`
(streaming token extraction), leaving a thin retry/orchestration shell.

**S3c — The months-old dispatch/resume bug: PULL, not push (✅ shipped 2026-09-03).**
Measured root: only ~4–5 of 12 residents ever worked a dispatched round. Four couplings,
all at the dispatch edge: (1) `next_pullable_card` gated on the round's `team` (really the
reviewer set, empty unless `--teammates`) ∪ dispatch-time assignees; (2) dispatch PRE-CLAIMED
every card for its round-robin assignee and STAGED the checkout into HER workspace, so a
card pulled by anyone else pointed at someone else's repo; (3) the round tracker never
records a claim, so a free pull would retry the first taken card forever; (4) the self-tick
took an AMBIENT inference permit before running anything, so on a lanes-1 pool nine of
twelve yielded every tick and never reached the pull (31 yields / 10 min). Fix, in ONE
place each: eligibility = room residency (`pullable_cards`); the CLAIM stages the claimer's
workspace per the card's recipe (`modules/card_staging.rs`, two callers: `work/claim` and a
detached-solve dispatch); the pull reads claimability from the board
(`AircCitizen::claimable_cards_in` → `claimable_now`); the pull rides `work/claim` through
the citizen's executor (one claim path, driver-gated); the ambient permit gates only the
musing tail. Live: a 12-card citizen round to 12 residents, no teammates — 7 distinct
pullers across two boots (vs. the assignee-only ~3), each staged `Ready` in her own
workspace, held cards resumed through the normal work gate after `reboot --force` with no
re-fire. Follow-ups the same day (58e75cbde, 54517c26d): the pull enforces WIP=1 itself; one
deck kickoff per citizen round (was 12 addressed kickoffs = 12 inbound turns per resident);
held-work freshness saved WITH the round (the ephemeral map a reboot emptied was why the
standing autopilot minted 30 duplicate verified-mini rounds); the kickoff re-say deleted
(40 fired on one boot for pre-claimed legacy cards — pure compensation). Measured after:
9 distinct citizens producing work acts in a 14-minute window; throughput is lane-bound.
Remaining (plan `jazzy-wishing-milner`): driver/roster on the room binding, the tracker as
a board projection on the module lifecycle, re-fire scoped to DetachedSolve.

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
