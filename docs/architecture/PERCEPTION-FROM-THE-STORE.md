# Perception from the store (Slice P)

Status: DESIGN FOR REVIEW (2026-11-01). The contract Codex set on the six-site design (airc
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

1. **One arrival order, owned by the store.** Each row the chat store persists gets a monotonic
   `arrival_seq` assigned at insert, under the store's write, never derived from the publisher's
   clock. `(arrival_seq)` is a total order of what this node received. (Keyset paging by
   `(timestamp, id)` stays for display, which is chronological by intent.)
2. **A replay cursor per mind, owned by the store.** `perception_cursor(mind, room) ->
   arrival_seq`, an ORM entity. A drain reads `arrival_seq > cursor` in bounded pages, ascending,
   until empty: complete across pages, bounded per page. The cursor advances only on
   acknowledgement, after the lines are in her perception (persist-before-wake is the store's
   insert; acknowledge-after-perceive is hers). A restart resumes from the acknowledged cursor.
3. **A wake is a dirty notification.** The feed bumps a per-mind revision (`watch<u64>`) when a
   row lands in one of her rooms. The loop drains whenever the revision moved since its last
   drain. A lost or coalesced wake loses nothing: the next drain reads everything past the
   cursor, in every room. Pending obligations live in the store and the cursor, not in the
   notification.
4. **Dedupe is membership within the drain.** A row is delivered once because the cursor
   passed it; the seen ring and its size go. Lines from her own peer are skipped by identity.
5. **No in-process event channel.** `events_tx` and its capacity are deleted; the perception
   region still receives the feed for salience (unchanged).

## What it deletes

`INBOX_CAPACITY`, the event channel and its `TrySendError::Full` drop path, the wake
`channel(8)` and its drop path, the perceived-backlog cap, `SeenIds` capacity sizing, and
`catch_up_from_store`'s newest-page-above-floor read (replaced by the drain).

## Validators (before the live read)

- A burst larger than a page: every row delivered once, in arrival order.
- A late arrival with an older publisher timestamp: delivered (arrival order, not publisher time).
- Persist-before-wake: a wake for a row not yet visible never advances the cursor past it.
- Lost and coalesced wakes: one wake for many rows across room A then room B delivers all.
- The subscribe/check race: a row landing between the drain's read and its wait is drained next.
- Restart before and after acknowledgement: no loss, no double delivery past the ack.
- Her own lines are never delivered to her.

## Open for review

- `arrival_seq`: a column on the chat message entity (migration) or a separate append-only
  arrival log entity keyed by message id? The column is simpler; the log keeps the message
  entity untouched.
- Acknowledgement granularity: per drained page, or per settled turn?
