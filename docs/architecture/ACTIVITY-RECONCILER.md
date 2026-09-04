# The Activity Reconciler — startup and resume without a hand

*2026-09-05. Joel: "Startups and resume still really disorganized and falls apart without
intervention. What is missing from our designs WITHOUT losing organic behavior?"*

## The diagnosis

An activity (a benchmark round is one) has no owner for its consistency. Its truth is
spread across the round tracker file, the board's cards and claims, the run room's
membership, the staged checkouts, and each citizen's process memory. Nothing reconciles
them, so every seam (a reboot, a pause, a lapsed claim, a re-dispatch) needs a human to
repair the world by hand. Measured on 2026-09-04/05 in one team round:

| Seam | What broke | Hand that fixed it |
|---|---|---|
| reboot | run room kept ONE member for 30 min; ten citizens held nothing and pulled nothing | `benchmark/dispatch --room <run room> --limit 0` re-seated the roster |
| pause | old round's twelve held cards blocked every pull (WIP=1) | `work/state open` ×10, once the verb could follow a card to its room |
| lapse | claims lapsed under working holders; 32 pulls / 32 re-stagings in 15 min | none possible by hand — fixed in the pump |
| dispatch | cards claimed faster than lanes could serve them | reopen seven cards by hand |

The organic loop (pull → work → done → review) was never the problem. The world under it
was inconsistent, and nobody owned repairing it.

## The rules (in build order)

1. **A reconciler per activity, not a resume script.** Desired state = the recipe + the
   board (cards, roles, "one measured round at a time"). An idempotent desired-vs-actual
   pass runs on a cadence — the hosting reconciler already does this for lanes — and repairs
   the WORLD only: stages a missing checkout, lapses a dead claim, re-seats a resident,
   re-publishes an unreceipted doctrine, pauses a competing round. It never tells a citizen
   what to think, so behaviour stays organic.
   *Shipped:* `reseat_working_rounds` on every resume pass (`modules/benchmark_resume.rs`);
   re-fire is the detached driver's compensation only.
2. **Liveness tied to acts, not timers.** A claim's heartbeat is her loop being alive and
   thinking — a work turn, a self-cycle, a lane wait — never "a room message within one
   lease". A working citizen keeps her card; a stuck one loses it; she or a peer pulls it
   again.
   *Shipped:* the cognition pulse counts self-cycles; renewals every `TTL/6`; the pump
   re-claims her own lapsed hold when she holds nothing live (`persona.claim.recovered`).
3. **Perception continuity instead of process continuity.** Where she stands and what she
   was doing is substrate state, not process memory: her own newest thoughts on the held
   card lead the work turn; receipts carry the acting root and only same-root receipts are
   perceived; the workspace map renders the acting root.
   *Shipped:* `persona/work_burst.rs`, `WorkingMemory::set_scope`, `acting_root_of`.
4. **Receipts for every signal.** Heard-by was the first. Kickoffs, doctrine, directed
   lines, review requests each get a receipt, and the reconciler re-sends until receipted.
5. **One admission predicate per consumer.** A wake signal is derived from the consumer's
   own admission function (`directed_pending` signals only lines that decode as a turn).
6. **Readiness gates for the room plane.** Lanes have `await_ready`; rooms need the same:
   subscribe, prime, seat — then serve.
7. **Every operator verb addressable by id.** `work/state`, `work/release`, `work/claim`
   follow a card to its room, node-wide through the round tracker. Any resume that needs a
   human to `room/join` first is a design defect.

8. **Planes, not deserialization.** A citizen's line carries its class at its head — `💭`
   thought, `⚙` act receipt (`persona/presence_glyph.rs`, the one vocabulary) — and every reader
   sniffs the class before it reads the body. While she holds work: her digest carries the
   MESSAGE plane only (human/agent lines, citizens' real speech; no presence lines, hers
   included), a line is a wake only if a human wrote it or it names her (agent status traffic is
   perception, not a trigger), and her receipts radiate into the held card's run room, not the
   room whose line triggered the turn. Measured 2026-09-04 before the cut: ten holders, 8 work
   turns an hour — every agent status line in #academy woke all twelve for a 265 s message turn.
   *Shipped:* PR 3698 (`airc_source::collapse_work_receipts(working)`, `turn_is_directed`,
   `acting_card_of` → `room_for_card`). Next: presence-class traffic on its own channel class so
   the message plane never carries it.

## Concurrency the reconciler enforces

- **WIP = lanes, board-true.** The roster holds no more claimed/in-progress cards than the
  server has lanes (`persona/work_pull.rs`). Review cards do not count: a `done` frees the
  slot its review needs. Idle citizens take no lane for ambient deliberation while the pull
  is deferred — they watch the board.
- **One measured round at a time.** Arms starve each other at WIP = lanes; the recipe
  declares concurrency and the reconciler pauses competitors.

## How to verify (the numbers, not the build)

- Core startup to "citizens hearing": an operator line sent six minutes after boot is
  admitted by every citizen (first at ~8 s). Probes: `persona.inbound.raw_event`,
  `persona.turn.start`, `persona.inbound.catch_up_rooms`.
- A round resumes with zero hands after a reboot: `bench.round.reseated` (or silence when
  everyone is already seated), no `bench.round.pulled` storm, no `persona.claim.recovered`
  storm.
- Diffs per checkout over time — the only number that is a solve.
