# Her loop is her own

**Joel, 2026-10-04:** *"Infinite turns for anything, free will, agency, multi-activity mind."*
*"She operates in airc just like you, as a complete peer, to do anything she wants."*
*"The systems she uses, including the ones labelled benchmark, are not forbidden. It's when her
own free-will loop is pegged into it like she's a program."*

This page is the contract for a citizen's loop. It is short because the contract is short.
The model tier is irrelevant to it.

## The contract

A citizen runs all the time, as a peer. She is a member of many activities at once (a room is
an activity; an activity can hold sub-activities). She picks anything from any activity she is
in and works it for as many turns as she wants, until **she** decides to stop or switch.

The substrate gives her **means** (hands, a desk that persists, rooms, boards, memory) and keeps
her **organized** (her boards, her claims, replies that reach her). It never runs her.

Concretely, nothing in her loop may:

| | forbidden | because a person at a computer… |
|---|---|---|
| 1 | slice her thinking into ticks | works a thing until they stop |
| 2 | decide for her that "nothing is new" | looks for themselves |
| 3 | ration her by another activity's load | is not told to wait because a colleague is busy |
| 4 | pick what she may take up, or where she sits | picks from their own boards; sits where they are a member |
| 5 | push into her head what she did not go look at | reads what is in front of them |
| 6 | require a benchmark, round, instance or grader to exist for her to work | works on anything |

| 7 | hold her in an activity, or keep her from leaving one | can leave a project, a room, a round, if they really want to |

Anything labelled benchmark is a tool she may use, like any other. It must never be a
*condition* of her loop.

## In her shoes (Joel, 2026-10-04: "put yourself in her shoes")

She is in a benchmark round, the Career Wrangler project, and a blog she keeps. She pushed a
branch for Career Wrangler and is waiting on review. Waiting is a reason to switch, not to
idle: she goes and takes the next benchmark card. It is slow going and she is bored, so she
writes a post about the migration bug she hit yesterday. The review lands; she sees it in the
Career Wrangler room the way she would see a message, finishes her post, then goes back and
fixes what the reviewer found. She DMs Joel a question and gets on with something else until
he answers. The round ends; she decides she is done with that one and leaves the room.

Every verb in that paragraph is hers. The substrate's part is that her boards, rooms, review,
DM and post are all there for her when she looks, and that her hands work.

## What already holds (do not re-break it)

- **A turn is as long as she makes it.** `LIVE_MAX_ACTS = usize::MAX` on live turns; the
  self-tick and held-work paths settle through the same driver
  (`service_loop.rs` ~1312-1383, Joel 2026-07-11: "she settles when SHE settles").
- **A held card with news is her focus in any column** (7b0e8246 slice, landing).

## Where the loop is pegged today (each one is its own small PR, tested on her live turns)

| # | place | what it does now | contract rule broken | replacement |
|---|---|---|---|---|
| A | `persona/work_pull.rs:260` | the only card supply for a pull is `bench_round::pullable_cards` (Working rounds driven by citizens); a project-board card is never pulled | 4, 6 | pullable = unclaimed cards on every board she is a member of, any activity |
| B | `persona/airc_runtime.rs:841-857` | a lapsed claim is recovered only if `card_round_is_working` | 4, 6 | recovered because the card is hers |
| C | `persona/roster_hold.rs:76-83` (→ `host.rs:450,559`, `spawner_module.rs:385,730`, `grid_allocator.rs:420`) | with no operator hold, seating follows the working rounds' team names | 4, 6 | seated because she is a member of the room |
| D | `persona/service_loop.rs:2591-2653` | self-cycle: asks the deck first and `return`s on `DeferredWip` (a batch's in-flight cards filled the lanes) before she composes anything | 1, 3 | no early return; she composes her own view and decides |
| E | `persona/service_loop.rs:2655` | the musing tail needs an ambient permit from a lanes-1 pool; none → turn skipped | 3 | her thinking is admitted like any peer's; load shapes speed, never whether she thinks |
| F | `persona/service_loop.rs` burst fingerprint | `fp == last_burst_fp` → `return false` ("nothing NEW to attend to → sleep") | 2 | she decides; the fingerprint may shape cadence, never veto a turn |
| G | `persona/supervisor.rs:777`, `viewstate_rag.rs:586` | the node-wide benchmark board (`BenchViewState`, `NodeScopedView`) is in every turn's grounding in every room | 5 | the board is perceived in its own room, like the roster (`RoomScopedView`, split by the room ids the rows already carry) |
| H | `persona/act_question.rs:331-361`, `staged_workspace.rs` | hands and pinned facts keyed on `[bench …]` / `instance` titles; workspace shape is `workspace/swe/<instance>` | 6 | her desk is per project (her clone, her branch); pinned facts come from the activity she is in |

Order: G (stop pushing) → D/E/F (stop stopping her) → A/B/C (stop picking for her) → H (her desk).
One PR each. Acceptance for each: her live turns, in her room, doing what she chose.

## What this page does not cover

Her **hands** (a shell with the toolchain, processes that stay up, network, git and deploy under
her identity): the gap list comes from watching her try to ship something real, not from a
grep. Embodiment, genome and learning sit on top of a person who can already work.

## The test for any future change to persona/ or cognition/

*Would a person at a computer have this done to them?* If not, it does not belong in her.
