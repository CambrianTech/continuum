# The grade tail: making the room grade itself

**Status:** plan of record. Written 2026-08-18 after a live session that recovered TWO
real SWE-bench Lite passes by hand and then found 11 more ungraded artifacts on disk.

**The objective in one line:** a citizen finishes a patch → a verdict exists, with no human
in the loop. Today the first half works and the second does not.

---

## ⚠ THE MOVE THAT LOOKS RIGHT AND IS FORBIDDEN — with the line the doctrine actually draws

**Do not add a periodic sweep that POLLS FOR A CONDITION.**

`modules/benchmark_grade.rs` opens with the law:

> The whole system is event-based, never polling
> ([[the-whole-system-is-event-based-not-polling]]): nothing scans the board on a clock —
> the transition event fires the grade.

I proposed exactly that sweep before reading the module, and it would have quietly
converted an event-driven substrate into a polling one.

**CORRECTION (2026-08-18), and it matters because I wrote the rule too broadly the first
time.** The same module already runs a 180s `tick`, and its own comment draws the real
distinction:

> The one periodic ACTUATOR (doctrine: actuators may tick; condition-polls may not): lease
> expiry is a TIME fact with no wire event…

So a tick is permitted when it ACTUATES a time fact the wire cannot carry. What is
forbidden is ticking to ask "has anything become gradeable yet" — a condition poll that
duplicates an event. My original ⚠ conflated the two and would have blocked a correct
build.

**The correct question is still "which EVENT did we fail to emit?" — but ask it second.**
Ask first: *does the mechanism already exist and is it simply not reaching production?*
Both times I skipped that question today, the answer was yes.

### The axis is DETERMINISM, not tick-vs-event (Joel, 2026-08-18)

> "if it's deterministic and not scan it or polling it's reliable"

That is the rule this doc should have led with, because it explains WHY the doctrine
exists rather than restating it as a taboo:

- **A scan is unreliable by construction.** Whether it catches a thing depends on when it
  ran, what happened to be on disk at that instant, how the results sorted, and whether a
  cap truncated them. Every one of those is real here: the board's artifact scan is capped
  at 200 trees and lost 12 of 13 artifacts to a recency sort against chatty run files.
  Same input, different answer depending on timing — that is the definition of unreliable.
- **A deterministic actuator is reliable even though it ticks.** Boot enumerates the run
  ledger, every record marked `running` becomes `failed`, and re-running changes nothing.
  No ordering, no sampling, no cap. It answers the same way every time.

So the test to apply to any mechanism in this tail is not "does it have a timer" but
**"given the same state, does it always produce the same outcome?"** The lease sweep passes
(a pure truth table over state + artifact presence). A "look around for work that seems
ungraded" pass fails, and would fail with or without a clock.

**And determinism is worth nothing if it is deterministic about the wrong thing.** The
reaper fixed in `f3cb3a65c` was perfectly deterministic — over a filename no writer had
ever produced. It answered the same way every time: nothing here. Reliable and blind are
not the same property; the guard has to be pointed at what production actually writes,
which is why the naming now lives in exactly one place.

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

## ~~Seam 2 — a lapsed lease is a state change, so emit it~~ — RETRACTED, IT IS ALREADY BUILT

**This section's premise was wrong and is withdrawn (2026-08-18).** I wrote it after
grepping `modules/work.rs` for a lease-expiry event, finding nothing, and concluding #451
"does not fire in practice despite being marked complete."

#451 is complete, correct, and lives in `modules/benchmark_grade.rs` — the same file I had
open. `sweep_lapsed_bench_cards` runs on a 180s tick, and it is thorough: a pure
`sweep_ready` truth table (claimed/in-progress **and** lease lapsed **and** artifact on
disk), `bench_artifact_present` checking a dirty tree for SWE and a non-empty solution file
for gym, room-SCOPED close (the #345 write-half trap), a provenance note posted into the
room, a per-tick close cap, and an error probe on refusal. A live claim is never preempted.

I grepped for my own concept — an event in the file I expected it in — instead of for the
job's name. [[read-the-code-you-intend-to-replace-before-designing-its-replacement]]

**Why the 13 artifacts sat ungraded anyway, measured rather than inferred:** all 13 carry
`card: None, owner: None`. They have **no cards at all** — they are detached `agent/solve`
run artifacts (#425). The sweep operates on board cards, so it can never see them, and that
is correct behaviour, not a gap in the sweep.

The open policy question survives on its own merits and is still Joel's: does a lapsed
claim with an artifact auto-CLOSE (what the sweep does today) or move to a `needs-verdict`
state a citizen can re-claim (truer to #419)? Not a plumbing detail.

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
