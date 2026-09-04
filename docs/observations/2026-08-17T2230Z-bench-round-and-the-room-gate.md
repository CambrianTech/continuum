# 2026-08-17 — two bench rounds, and the room gate that made them blind

Written after the rounds, per the standing instruction: a new dated observation file after
every run, and no reading of old ones as if they were current.

## What was run

| when (CDT) | what | receipt |
|---|---|---|
| 16:00 | `benchmark/dispatch --name=hard-rs` | 3 cards, 3 kickoffs, room `bench-hard-rs-1787000446` |
| 16:08 | `benchmark/dispatch --name=swe-bench-lite --limit=12` | 12 cards, 12 kickoffs, room `bench-swe-bench-lite-1787000921`, 2 astropy env errors |

Both were dispatched by me out of the order the plan file specifies (Phase 4 gated behind
Phases 0–2). The `hard-rs` one was also the wrong suite — Joel asked for SWE. **Neither
round produced a grade.** 15 cards remain on the board; the operator has no verb to recall
them (see "what the operator cannot do" below).

## What the rounds measured

**Serving:** ready, `lanes: 1`, window 36,608, Devstral-Small-2507 + a Qwen2.5-VL-7B lane.
24 personas hosted (`persona/roster` key is `citizens`, not `personas` — my first parse
reported 0 and I nearly filed a false #437).

**The fan-out.** 24 `persona.turn.start` rows in 30 minutes are **2 events**, not 24
workers:

| lamport | room | personas woken |
|---|---|---|
| …301506 | hard-rs | 5 |
| …301789 | swe-bench-lite | 19 |

All 24 carry the same `peer_id`. One message lands in a bench room and every member takes
a turn on it, all queued on one lane. 21:09:04 is 23 seconds after the dispatch.

**Acts in 90 minutes:** 28 rows — `code/shell` 4, `code/write` 4, `code/run` 3,
`work/claim` 3, `work/get` 2, `code/read` 2, `code/edit` 1, `work/list` 1,
`code/git/status` 1, `code/list` 1, `commands/help` 1. Arjun read a card and wrote a patch
in-room (`wrote=true`). So the loop actuates. `work.card` state-changes: **0**.

## The defect: room-scoped grounding abstains in a bench room (#443)

`rag.room_gate.abstain`, 90 minutes, `bound_room` → `turn_room`:

| source | bound | turn | n |
|---|---|---|---|
| roster | academy | swe-bench | 23 |
| room-doctrine | academy | swe-bench | 19 |
| roster | academy | hard-rs | 3 |
| room-doctrine | academy | hard-rs | 3 |

A citizen answering in a per-run bench room received the cards and **neither the peers nor
the room's operating rules**.

`RoomBoardSource` was already turn-parametric (#443's first half). Roster and doctrine were
not — they went through `rag_budget::room_scope_allows`, which abstains whenever
bound ≠ turn.

**The capability existed and was never called.** `airc_lib` has `room_roster_in`,
`room_roster_cards_in`, `room_doctrine_in`. `room_doctrine_in`'s own doc names the defect
verbatim: *"A citizen who belongs to several rooms answers a turn in the room it arrived
in; reading doctrine from her default instead grounds that answer in another room's
rules."* Continuum called the roomless variants.

Fixed in `2566a09d6`: both readers take `room: Option<Uuid>`, both sources resolve
`turn_room.or(bound)` exactly as the board source does, `None` keeps pre-#443 behaviour for
unstamped contexts, and the exam-bleed nil-room pin is now held by an executing test rather
than by the gate's side effect.

**STILL OPEN — the third instance.** `wall_source` has the same shape. airc has
`wall_posts_in(&Room, category)`, but it takes a full `Room` struct and airc exposes no
id→Room resolver (`current_room()` only), so wiring it needs an airc-side seam keyed by
`RoomId`. Not done. `viewstate_rag` also calls `room_scope_allows` (2 sites) and was not
examined.

## What the operator cannot do

`activity/archive` and every `work/*` verb refuse the substrate-local operator — *"activity
verbs act as the caller's own airc identity, and the substrate-local operator has none
in-core (the self-peer gap, task #27)"*. So `benchmark/dispatch` can CREATE a round from
the CLI and nothing can cancel one. That is #371's "a round has no END" with a sharper
edge: it has no **abort**. The 15 cards from today are there until their claims lapse.

## Also landed today

- `cabecb9c4` — one era-pinned uv install path. The dependency-sdist pre-install had no
  heal loop while the `-e .` install did, so every astropy instance died on a hint the code
  30 lines below knew how to parse. Same "one decision, several sites, correct at one"
  shape as #443.
- `4323ac158` (deployed earlier) — board render cap. **Live-verified across three samples
  and two citizens:** board share of the system prompt 65.4% → 17.4% / 29.1% / 26.7%,
  cards 90 → 8–12.

## Method errors I made today, for the record

1. Counted fan-out rows as workers ("20 personas working"). Two events.
2. Parsed `persona/roster` for a key that doesn't exist and read the empty result as
   "0 hosted" — the exact `[[an-absence-is-an-unfinished-measurement]]` failure, caught
   only because the probe stream contradicted it.
3. Dispatched two rounds before the phases they depend on, then tried to quiesce with
   sleep-mode — which would have treated a symptom, since those citizens were being woken,
   not choosing to burn the lane.
4. Ran the runbook's exact command (`benchmark/dispatch --name=<x> --limit=<N>`) only after
   being told to open the runbook, which is the first line of my own memory index.

## Not deployed

`cabecb9c4` and `2566a09d6` are committed on canary and compile clean; the running core is
`4323ac158`. Neither fix is live. The owed proof for #443 is a reboot followed by
re-reading `rag.room_gate.abstain` for bench-room `turn_room` values — the count should go
to zero for roster and doctrine, and wall should still show them until its seam lands.
