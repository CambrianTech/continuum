# The surface follows her activity's state

Status: slice 1 written (Fable, 2026-11-04, branch `fable/state-driven-surface`), reworked after
reviews by Cormac and Codex. Compile and live receipts are pending. It serves plan goal 10 (tools
are working sets) keyed by state, and order-of-work line 2 (her context is an epoch of keyed
items).

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
`apply_card_state_changed` (airc-work `projection/apply.rs:437`) accepts any state from any state.
Each verb checked its own preconditions inline in its handler, so the only way to learn which verb
applied was to call it and read the refusal.

## The rule: each verb's precondition is the one owner

Each work verb declares its PRECONDITION as a typed, pure predicate over the card and the actor
(`continuum-rooms/src/work_preconditions.rs`). The verb's handler calls it before it executes, and
everything that tells her what she can do calls the same function. There is no separate table of
moves, so what she is shown cannot drift from what executes.

| Verb | Precondition | Unmet (typed) |
|---|---|---|
| `work/claim` | not Review/Merged/Closed; nobody else's claim stands (a live lease, or a claim on record, which airc keeps its holder's while she is away, `apply.rs:285-300`); her own claim resumes; an unowned card in any working column is claimable | `Settled(state)`, `HeldBy(peer)` |
| `work/submit` | not Merged/Closed; not a review card; she holds it; a claim she names is the card's claim now; a stated revision is the card's revision now. **Review is allowed**: a revised candidate publishes there | `Ended`, `ReviewCard { parent }`, `HeldBy`, `NotHeld`, `StaleClaim { current }`, `StaleRevision { current }` |
| `work/review` | the card reviews something (the parent she names, if she names one); she holds it, unless she states the review claim (out of band) | `NotAReviewCard`, `ReviewsAnother { reviews }`, `HeldBy`, `NotHeld` |
| `work/state` | a holder never opens her own card; claimed/in_progress go through the claim path; her own live review returns to in_progress for a revision. Returns the route (`Direct`, `ThroughClaim`, `ResumeReview`) | `OpenByHolder`, the claim's |
| `work/heartbeat`, `work/release` | she holds it | `HeldBy`, `NotHeld` |
| `work/submission` | the card carries a submission | `NoSubmission` |
| `work/note` | none: any card on a board she stands in takes a ledger note | |

Holding a precondition can grant more than the verb itself (`WorkVerb::grants`). Submit grants
`code/**`, because an author edits her checkout. Review grants `code/**` and `work/submission`: a
reviewer reads the parent's submission and runs its tests in the checkout under review, whatever
the room's recipe declares.

**One function, two forms.** Discoverability calls a precondition with `Request::any(at_ms)` (her
own claim, no stated revision). Execution calls it with the arguments she passed: the claim she
names, the revision she saw, the state she asks for.

**Revalidated at the mutation boundary.** A handler decides against the card as the board holds it
at execution, never the snapshot she acted from. `work_preconditions::decide` turns an unmet
precondition into a `Refusal`. The refusal carries the typed reason, the card now (state, owner,
claim, revision) and her moves on it now. Cormac's typed refusal (card 9bba067f) reads this.

**Deterministic.** A precondition reads the time it is given (`at_ms`, the time the mutation's
event will record), never the wall clock. Lease expiry compares against that time, so replaying
recorded inputs replays the decision.

**Placement.** The predicates live in continuum-rooms, beside `card_facts` and `card_holder`, so
cognition (the surface) and mind (the handlers) both read them. Cormac moves them into airc-work
with the board's transition rules after this merges.

**Handler checks moved here** (before this slice):

- **submit:** the holder and not-claimed checks (`next_after_submission.rs:50-72`) and
  `holder_guard` (`latest_submission.rs:438-461`, deleted) now go through `submit`.
- **review:** "it reviews nothing", "held by another" and "not the named parent"
  (`next_after_submission.rs:493-516`) now go through `review`.
- **state:** the open-by-holder refusal and the review resume (`work_create.rs:203-235`;
  `resumes_held_review` moved from `work_list.rs:46`) now go through `state`.
- **claim:** the holder rule `live_holder` (`work_list.rs:31`, deleted) is now `holder_on_record`,
  and `work/claim` checks `claim` before publishing.

## Identical submissions are answered at submission identity

The same submission (same publisher, the same patch content hash, on the same base, for the same
instance) is answered as success, `outcome: AlreadySubmitted`, naming the existing submission and
what became of it. A changed patch, or the same patch on a new base, is a new submission and
publishes, in Review too. The loop Benchy ran ends with information instead of a refusal.

## The surface

The cycle reads her held work once per tick through the `held_work` extension slot, filled by
mind's `persona::held_work::read` from her own `active_claims`. It stores the result on
`Workspace::held`. For each held card, `Held::in_hand` holds the verbs whose precondition holds,
with what they grant. `ToolSurface::activity` (`tool_surface.rs`) is the room's declared verbs (or
her core hands) UNION every in-hands verb those commands and subtrees name, in canonical order. A
verb declared `OnDemand` joins while her card needs it; a `Withheld` verb never does.

`ToolsEpoch` records the commands its set was selected for and re-selects only when they change
(`delib.tools.epoch_moves_changed`). A change in what she has in hand is one new cached prefix.
Unchanged cards send the same bytes every act.

## Her state as keyed items

`HeldWorkStream` (`context_streams/held_work.rs`) is a native `continuum_context::Source` named
`held-work`. It renders one item per held card, keyed by the card.

Its OCCURRENCE follows the producer-revision contract the other sources keep (an act's `seq`, an
open view's `touched_seq`). It is made of the card, the state entry, and the rendered projection's
content:

- **State entry:** `Held::entered_ms` is the board's revision at the first read that saw the card
  in its current state. It holds through heartbeats and is new on each re-entry, so Review,
  InProgress, Review again is three occurrences.
- **Content:** the title, her stance among several claims, her submission or its refusal, and what
  she has in hand. A change to any of these appends once. A heartbeat changes none of them, so it
  appends nothing.

Ended and rejected claims are their own items. The pointer keeps "you can: ...", so a collapsed
card still says what she can do.

**Deleted:**
- `ActiveWorkSource` and its grounding registration;
- the `[InProgress]` text predicates (now typed reads of `HeldWork`);
- the `active-work` special cases in the legacy enrichment walk;
- the separate move table from the first draft (`card_moves`).

**Left dead, with an owner (Fable, next slice):** `burst_fingerprint` and `build_workspace_turns`
read an `active-work` delivery that `compose_for_turn` never carries in production. Next: fold her
held cards' states in from `persona::held_work`, and delete both branches.

**Finding for Cormac:** `card_holder::claimable_now` still advertises a lapsed claim by another as
takeable (#321). Since `TAKEOVER_ON_RECORD_SINCE_MS` the projection ignores that claim, so the board
over-advertises it. The claim precondition is the truth. `claimable_now` should call it when the
rules move into airc-work.

## Owners

Fable owns the surface, the state item and retrieval by state. Cormac owns the typed refusal with
next moves (card 9bba067f), repetition as perception on `persona.settle.repetition_observed` (card
e03c2aa0), and Shown ids plus their ratchet (card 484b000b). Codex owns engine speed.

## Ratchets

- Every verb, state and actor has a defined answer in canonical order. Every precondition rule has
  a case. Every verb's name, and every subtree a verb grants, is pinned to a registered command.
- The unowned-InProgress claim is proven through the real airc projection.
- A stale claim or revision is refused with the refreshed card and her moves now. A changed patch
  in Review submits; an identical one returns `AlreadySubmitted`.
- Through the real view: an unchanged card or a heartbeat appends nothing; a same-state refusal or
  a selection switch appends once; a re-entry is a new occurrence.
- Kept: `prompt_messages_outside_the_view`, `collapse_outside_the_view`, `hand_churn_labels`,
  `unwrap_justification`, `file_size`.

## Receipts owed

- Compiled and tested 2026-11-04/05 on the M5 (crate-scoped, low priority, serving up): lib tests
  for continuum-rooms 27, -cognition 912, -mind 1111, -context 43, -prompt 63, all passing; `cargo
  check --tests` clean for -core, -academy, -dreaming, -gateway, -grid, -serving. The ts-rs run
  regenerated `SubmitOutcome` and `WorkSubmitResult` identical to the hand edit.
- Landed with this slice: `work/list` lists every match (no default count), and a structured
  result goes on one record per line, so the one allocation shows a big board at a level (its
  first and last cards, the counts, the handle). The tool-result store writes through its disk
  class (`tool-output`), its handle is the content hash's address, and a read's default tail is
  the newest lines its budget holds. The work turn's stimulus no longer lists her cards (her
  held-work items do) or her progress receipts (her actions and open views carry every act across
  turns and restarts, `a_restart_keeps_every_act_as_she_read_it`).
- Her thoughts are a log like her actions (`working_memory::thought_log`: one record each, kept
  whole, checkpointed under its occurrence; an older checkpoint's ring thoughts move into it). The
  thoughts source reads every one, her newest whole and the older ones at the level her one
  allocation affords. So the work turn no longer pages her thoughts off the room store:
  `WORK_GATE_PAGE_ROWS`, the 4/400 keep-count and length, the scoped resume block and its
  older-thought head cut are deleted, and the write-less count reads her action log from the hold
  boundary (`WorkingMemory::operations_since`). The entry ring keeps receipts, facts and
  settlements; no thought rides it.
- Command-boundary tests through the real handlers on an airc fixture. The decisions are tested at
  the boundary function (`decide`), not yet through `WorkSubmit::run`.
- The live read after deploy (Fable, within the deploy's 15-minute window), on the M5, per
  resident: `commands/help`, `work/list` and refused `work/*` calls per hour, against the
  2026-11-04 baseline; `delib.tools.epoch_moves_changed` per card transition; prefix reuse
  unchanged or better.
