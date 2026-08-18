# The grade tail: making the room grade itself

**Status:** plan of record. Written 2026-08-18 after a live session that recovered TWO
real SWE-bench Lite passes by hand and then found 11 more ungraded artifacts on disk.

**The objective in one line:** a citizen finishes a patch → a verdict exists, with no human
in the loop. Today the first half works and the second does not.

---

## ⚠ THE MOVE THAT LOOKS RIGHT AND IS FORBIDDEN

**Do not add a periodic sweep that scans for ungraded artifacts on a clock.**

`modules/benchmark_grade.rs` opens with the law:

> The whole system is event-based, never polling
> ([[the-whole-system-is-event-based-not-polling]]): nothing scans the board on a clock —
> the transition event fires the grade.

I proposed exactly that sweep during the session before reading the module. It is the
shortest path to "grades appear", it would work, and it would quietly convert an
event-driven substrate into a polling one. The module's own doc is what caught it.

**The correct question is never "when do we scan?" — it is "which EVENT did we fail to
emit?"** Both answers are below, and both are events that already have a home.

---

## What is actually broken (measured, 2026-08-18)

`benchmark/runs` showed 20 rows, all with `files_changed: []`, 15 `quiet`/stalled. On the
same disk, 13 staged trees held real in-place source edits. Two of them were PASSES
(astropy-14995, pytest-11143 — `resolved=true`, F2P 1/1, P2P 40/40) that had been sitting
ungraded for ~22 hours.

The grade-on-done subscriber is healthy and correct. It simply never fired, because it
triggers on a **card reaching a terminal state**, and these cards never got there:

| why the card never went terminal | the event that is missing |
|---|---|
| the solve process froze / was killed mid-flight | boot reconciliation already exists — it just doesn't grade |
| the claim's 30-min lease lapsed and nobody closed the card | **no lease-expiry event exists at all** |

Neither is a "we forgot to poll" problem. Both are "a real transition happened and nothing
announced it".

---

## Seam 1 — boot reconciliation grades what it reaps

`cognition::swe_bench::reap_orphaned_solve_runs_in(dir)` (swe_bench.rs:186) already runs at
boot and rewrites any run still marked `running` into a FAILED run. Test:
`a_run_still_marked_running_at_boot_is_journaled_as_killed`.

That is the event. It currently journals the death and stops.

**Change:** for each run it reaps, if that run's workspace carries a non-empty candidate
diff and no grade exists, grade it through the SAME `grade_swe` and write the verdict to
the run ledger. Boot owns reap-or-adopt for every service (#452,
[[boot-owns-the-process-tree-reap-or-adopt-never-fight-yourself]]); an orphaned ARTIFACT is
the same class of thing as an orphaned process.

Constraints:
- Reuse `benchmark::workspace_candidate_diff` — the ONE reading of her work. Never a second
  inline `git diff` (that drift already cost a credential leak; see
  `SOLUTION_PATH_EXCLUDES`).
- Grading is minutes per instance and boot must not block on it. Fire it as owned
  background work with the standard bounded-run discipline, not inline in the boot path.
- Idempotent: a run that already has a `.grade.json` is skipped.

## Seam 2 — a lapsed lease is a state change, so emit it

`grep` for a lease-expiry event in `modules/work.rs` returns **nothing**. Expiry today is
*passive*: `work/list` computes "is this hold still live?" at read time. So a claim can
lapse with a finished patch under it and the board experiences no transition — which is
why #451 ("lapsed claim + artifact → auto-close → the one grade tail") does not fire in
practice despite being marked complete.

**Change:** make expiry emit `work.card.state_changed` like every other transition, so the
EXISTING grade-on-done subscriber picks it up with no new grading path. One emitter
([[the-same-bug-at-two-sites-is-a-missing-constraint]]) — `work/state` already owns that
event (`WORK_CARD_STATE_CHANGED`); expiry must go through it rather than growing a parallel
notification.

Open question for Joel, deliberately not decided here: does a lapsed claim with an artifact
auto-CLOSE the card, or move it to a `needs-verdict` state that a citizen can re-claim?
Auto-close is simpler; re-claimable is truer to #419 (recover a claim whose work session
died). This is a recipe/lifecycle policy call, not a plumbing detail.

---

## Two tail gaps that must land with the above

1. **Workspace cards get truncated off the board.** `scan_run_cards` sorts by
   `last_activity_ms` and truncates to `limit`. A workspace card's timestamp is its
   directory mtime, so artifacts lose the recency race to chatty run files: the live board
   showed **1 of 13** at the default limit and all 13 only at `--limit=100`. An artifact
   awaiting a verdict must not be evictable by run-file noise — give artifacts their own
   floor in the projection, or sort terminal-pending ahead of quiet.

2. **A grade taken via `--workspace` writes no ledger entry.** That is how astropy-14995 and
   pytest-11143 were graded during the session, and it is why they *still* read `ungraded`
   afterwards. Every verdict — operator arm included — must land in the run ledger, or the
   board keeps re-offering work that is already judged.

Also noted, not blocking: the per-tree `git diff` in the artifact scan is genuinely
expensive (a hand-rolled equivalent over ~23 trees timed out at 5 minutes). The scan is
capped at 200 trees and warns when it drops any, but if the board is polled by a live
ViewState this wants a cheaper freshness check (dir mtime vs last-graded stamp) before
paying for a diff.

---

## Acceptance test (the doc's own standard, not a new one)

From `docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md`:

> can a citizen standing in the room perceive the run's state through the same ViewState
> pipe the human's screen uses? If answering needs a file read or a log parse, it is
> disconnected and it failed.

Concretely, this work is done when:

1. Kill a solve mid-flight with a real diff in its tree. Reboot. A verdict exists, and the
   board shows `resolved`/`failed` for that instance — with no operator command.
2. Let a claim lapse on a card whose workspace holds a patch. A verdict appears through the
   existing grade-on-done subscriber — no new grading path was added.
3. `benchmark/runs` at DEFAULT limit shows every artifact awaiting a verdict.
4. Grep the tree: still exactly one `grade_swe` and one `workspace_candidate_diff`. If a
   second appeared, the fix went in wrong.

## What is already true (don't rebuild it)

- `grade_swe` — one grader, fresh clone at `base_commit`, laundering-proof.
- `modules/benchmark_grade.rs` — grade-on-done subscriber, correct, already wired to
  `work.card.state_changed`.
- `workspace_candidate_diff` + `SOLUTION_PATH_EXCLUDES` — one reading of "her work", with
  the `.airc` credential exclusion.
- The board's workspace-artifact source (`scan_workspace_artifact_cards`) — this is what
  made the 13 visible; the plan above turns visibility into verdicts.
