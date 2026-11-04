# The surface follows her activity's state

Status: slice 1 written (Fable, 2026-11-04, branch `fable/state-driven-surface`); compile and
live receipts pending. It serves plan goal 10 (tools are working sets) keyed by state, and
order-of-work line 2 (her context is an epoch of keyed items).

## The failure class

The substrate knew where a resident stood and did not show her. She learned the protocol by
trial, one act per guess, at 2-5 minutes an act on the M5. Receipts from 2026-11-04:

- **Benchy** held REVIEW card `32d53734` in Claimed. She called `commands/help` 4 times and
  `work/submit` 5 times ("nothing to submit"). `work/review`, the only verb her card wanted, was
  not on her surface: the room's recipe narrowed her hands and the verdict verb was one
  `commands/help` away.
- **Atlas** called `work/list` 6 times to find where he stood. His held card rendered as one
  line inside an `[active-work]` grounding blob that re-rendered whenever any card changed.

Under both sits one gap. airc-work has states (`CardState`) and no transition table:
`apply_card_state_changed` (airc-work `projection/apply.rs:437`) accepts any state from any
state. Each verb checks its own preconditions, so the only way to learn which verb applies was to
call it and read the refusal.

## The five pieces and their owners

| # | Piece | Owner | Where |
|---|---|---|---|
| 1 | One owner of "what moves a holder has in each state" (the transition table) | Fable | `continuum-rooms/src/card_moves.rs` |
| 2 | The surface: the room's declared set UNION the held moves of every card she holds | Fable | `continuum-cognition/src/cognition/tool_surface.rs` (`ToolSurface::activity`) |
| 3 | Her state as keyed items: one item per held card, its occurrence named by (card, state) | Fable | `continuum-cognition/src/cognition/context_streams/held_work.rs` |
| 4 | A refusal names her next moves as typed data (card 9bba067f); repetition as perception on `persona.settle.repetition_observed` (card e03c2aa0); Shown ids everywhere plus their ratchet (card 484b000b) | Cormac | command executor refusal rendering, settle, id parsing |
| 5 | Engine speed, so a wrong guess costs seconds rather than minutes | Codex | serving |

Pieces 1-3 land in this slice. Piece 4 is Cormac's and stays out of this slice: no edit here
touches the executor's refusal rendering or id parsing. Retrieval by state (paging a card's
ledger and submission into her view when its state asks for them) is Fable's next slice.

## 1. The transition table

The table is data: `card_moves::moves_in(CardRole, CardState, Stance) -> Moves`, pure, matching
all three enums with no wildcard, so a new state, role or stance is a compile error.
`moves_for(card, actor)` reads the role from the card's typed link (`reviews: Some(parent)` is
`CardRole::Review { parent }`) and the stance from its owner (`Holder`, `Unheld`,
`HeldBy(peer)`). The result is `Moves::Some(MoveSet) | Moves::None(NoMove)`. `MoveSet` is
non-empty by construction, so a bare empty list is unrepresentable, and a refusal always carries
moves or a typed reason (`NoMove::Held { by }`, `NoMove::Ended(state)`): the shape Cormac's refusal
ratchet reads. Her surface reads the same function through `Held` (her own cards). There is one
table and no holder-only copy. `Move::command()` is the only place these command names are
spelled. A test in `continuum-mind/src/work/tests` pins each name to the command's own
`ActionCommand::NAME`, and pins `code/**` to a registered `code/` command (`code/run`).

| Stance, role, state | Moves |
|---|---|
| holder, code, Claimed / InProgress | `code/**`, `work/submit`, `work/note`, `work/state`, `work/release`, `work/heartbeat` |
| holder, code, Blocked | `work/note`, `work/state`, `work/release` |
| holder, code, Review | `work/submission`, `work/note`, `work/state` |
| holder, review, Claimed / InProgress | `work/submission`, `work/review`, `work/note`, `work/release`, `work/heartbeat` |
| holder, review, Blocked | `work/note`, `work/state`, `work/release` |
| holder, review, Review | `work/submission`, `work/note` |
| anyone, Open | `work/claim` (an owner left on an Open card is stale) |
| anyone, Merged / Closed | none: `Ended(state)` |
| another holds it, Claimed / InProgress / Blocked | none: `Held { by }` |
| nobody holds it, Claimed / InProgress / Blocked | `work/claim` |
| not hers, Review | none: `Ended(Review)` until its verdict |

Where the verbs differ from a first guess (each row was read off its verb in
`continuum-mind/src/work/`):

- **A code card in Review has `work/state`.** `work/state in_progress` resumes the owner's own
  review for a revision (`resumes_held_review`), which brings her hands and `work/submit` back.
  Without it she has no way out of Review.
- **`work/submit` is not offered in Review, although the verb accepts it.** airc-work's
  `validate_for_card` refuses only Merged and Closed, so a corrected candidate can be published
  in Review. Without a change, though, the submit is refused as the same patch, which is the loop
  Benchy ran. Revising goes through `work/state` instead.
- **`work/heartbeat` is not offered in Blocked.** The runtime's heartbeat pump renews her live
  claims on the presence cadence, so a blocked card keeps its lease without the verb.
- **`work/submission` reads the parent's submission for a reviewer.** The submission lives on the
  parent card, so the reviewer's item names the parent's shown id.

**Follow-up (Fable, after this slice merges):** move the table down into airc-work as the board's
own transition table. Then `apply_card_state_changed` refuses a transition no row allows, and
every board client reads one owner.

## 2. The surface

The cycle reads her held work once per tick through the `held_work` extension slot, which mind
fills from `persona::held_work::read`, her own `active_claims`. It carries the result on the
workspace as `Workspace::held` (`HeldWork`). `ToolSurface::activity(declared, held)` computes:

- the room's declared verbs with every `Always` verb, else her core hands;
- plus every verb in her hands that a held move names (`code/**` selects the subtree), whatever
  the verb declares about her prompt. An `OnDemand` verb is in hand while her card's state needs
  it; a `Withheld` verb never joins;
- in one canonical order, priced once in her claim (`claim_tokens`, `claim_with_head`), and
  selected by `select_granted`.

`ToolsEpoch` holds the selected set across her view's turnovers. It now also records the moves it
was selected for, and drops the held set when they change (`delib.tools.epoch_moves_changed`).
A card's transition is one new epoch of her hands, one cached prefix. A transition that keeps her
moves (Claimed to InProgress) changes nothing, and an unchanged board sends the same bytes every
act.

## 3. Her state as keyed items

`HeldWorkStream` is a native `continuum_context::Source`, named `held-work`, at grounding
priority, ordered after the settled blocks (where `active-work` used to sort). It produces:

- one item per held card. Its key is the card; its occurrence is (card, state), so the item
  changes only on a transition. Whole: `[my work] card <shown> "<title>": <your work | you
  review card <shown parent>>, <state>`, then the one submission fact she needs (the parent's
  submission for a reviewer, her latest submission, or why the board refused her last one),
  then `you can: <moves>`. Pointer: the card, its state and its moves, so a collapsed card
  still says what she can do;
- an ended claim (lease lapsed or released), keyed `ended:<card>`, delivered on the one read
  that sees it end. A degraded read never reports one;
- a rejected claim, from the `claim_rejections` ring.

Ids render through `airc_core::shown_id::shown_form`. The faculty's working-presence and
lane-gate predicates now read the typed `HeldWork` (`holds_in_progress`,
`holds_a_working_card`) instead of grepping `[InProgress]` out of rendered text.

**Deleted:** `ActiveWorkSource` (the RagSource), its grounding registration, the
`faculty_ids::ACTIVE_WORK` id with its two text-grepping predicates, and the `active-work`
special cases in the legacy enrichment walk (`prompt/view.rs`, `prompt/system.rs`).
`AircWorkReader` moved to `persona::held_work`. Prompt text that pointed at `[active-work]`
now points at `[my work]`.

**Left dead, with an owner (Fable, next slice):** `burst_fingerprint` and
`build_workspace_turns` still read an `active-work` delivery that `compose_for_turn` never
carried in production (it binds only airc and roster). The next slice folds her held cards'
states into the fingerprint from `persona::held_work` and deletes both branches with their tests.

## Ratchets each piece adds

- **Table:** exhaustive matches (compile time). A test walks every role and state for a
  defined, canonically ordered row. Another pins every move's name to a registered command.
- **Surface:** a held review card's verdict verb is in hand in every room, with the claim priced
  as sent. The bytes are unchanged while her moves are, and change once when they do.
- **Item:** an unchanged state is the same occurrence with the same bytes; a transition is a new
  occurrence under the same key; the pointer keeps her moves.
- **Existing ratchets kept:** `prompt_messages_outside_the_view`, `collapse_outside_the_view`
  and `hand_churn_labels` (the item is a Source; nothing is pushed by hand), plus
  `unwrap_justification` and `file_size` (no touched file passes 1,000 lines).

## Receipts owed

- `cargo check` and the touched tests on a non-serving executor. This slice was written with
  serving up and has not been compiled.
- The live read after deploy (owner Fable, within the 15-minute window of the deploy that
  carries it), on the M5: for each resident, `commands/help`, `work/list` and refused `work/*`
  calls per hour, against the 2026-11-04 baseline above; `delib.tools.epoch_moves_changed`
  counted against card transitions (one per transition that changes moves, none otherwise); and
  prefix reuse unchanged or better.
