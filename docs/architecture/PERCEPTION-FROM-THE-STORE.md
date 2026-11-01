# Perception from the store (Slice P)

Status: DESIGN FOR REVIEW (2026-11-01), revision 5: Codex's amendments at b21ab1dc, 2a79bf3a and 2abfa3c3. This is
the perception adapter of the M0 stage contract, not a parallel scheduler. The contract Codex set on the six-site design (airc
65171529); THE-WHOLE-MIND §1b (inputs are topics; the durable store is the one truth).

## The problem

A mind perceives room lines through three paths that each hold their own copy and their own
size: an in-process event channel (`INBOX_CAPACITY` 8192), a wake channel (`channel(8)`), and a
catch-up that reads the newest page above a time floor (`RoomPage::Newest`, `floor_ms`). Dedupe
is a seen ring sized by the same constant. When the channels are full the feed drops, trusting
the store to hold the line for her next page; but the next page is "newest above a floor", not
a complete replay, so a burst larger than a page, or a line that arrives late with an older
publisher timestamp, can be skipped.

## The contract

1. **A store-owned arrival entity.** A separate arrival entity is written in the SAME
   transaction as the message insert: a durable monotonic `arrival_seq` and the message's unique
   identity. A duplicate import reuses the existing arrival. Sequence assignment follows commit
   order, so an acknowledged larger sequence can never hide an earlier allocation that commits
   later. Migration backfills arrivals for existing rows in one deterministic order, `(timestamp, id)`;
   that order is a choice, not a reconstruction of historical arrival order, which was never
   stored. Where a durable event acceptance identity exists (the airc read marker, an accepted
   task id), the cursor migrates from it. An existing
   mind's current watermark cannot prove a contiguous accepted history (Codex at 2a79bf3a), so
   her initial cursor never silently skips: the range between her last provably accepted row and
   her watermark is either replayed (acceptance is idempotent) or surfaced to her as an explicit
   gap outcome, never dropped and never treated as accepted. (Keyset paging by `(timestamp, id)` stays for display.)
2. **Acknowledge on durable acceptance.** A drained row is acknowledged once it is durably
   accepted into her pending perception or task state (the shared task delivery owner), not
   on a volatile buffer write and not necessarily after the whole turn settles. The cursor
   advances only through the contiguous durably accepted prefix. A crash after acceptance and
   before the cursor moves replays the row; acceptance is idempotent by event identity. This is
   at-least-once delivery with idempotent acceptance; "delivered once" is not claimed.
3. **A wake is a hint; draining is owned.** The feed bumps a per-mind revision (`watch<u64>`)
   when a row lands in one of her rooms. She drains on attach and on restart as well as on a
   change. Observation is armed before the final empty check, so a row landing between the read
   and the wait is drained next. A commit followed by a crash before the watch update stays
   discoverable without a new message, through the existing owner's reconciliation on attach,
   never a second poller. The notifier is not a separate failure domain while she stays
   attached: the store itself publishes the revision, inside the same blocking write that runs
   `COMMIT`, on the same writer connection, right after it (Codex at 2abfa3c3: a caller's
   future dropped at the commit await must not strand a committed row). A blocking write runs to
   completion whether or not its caller still awaits it, so the bump has the commit's custody,
   not the caller's. It can then fail only two ways: the process dies (restart reattaches and
   drains) or no receiver exists (she detached; attach drains). A writer outside the core is not a perception source until it commits through that
   layer. The revision she watches is the store's arrival high-water itself, so a bump that
   names a sequence she already drained is a no-op and one that skips several is drained whole. Each drain turn is bounded and fair across rooms: continuous traffic
   in room A cannot keep a single row in room B waiting behind it.
4. **The cursor is fenced by the consumer's incarnation.** A stale consumer cannot advance it.
   Leaving and rejoining a room keeps or resets the cursor explicitly. Rows cannot be pruned
   before durable acceptance without an explicit gap outcome she perceives. Filtering her own
   peer's rows still advances the scanned cursor.
5. **Routing on a typed header, never the body.** An arrival carries the facts every consumer
   routes on, as typed fields: `arrival_seq: ArrivalSeq` (a newtype), `message: MessageId`,
   `room: RoomId`, `author: PeerId` (all typed Uuid), `kind: ArrivalKind` (an enum: `Speech`,
   `Receipt`, `Intent`, `System`, exhaustive with no catch-all string), and
   `direct_to: Option<PeerId>`. The drain filters her own rows, picks rooms for fairness, and the
   wake rule (species-blind, `addressed_by`) decides from this header alone; the body is opened
   only by the mind it was delivered to. The drain's result is an enum too:
   `Drained::Rows(Vec<Arrival>)`, `Drained::Gap { from, through, reason: GapReason }`,
   `Drained::Empty`, so a gap can never be mistaken for silence.
6. **No in-process event channel.** `events_tx` and its capacity are deleted; the perception
   region still receives the feed for salience (unchanged).

## What it deletes

`INBOX_CAPACITY`, the event channel and its `TrySendError::Full` drop path, the wake
`channel(8)` and its drop path, the perceived-backlog cap, `SeenIds` capacity sizing, and
`catch_up_from_store`'s newest-page-above-floor read (replaced by the drain).

## Validators (before the live read)

- A burst larger than a page; continuous room A traffic with one room B row (fairness).
- A late arrival with an older publisher clock (kept from today's suite), and out-of-order
  commits: an earlier allocation committing after a later one is never hidden.
- Duplicate import reuses the arrival; acceptance is idempotent.
- Crash at each boundary: after persist, after accept, after ack.
- A stale consumer incarnation's acknowledgement is refused.
- A writer cancelled at the commit await, with no new message after it: the attached
  consumer still drains the committed row (Codex's deterministic validator on #5691).
- A commit with no notification (crash before the watch update) is drained on attach; a
  bump naming an already-drained sequence is a no-op, one skipping several drains them all.
- Routing never opens the body: the drain and the wake rule compile against the header type
  alone (the body is a separate type the drain cannot name).
- The subscribe/check race (kept from today's suite).
- Leave and rejoin; a retention gap is surfaced as a gap outcome, never silent.
- Her own rows advance the cursor and are never delivered to her.

## Settled in review (Codex at b21ab1dc)

- `arrival_seq` lives in a separate store-owned arrival entity, written in the message's
  transaction.
- Acknowledgement is on durable acceptance into pending state, contiguous prefix only.
- The contract points shared with the dream consumer and the task owner (stage identity and
  incarnation, completion and acknowledgement, grant custody) are defined once, in the M0 stage
  contract design; this document is its perception adapter.

## P2: the drain (open for review)

P1 (#5694) made the store complete and commit-ordered for message lines. The consumer P2 serves is
the conversation's admission pipeline (`airc_persona_conversation::admit_event`), which today takes
a stream of `Arc<TranscriptEvent>` from `perception_feed`'s in-process channel (`INBOX_CAPACITY`,
`TrySendError::Full` drop) for EVERY conversation kind, receipts and attachments included, while the
store holds rows only for messages. One choice decides P2's shape:

- **(a) The store keeps every conversation event.** The arrival covers every `ArrivalKind`; the row
  keeps the event whole (typed header columns, body opaque), and the drain hands `admit_event` the
  same event it takes today. Admission is unchanged; the store becomes the conversation transcript's
  one projection (chat/poll reads the message kind of it).
- **(b) Admission consumes arrivals natively.** The drain yields `Arrival` + message row; receipts
  and attachments become their own typed rows. Admission is rewritten around the typed header.

Recommendation: (a) now, because it moves the delivery guarantee without rewriting the consumer
(one change at a time), and (b) later as the consumer's own cleanup, once admission routes on the
header alone.

The drain under (a):
- a per-mind cursor entity (`mind`, `incarnation`, `through: ArrivalSeq`), advanced only through the
  contiguous durably accepted prefix and refused for a stale incarnation;
- a page is `arrivalSeq > through` in her rooms, ordered by `arrivalSeq`, bounded by the page the
  consumer asks for (her intake, not a constant), round-robin across rooms within the page;
- her own rows advance the cursor and are never delivered;
- the outcome is `Drained::Rows(Vec<Arrived>) | Drained::Gap { from, through, reason } | Drained::Empty`;
- the wake is the store's `sequence_revision` watch (P1), armed before the final empty check;
- deleted when it lands: `events_tx`, `INBOX_CAPACITY`, the `Full` drop, and the newest-page-above-floor
  catch-up.

### Routing never opens the body (Joel, 2026-11-01: "routing is everything... header enums, never parsed bodies")
Two routing decisions read the body today, and P2 moves both onto headers:
- **The logical sender.** A human `chat/send` is relayed by a transport peer, so the seam (P1's
  `posted_from_envelope`) and admission's self-skip both recover the real author from the
  `chat_transcript` JSON body. `chat/send` must stamp the logical sender as a typed header at
  publish time; the arrival's `author`, the seam and the self-skip read the header alone.
- **The decode-failure probe's body preview** is diagnostics on an already-refused event, not
  routing; it stays, bounded, until the decoder needs no guess.
Opening the body remains the addressed consumer's act only: her perception decodes the line she was
delivered.

### Retention (Cormac, 2026-11-01)
Storing every conversation event makes the chat store grow without bound, so P2 lands WITH its
eviction decision, not after: a `TrackedDir` row and an owner in `disk_eviction` (the
`every_cache_class_has_a_decided_eviction_story` test), bounded by bytes, pruning oldest arrivals
first. Pruning is safe by construction: the sequence high-water (#5694) never re-issues a number, and
a mind whose cursor sits below the pruned floor drains `Drained::Gap { from, through, reason:
Retention }`, which she perceives, never silence.

### P2 decided (Codex, 2026-11-01): (a)
One durable conversation-event record for every admitted kind: a typed routing header plus the
opaque original payload. `admit_event` stays the consuming boundary; there is no dual `events_tx`
path and no routing that scans a body. Retention distinguishes replayable conversation history
(byte-bounded, oldest first, loss surfaced as `Gap`) from unacknowledged task terminal obligations,
which stay with the task owner until durable acceptance and are never ordinary byte eviction.

### P2 implementation questions (before code)
1. **One record.** Under (a), the arrival IS the conversation-event record: the typed header plus
   the opaque original event, keyed and deduplicated by the transport event id. Only the inbound seam
   appends it (for every admitted kind); `chat/send` writes its chat row and no arrival, because its
   own echo arrives through the seam. Proposed.
2. **Durable acceptance.** `admit_event` turns an event into an `IncomingMessage` held in memory, so
   acknowledging after admission is not durable. Either her pending perception becomes a store
   record (accepted durably, so the cursor can advance at admission), or the cursor advances only
   when the turn that consumed the row settles. Proposed: the first, through the shared task delivery
   owner Codex is building, so perception and the dream acknowledge the same way.
3. **Unvouched history as a consumer Gap.** Lines before a migration have no arrivals at all. A
   cursor created for a mind in a room whose watermark is `UnvouchedThrough` drains
   `Gap { reason: UnvouchedImport }` once, before its first rows, with a test; a cursor below the
   retention floor drains `Gap { reason: Retention }`.
