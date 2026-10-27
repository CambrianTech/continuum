# Submission review handoff

Ordinary submission publication currently tells its author to wait, while only
the finish transition creates a reviewer card. Publication must instead establish
the existing review handoff without changing the author's state or claim.

The shared AIRC work owner will materialize a review request for a precise accepted
submission on its parent's board. Its identity is stable for that room, parent,
and submission; retries retain the review's claim and lifecycle. The relationship
is typed, so a newer candidate cannot silently replace what the reviewer accepted.
The existing review validator remains responsible for reviewer identity, claim,
artifact, and verdict. Room policy and benchmark grading stay in their existing
Core owners.

Repeated legacy CardCreated events are structural replay errors, including on
rollback readers. The idempotent request therefore uses the existing review
extension envelope: older readers skip the unfamiliar extension without poisoning
their ordinary board; current readers materialize through the existing card
projection. This is not a second review queue or runner.

Core submit and finish share the handoff operation. A publication receipt survives
a failed handoff, with an explicit retry instruction using the same submission ID.
Read surfaces derive waiting/revise text from accepted verdicts and actual handoff
state, never from submission existence alone. Benchmark oracle behavior is unchanged.

Validation extends the real room/submission/review fixture: ordinary submission,
same-candidate retries, claimable review, failed verdict and revision; different
room/candidate refusal; concurrent request replay retaining reviewer custody;
legacy extension compatibility. This design is source work, not deployed proof.
