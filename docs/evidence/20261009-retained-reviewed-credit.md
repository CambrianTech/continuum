# Retained reviewed-credit boundary recovery

Card: `a346b3b0` (fable-astra). Source work only; no installed recovery claim.

Before: `CreditReviewAcceptance` survived a core restart, but only an accepted
review event/replay placed its immutable submission ID in DreamConsolidation's
memory queue. Losing that notification stranded the consolidation boundary;
forgetting the memory completion cache allowed an exact replay to queue it again.

After: the existing reviewed-credit ORM owns `ProcessedReviewBoundary`, keyed to
the accepted submission, with the same persona, decision and review IDs. The
existing DreamConsolidation permit reads at most sixteen retained IDs per tick,
rotates its cursor, and acknowledges a processed boundary durably before removing
the pending ID. Event replay checks the same receipt. No separate scheduler,
grader, training submission, model binding or inference path was added.

Recovery is bounded within the existing governor tick: two seconds for recovery,
with progress within a one-second row page, and two seconds for completion I/O.
Schema initialization occurs once per resident recovery state. Malformed or slow
rows remain durable, emit a diagnostic, and retry after cursor wrap without
starving later valid IDs. Storage failure does not disable ordinary dreaming or
memory decay. An unsuccessful completion write retains pending work.

The receipt proves completion of eligible episode consolidation, including an
empty eligible set. It does **not** prove training, weight changes or adoption.
A crash after semantic admission but before the completion receipt remains an
at-least-once retry window: these two existing persistence owners do not share a
transaction. This change does not claim exactly-once semantic admission.

Existing regression scenarios extended:

- Real ORM review acceptance, lost in-memory notification, store reopen and dream
  recovery without another review event or destination submission.
- Durable completion, another owner restart and exact-review replay suppression.
- An invalid earlier acceptance cannot hide a later valid acceptance; another
  persona cannot complete its boundary.
- Unavailable credit storage retains a pending request while ordinary decay runs.

Validation: existing real-ORM restart/replay regression passed (1/1), existing
dream-consolidation suite passed (18/18), and ORM entity registration passed
(1/1), on the compiled Windows lib-test target. Independent corrected-source
review: retirement_review APPROVE. git diff --check and formatting of changed
lines pass. Workspace fmt finds unrelated baseline drift; no broad reformat was
applied. Broad Clippy (--lib --tests) fails in unchanged bin/continuum.rs:3242
(clippy::never_loop); library-only check tracked separately. No deployment.
Library-only Clippy completed successfully (existing warnings remain).
