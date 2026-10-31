# One board: a resident perceives the board a human sees, verdicts with their findings

**Owner:** Fable. **Status:** design, 2026-10-24. **Plan:** goal 2 (residents code at pace),
learning (goal 8: corrections must reach her as text), goal 7 (one budget).

## Why

Joel, 2026-10-24: residents must be capable peers that get better, and "using rag budgeting and
positron correctly too. No more hacks." Today the work board has two faces that never meet:

- A human's kanban widget renders `KanbanViewState`, projected by
  `continuum-core/src/ipc/positron_kanban_source.rs`, for the bootstrap room only, with no
  review on any card (`submission_reviews` is dropped at projection).
- A resident reads `continuum-mind/src/persona/room_board_source.rs` (848 lines), a separate airc
  read rendered as one text block: a `[board] … N claimable here` headline that changes as claims
  move (re-appended into her view whole, card 5b09111e), and per card only "Read work/submission
  … for all N signed verdicts". A review reaches her as `[board fact] review on card X: Failed
  (reviewer Y)` (`room_event.rs`), the finding dropped.

Measured on the M5 the same day (resident tool audit): Atlas and Aiko saw "Failed" with no finding;
the substance arrived only when a reviewer happened to restate it in chat. A correction she never
reads as text cannot become a lesson, and the academy has nothing to teach from.

## The shape

One truth (airc's board fold), ONE projection (positron `KanbanViewState`), every renderer reads
it: the human widget, and her context view through a stream adapter, priced by the one allocation.

- **The card carries its verdict.** `KanbanCardView.review: Option<KanbanReviewView>` — the latest
  review of the card's latest submission: outcome (a positron mirror of `WorkReviewOutcome`,
  mapped exhaustively at the projector like `map_card_state`), reviewer id + resolved name,
  reviewed-at, submission id, and `finding: Option<String>` = the review's `evidence_text`
  (inline on current reviews; a legacy hash-only review reads its blob once off the hot path,
  cached by hash, or says its finding is at `work/submission <id>`).
- **Every room a resident is in is projected**, written through the per-room substrates
  (`global_room_substrates()`, the `chat_projection::sink` pattern; `"kanban"` is already a
  `PER_ROOM_KINDS` entry), the focused room mirrored to the node substrate as today.
- **Her board is a context stream** (`context_streams/board.rs`) over the room's
  `KanbanViewState`: one item per card, keyed by card id, viewer-relative (her cards first,
  "yours" from `assignee_id == her`), no per-act count in any header. A card's item appends only
  when that card changes. Full = the card line with its verdict and finding; pointer = the card
  line with the outcome and `work/submission <id>` to expand. Required? No: the board is
  grounding, granted by the one allocation like every source; a verdict on HER card is the one
  part she must read, and rides as its own required item.
- **The event line names the finding too**, or is retired once the card carries it: one fact,
  one rendering.

`RoomBoardSource` keeps only `RoomBoardReader` (the held-cards fold and `work/list` read the board
through it); its rendering, headline and RagSource are deleted when the stream lands.

## Slices (each lands alone, with its validator)

1. **The verdict on the card** (positron + projector): `KanbanReviewView`, the exhaustive outcome
   mirror, `project_card` reading `submission_reviews`; the human widget shows it. Validator: a
   projector test over a board snapshot with a passed and a failed review, finding present;
   legacy review → handle, never empty.
2. **Every resident room projected**: the kanban projector writes per room through the sink;
   verify the `kanban:changed` cue for non-bootstrap rooms (unverified today). Validator: two
   rooms, two views, a change in one re-projects only it.
3. **Her board stream**: `context_streams/board.rs` over the per-room view, registered in place of
   `RoomBoardSource`'s RagSource; the `room-kanban` id kept for `board_spoke` and `PromptChurn`, or
   those three readers moved with it. Validator: replay her captures, appended chars per act from
   the board (today the whole block on every claim change) → one card line per changed card;
   every verdict on her card present with its finding.
4. **Delete**: `RoomBoardSource`'s rendering and RagSource, the `[board fact]` review rendering
   (or it names the finding), the "Read work/submission" guidance.

## What this is not

Not a new prompt block, not a string spliced into the faculty, not a second store of the board.
The academy later reads the same verdicts (a correction is a lesson) through the same view.
