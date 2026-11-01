# Perception from the store (Slice P)

Status: DESIGN FOR REVIEW (2026-11-01), revision 2 with Codex's amendments at b21ab1dc. This is
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
   later. Migration backfills arrivals for existing rows in their stored order. An existing
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
   never a second poller. Each drain turn is bounded and fair across rooms: continuous traffic
   in room A cannot keep a single row in room B waiting behind it.
4. **The cursor is fenced by the consumer's incarnation.** A stale consumer cannot advance it.
   Leaving and rejoining a room keeps or resets the cursor explicitly. Rows cannot be pruned
   before durable acceptance without an explicit gap outcome she perceives. Filtering her own
   peer's rows still advances the scanned cursor.
5. **No in-process event channel.** `events_tx` and its capacity are deleted; the perception
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
- A commit with no notification (crash before the watch update) is drained on attach.
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
