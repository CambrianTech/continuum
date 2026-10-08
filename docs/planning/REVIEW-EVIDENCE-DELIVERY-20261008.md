# Review evidence delivery repair — 2026-10-08

Card: 9f1de8dd-d4dc-4489-8dd0-f84fe5f5e339. Owner: Codex / return_fix_review.

Observed consumer: Kimi submitted bcb3424e after responding to independent review of d0eae992. Her next work/review attempts tried to retrieve findings, but work/get returned no verdict and work/submission was withheld from implementation hands. Both publishers kept only a hash of reviewer words.

Before: CLI/native reviewer text became a hash-only reference. Author card reads exposed no review. After: AIRC signed review events optionally carry up to 32 KiB of exact text, validated against immutable size/hash. Old references remain valid with absent text. Projection-cache version changes rebuild events older readers had decoded without text. CLI and native publisher both use the same event contract. Continuum work/get (parent and linked review card), work/submission and publication receipts share WorkReviewResult conversion; two repeated projection constructors are removed.

Immediate normal-path recovery: the exact round-two reviewer text was added to c1177820's body through signed work update; verdict2801298c and Kimi's code/authorship were untouched. This does not retroactively add text to the old immutable verdict.

AIRC PR: https://github.com/CambrianTech/airc/pull/1558. Existing signed-review replay scenario passed, including legacy absence, content mismatch, bound rejection and persisted replay boundaries. CLI tests compile; pre-push fmt/clippy passed. Continuum's existing subscribed-card read fixture now exercises signed review text from parent and linked review card without changing focus.

Delivery gap: source validation is not installed proof. Await supported prebuilt install and running-revision verification, then observe normal author retrieval and response. No forced persona turns, live bucket changes, or private-store edits.
Read receipt: installed work/get(c1177820) at observed_at_ms1791476663199 returned body exactly equal to the reviewer evidence file (2215 characters). This verifies immediate card-body recovery, not deployment of the new signed-inline contract.

Validation: cargo test -p continuum-core --lib work_get_reads_subscribed_cards_without_changing_focus passed (1 test, 0.65s; build 6m52s). The WorkReviewResult binding export passed using that compiled test binary (1 test, 0.03s). Initial compilation identified three legacy fixture initializers needing evidence_text:None; fixed before the passing run.
