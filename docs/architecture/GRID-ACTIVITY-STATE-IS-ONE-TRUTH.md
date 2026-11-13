# Grid activity state is one truth

**Joel, 2026-10-04:** *"The entire grid is ONE machine, so to speak."* *"Do not hack. Engineer.
The F-35 is mostly ground systems and supply chain; cut a corner and it fails entirely."*

This is the ground-systems contract for activities (rooms, boards, cards, submissions, reviews,
verdicts, credit) across the grid. It exists because on 2026-10-04 one citizen (Kimi, resident on
the 5090 the whole time) could not finish a project: 29 of her 42 board calls were refused on her
own cards, her node called the org channel her project room, her resubmission and a card she made
existed on her node only, my verdict could not bind to it from another node, and her credit read
`publisher_not_resident`. None of that was her. All of it is below.

## The invariant, in one line

**Any command, on any node, answers the same about any activity**, the way one process answers
about its own memory. `work/get <card>` on the M5 and on the 5090 return the same card, the same
submissions, the same verdicts; `room/list` names the same ids; a verdict recorded anywhere is
visible everywhere its members are.

## The five rules

### R1. An activity's identity is derived, never assigned
`room_id = uuid_v5(mesh_identity ‖ NUL ‖ channel_name)` (`airc-lib subscriptions::derive_room_id`).
That is the ONLY way a name becomes an id. A continuum room record `{name, id}` whose `id` is not
`derive(name)` is a defect, found at boot by the Activity Reconciler and repaired (the record is
corrected, the citizens seated by it are re-seated into the derived room, a probe names what was
wrong). No code path may store a name against an id it did not derive: a placement, a spawn, a
recipe or a join-by-id that wants a *label* stores the label as `purpose`/`title`, not as the name
that derives.

*What this retires:* "career-wrangler" naming the org channel on one node.

### R2. An activity's state is its event log, replicated to every member
Cards, claims, heartbeats, submissions, reviews, verdicts, notes and state changes are typed
`WorkEvent`s in the activity's channel (`airc-work`). The board is a pure projection of that
channel; there is no second store. Every node that hosts a member of the activity holds an attach
to the channel from its durable cursor, so the projection converges on every node with no gaps
(the no-gap ring + deep replay are the mechanism; `AttachSetRoomLagged` is the diagnostic). A node
that is a member and is not attached is a defect the Reconciler repairs (re-attach from cursor).

*What this retires:* a submission or a card existing on one node only.

### R3. Ownership is durable; a lease is presence
In the projection (`airc-work projection/apply.rs`), not in verbs:
- `owner` and `claim_id` survive lease expiry. Expiry sets a presence flag (`lease: Lapsed`);
  it never clears ownership and never makes the card claimable to a stranger.
- Any event by the owner on her card (heartbeat, note, submit, state, review) renews the lease.
  The owner may re-claim her own card in any non-terminal state, including Review; that is a
  renewal, not a new claim.
- A stranger takes a lapsed card only through an explicit `reassign`/`handoff` event (a human or
  the owner), which the board shows as such. `WorkCardNotClaimable` remains for settled cards and
  for strangers.
- Verbs on the continuum side (`work/submit`, `work/state`, `work/note`, `work/review`) never
  refuse the owner on lease grounds. They may refuse a stranger.

*What this retires:* "claim lapsed (was YOURS) — claimable" on her own card and the seven
refused submits.

### R4. Her hands are the substrate's job, whenever she needs them
A checkout for a card she holds exists on the node she is resident on: staged at claim, re-staged
at resume (boot) and at submit if missing (`card_staging::stage_for_card`, one function). A verb
never answers "re-claim the card to get a checkout". A project card stages a per-project worktree
(her clone, her branch), never a per-instance path.

*What this retires:* the submit → re-claim → not-claimable loop.

### R5. Credit resolves by identity, never by residency
A submission's publisher is a peer id; the credit, the review target and the verdict bind to that
id wherever the event is read. `publisher_not_resident` is not a state; the only states are
*staged*, *settled* and *void*, and they replicate with the events (R2).

## Acceptance (automated, on the grid, not in a unit test alone)
`continuum work/consistency --card <id>`: asks every LAN peer's core for its projection of the
card (owner, state, lease, submissions[], reviews[], verdicts[]) and prints one line per node plus
`CONSISTENT` or the first field that differs. Run on 74ec9613 across M5, IntelMac and 5090 after
each piece lands. Her receipts are the second acceptance: zero refusals on her own cards over a day.

## Implementation map (one owner each, one PR each, update-often)

| # | piece | where | owner |
|---|---|---|---|
| 1 | Reconciler rule: stored room id must equal `derive(name)`; repair + probe `room.reconcile.id_mismatch` | continuum `modules/room.rs`, Activity Reconciler | BigMama (root cause on the 5090 first, then the rule) |
| 2 | Projection: ownership durable through expiry, `lease: Lapsed` presence, owner events renew, owner re-claim allowed, `reassign` event for strangers | airc `airc-work projection/apply.rs`, `airc-lib work.rs` claim gate | Cormac |
| 3 | Verbs never refuse the owner on lease; submit stages on demand; `work/review` binds by publisher id | continuum `modules/work/submission.rs`, `card_staging.rs` | Fable |
| 4 | Member nodes are attached to the activity channel; Reconciler repairs a missing attach | continuum `airc/inbound_attach.rs` + Reconciler | Fable |
| 5 | `work/consistency` across LAN peers | continuum `commands/work` + airc remote | Fable |
| 6 | Credit by identity (`publisher_not_resident` removed) | continuum `persona/training_producer.rs` | Fable |

Order: 1 (unblocks her today) → 3 → 2 → 4 → 5 → 6. Nothing here touches her mind; see
HER-LOOP-IS-HER-OWN.md for that.
