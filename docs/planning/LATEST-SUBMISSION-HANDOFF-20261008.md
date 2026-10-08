# Latest submission continuity — 2026-10-08

Card f428279d-06f1-4104-8c06-1f9bc67567e1. Owner: Codex retirement_review.

Installed 8169 Kimi trace cycle2 retained a handoff naming e7758574 as latest, while normal work/get and the board showed bcb3424e. Her current corrected source was a separate, not-yet-submitted candidate. The discrepancy is substrate evidence, not proof of a model competence failure. Raw observation: team-proof-20260921/20261008-kimi-review-trace-1915.json.

AIRC WorkCard documents submissions newest accepted transcript event first; projection inserts accepted submissions at index0. Handoff incorrectly used last(). The review gate used first(), while native work/review's default candidate used the largest publisher timestamp, allowing clock skew to disagree with both.

The existing work submission owner now provides latest_submission. Handoff, review gate and default native review selection all use it. Explicit submission IDs still select that exact candidate. Three competing position/time assumptions are replaced by one transcript-order contract. No room subscription, persona identity, scheduling, inference, event, or cancellation behavior changes.

The existing wake-line fixture retains claim/lease verification and adds empty-card plus two-submission coverage: the newest accepted event deliberately has an older timestamp, and only its ID may be named latest. No additional fixture or gate is added. Independent source review by cleanup_fix approved the change. All seven existing cognition::handoff tests passed on Windows (0.04s execution, 8m28s compilation), using the shared Cargo cache and retained dependency-supported WebRTC package. Current canary recovery changes were merged cleanly before validation. No installed correction or recovery is claimed.