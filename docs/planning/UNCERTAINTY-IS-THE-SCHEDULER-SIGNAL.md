# Uncertainty Is the Scheduler Signal

**Status:** design, 2026-09-12. Source: Joel's exchange with Casey Hughlett (an MHH-style
cognitive-affective layer, and an essay on adaptive compute) read against what actually ran on
the M5 that day: the first Rust round graded end to end, and a benchmark card taken by an agent
through `uu` verbs alone.

**Owner docs this extends:** [THE-MIND-AND-THE-BEING](../architecture/THE-MIND-AND-THE-BEING.md)
(the mind is genome + memory + identity, not a model),
[BEING-SOCIETY-GOVERNOR](../architecture/BEING-SOCIETY-GOVERNOR.md) (the governor negotiates
capacity), [INFERENCE-LANES-REALISTIC](../architecture/INFERENCE-LANES-REALISTIC.md) (one base,
N lanes), [GENOME-FOUNDRY-SENTINEL](../architecture/GENOME-FOUNDRY-SENTINEL.md) (paging, demand-aligned
recall), [FOLLOW-THE-SIGNAL-THE-COMPRESSION-LADDER](../architecture/FOLLOW-THE-SIGNAL-THE-COMPRESSION-LADDER.md)
(dreams consolidate), [ACTIVITY-RECONCILER](../architecture/ACTIVITY-RECONCILER.md) (the board is the
saved state).

## The claim

A mind has one scalar the substrate never reads today: **how likely its current belief is wrong.**
Joel's framing (to Casey): uncertainty is what moves a mind across tiers while the mind stays
intact, it is what keeps misfit hardware from spending too much or too little power on a thought,
and it is the scalar that should enter the loss when a citizen trains. Casey's framing: compute
should be allocated by what the current uncertainty requires, not by token or by prompt.

We already have the room-scale version of Casey's adaptive computation graph: lanes, the governor,
engram paging, a per-act thinking budget, a team round with a review gate. What is missing is the
signal that should drive all of it. This document says where the signal comes from, what reads it,
and what it costs to get wrong. Nothing here is a new runner or a new manager; every consumer is an
existing owner gaining one input.

## Where the signal comes from

Three sources, cheapest first, each already observable at a seam we own:

1. **The model's own token-level surprise** on its plan and its edits (mean and tail logprob over
   the act's tool-call arguments). Free; noisy; available on every local lane. Not available for a
   remote lane unless the responder ships it (a wire field on `ai/generate`).
2. **Disagreement between the act and the world.** The verdict harness, the compiler, the test
   run, the reviewer's send-back. Expensive; exact. This is the ground truth the cheap sources are
   calibrated against.
3. **The evidence ledger's open questions** (Casey's MHH layer, §5-6): a card whose leading
   hypothesis has no discriminating observation yet is uncertain by construction, whatever the
   model says. This is a recipe artifact on the run room, readable by every resident.

The number that matters is not raw uncertainty but **calibration**: was the confidence earned? Every
graded card answers that for free. A citizen who was confident and wrong, or unsure and right, is
the training example.

## What reads it (each an existing owner, one new input)

| Consumer | Today | With the signal | Cost of getting it wrong |
|---|---|---|---|
| Act budget (`cognition/act` reasoning cap, #3831) | fixed cap per act | branch / retrieve / verify chosen by uncertainty; a confident act spends less | a low cap faults every act (IntelMac, #3824); a high cap narrates |
| Tier escalation | none; the tier is roster-wide | low certainty on a hard step borrows a bigger tier's lane **for one turn**, over the grid (the remote-lane shape, #3800); the mind, memory and genome do not move | a roster-wide switch starved twelve minds on three lanes (2026-09-06) — escalation is a lane a citizen takes, never a fleet decision |
| Governor / write-or-release | releases at N write-less acts | releases sooner when uncertainty is flat and rising; extends when it is falling | a fixed N released Joaquin one act before his fix twice |
| Engram paging / eviction | recency and cluster | Casey's value: P(needed later) × value − cost; "needed later" is high for the open questions of a held card | evicting the ledger of a held card recreates the orientation tax every turn |
| Team round / review gate | reviewer picks by availability | a card's competing hypotheses become the deck; the reviewer is the falsifier of the owner's leading hypothesis; seat the reviewer who was right on this class before | a reviewer who shares the owner's blind spot is a second owner |
| Dreams | consolidate by cluster | consolidate the evidence ledger of finished cards: hypotheses that survived, tests that discriminated, the calibration error per class | dreaming the narration instead of the decision structure |
| Curriculum / genome training | buckets by domain × role × outcome | add the calibration axis; the loss carries a calibration term (Brier over the act's stated confidence vs the verdict), and examples are selected by information value (surprise), not volume | training on confident wrong answers teaches confident wrong answers |

## Multi-context minds: the ledger is the room's saved state

A mind standing in several activities re-enters each one cold today: "let me re-establish ground
truth" opened most of the day's work turns (78 acts on cards that read Open). The ACTIVITY-RECONCILER
rule already says the board is the saved state of an activity. Extend it one level: **the card's
evidence ledger is the saved state of a thought** — observations, competing hypotheses with their
discriminating tests, the load-bearing unknown, the last action and what it was expected to show. It
is a wall record on the run room, written by whoever holds the card, read first by whoever holds it
next, the same across a reboot, a hand-off, a tier change, or a grid hop. Re-entry becomes "read the
ledger, run the pending test" instead of re-orientation. That is the scaling efficiency: the mind's
context per activity is the ledger, not the transcript.

This is also what makes a team a team rather than N solo coders in one room: two holders on sibling
cards share a ledger's discriminating tests; a reviewer reads the owner's ledger, not her diff.

## What we do not do

- No emotional narration. The MHH layer is a recipe, off by default, invoked by a card's recipe
  step; its functional states never appear in prose.
- No multi-horizon world-model training and no new attention variant. Those are foundation-model
  bets; ours is the substrate above the model.
- No fourth budget. The act budget, the governor, the tier borrow and the pager each keep one owner;
  the signal is an input to each, not a coordinator across them.

## Build order (one in flight at a time, each with a receipt)

1. **Measure before wiring.** Emit the act's mean/tail logprob and the ledger's open-question count
   as probe fields on every work act; join them to verdicts for one seeded round. Receipt: a
   calibration curve per citizen. If the cheap signal has no predictive value, stop here.
2. **The ledger as a recipe step.** A card's first work turn writes the ledger; every later turn
   reads it first; the review gate reads it. Receipt: acts-to-first-edit on the same seeds.
3. **Uncertainty-driven act budget.** Branch / retrieve / verify per act. Receipt: verified resolves
   per inference token, the metric Casey names, replacing pass rate as the round's headline.
4. **Tier borrow per turn.** One remote lane on a bigger tier for a low-certainty step. Receipt:
   the 5090's 27B taking a step for an M5 citizen without the M5 roster changing.
5. **Calibration into the loss.** The curriculum's buckets gain the calibration axis; the trainer
   adds the Brier term. Receipt: the calibration curve moves on the next seeds under the same
   harness.
