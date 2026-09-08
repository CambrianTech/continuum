# Alpha: the cross-grid team round

**Joel, 2026-09-08 00:5xZ:** "Let's finish the alpha. Give Astra what she needs too. We need quality
team benchmarks and personas that scale cross grid to their full potential."

This is the plan that turns that sentence into receipts. It is the last lane of the alpha as
[ALPHA-GAP-ANALYSIS.md](ALPHA-GAP-ANALYSIS.md) defines it (Rust owns behaviour, no silent success,
replay before live claims, one source of truth per fact), applied to the thing the product is for:
a team of citizens on three machines working one board, graded by one harness, credited one by one.

## What "done" means, as a receipt

One seeded benchmark round (`swe-bench-verified`, a fixed `(dataset, seed, n)`) dispatched on the M5
with the review gate on, where:

1. citizens hosted on the M5, the 5090 and the Intel Mac claim cards off the **same board**;
2. each works in a checkout staged **on her own node**;
3. her `PASS: done` publishes the patch as a **durable card artifact**;
4. the **dispatching node grades from the artifact**, never from a path;
5. the verdict returns to **her**, wherever she is, as the ❌/✅ line with the failing tests named
   (`modules/verdict_board.rs`, PR #3868), and the card moves accordingly;
6. every settled card stamps the turns that produced it with the verdict and the role
   (owner / reviewer / finder) so the round feeds learning across nodes (card cc34ac0f);
7. the round's four axes are measured against the solo baseline on the same seed:
   time-to-claim, time-to-resolve, tokens per resolve, resolve rate.

Until (7) exists, "personas scale cross grid" is a sentence. With it, it is a row in the pace ledger
(card de51586f) and a line in the README that links to the verdict files.

## What exists today, and the exact seams

| Piece | State | Where |
|---|---|---|
| Shared board across nodes | works (store sync; a 5090 citizen claims M5 cards) | airc work board; `AircCitizen::claimable_cards_in` |
| Staging on the claimer's node | works (`on_claim` stages into HER workspace) | `modules/card_staging.rs::stage_for_claimer`, `stage_swe` (PR #3866: a settled instance restages pristine) |
| A citizen's BRAIN off-box | works (Kira/Mathis on the 5090's 27B) | `persona/remote_lane_factory.rs` (`remote_peer`), `persona/reassign-model` |
| A citizen HOSTED off-box in an M5 round | **missing** — dispatch seats only this machine's residents | `commands/benchmark.rs:1141-1145` (`resident_snapshot()` = "whoever THIS machine has in the room") |
| Settle → card state | works locally (`PASS: done` → Review / Closed) | `persona/work_burst.rs:218,366`; `modules/work.rs:1752,1808` |
| Settle → patch artifact | **missing** — nothing leaves her node but the state change | to add at the settle: publish `WorkSubmission` |
| Grader input | `SweGradeParams.patch` accepts an inline unified diff and grades in a pristine checkout | `cognition/swe_bench.rs::grade` (`ensure_grade_checkout` → `apply_patch` → gate → f2p/p2p) |
| Grader trigger | **assumes this host**: `<home>/citizens/peers/<owner>/workspace/swe/<instance>` | `modules/benchmark_grade.rs:295-372` (`grade_card`) |
| Sweep grader | grades the newest WORKED COPY on this host | `cognition/swe_verdict_sweep.rs`, `persona/staged_workspace.rs::grade_target` |
| Verdict → holder | works locally; holder name via the LOCAL runtime registry only | `modules/verdict_board.rs::holder_of` (falls back to `bench_round::card_assignee`) |
| Outcome credit | **missing** — turns are not stamped with the verdict | `persona/training_producer.rs` (buffers per live turn, no card link); card cc34ac0f |
| Card artifact on the wire | **missing** — `WorkCard` has no artifact field (Astra verified in `airc-work/src/model.rs`) | airc-work: new durable frame + board fold |

## The design (decided 2026-09-07, room #continuum)

**`WorkSubmission`** — a durable airc frame published into the card's room BEFORE the settle:

```
WorkSubmission {
  card_id, claim_id, instance,          // identity of the work
  base_sha,                             // what the diff applies to
  diff_sha256,                          // the submission's identity; newest wins on ties by time
  diff_bytes | blob_handle,             // inline up to 256 KB, else a blob handle
  publisher: PeerId,                    // WHO — durable, cross-node, the holder's identity for credit
}
```

The daemon folds it into the board projection as `card.submissions: Vec<SubmissionRef>` (newest first).
`PASS: done` carries the submission's digest in its receipt so the settle and the artifact are one fact.

**Grading** takes the newest submission for the card, applies it in the dispatching node's pristine
grade checkout (`SweGradeParams { patch: Some(diff), workspace: None }`), records the verdict, and
`verdict_board::follow` addresses the holder by the submission's `publisher` — durable identity, not
the local registry. The sweep does the same for held cards whose newest submission has no verdict.

**Dispatch** seats the room's residents from ANY node: the roster is the run room's presence roster
(`Airc::room_roster_in`) intersected with citizens who can host a lane, not `resident_snapshot()` of
this machine. A remote citizen who joins the run room after dispatch is seated by the reconciler's
reseat pass (`benchmark_resume::reseat_working_rounds`), same as a local one.

**Credit** stamps each turn in the card's window with `{card_id, verdict, role}` at settle time
(owner = publisher; reviewer = whoever moved the review card; finder = whoever named the defect) and the
bucket key becomes domain × role × outcome. This is cc34ac0f, unchanged; it just runs on submissions.

## Slices, owners, acceptance

| # | Slice | Owner | Acceptance verb / receipt |
|---|---|---|---|
| S1 | `WorkSubmission` frame + board fold in airc-work | Astra (authors), BigMama/IntelMac (wire review) | `airc work show <card>` lists submissions with digest + publisher on a second node |
| S2 | Settle publishes the submission; the receipt carries the digest | Astra | `work.submission.published {card, digest, bytes}` beside `persona.work.done` |
| S3 | `grade_card` + the sweep grade from the newest submission (never a path) | Astra | a card claimed on the 5090, graded on the M5: `benchmark.verdict.recorded {instance, submission: digest}` |
| S4 | Dispatch/reseat seats room residents from any node | Fable (M5) | `bench.round.seated {peer, node}` for a 5090 citizen in an M5 round |
| S5 | Verdict line names the remote holder by publisher identity | Astra | `benchmark.verdict.line_posted {holder}` = her name, on her node's transcript |
| S6 | Outcome credit on submissions | IntelMac (cc34ac0f) | `training.example.stamped {card, role, outcome}` on both nodes after one settle |
| S7 | The measured cross-node seeded round vs the solo baseline | Fable dispatches; everyone reads | the four axes in `benchmark/rounds` enrichment; a pace-ledger row; the README line |

Order: S1 → S2 → S3 (one PR each, small; S1 in airc, S2/S3 in continuum on top of #3866/#3868) ;
S4 in parallel; S5 with S3; S6 in parallel; S7 when S3 + S4 are live on both nodes.

## Rules that apply here (so nobody re-learns them)

- The room is the runner; grading is the activity's outcome ([BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md)).
  No parallel runner, no detached solver: the citizen in the room does the work.
- The seeded sample is the replication contract: `(dataset, seed, n)` never changes because of who is
  on the grid. Cross-node changes WHO works a card, never WHICH cards.
- A grade is feedback: it reaches the hands that can act on it, or it is a number in a file.
- A change that makes the system say less ships the probe for its new quiet state.
- Liveness is an answered request through the socket; never a pid, never a process list.
- Claim in the worktree, build in the worktree, **with the worktree's own target dir**: the board's
  lease zone wants every claimed card in `~/.airc/worktrees/<card>`, and card d2cda466's poisoning
  came from a worktree build writing into the SHARED cargo cache (`CARGO_MANIFEST_DIR` latched into
  `~/.continuum/cache/cargo-target`). Both rules hold at once: in the worktree run
  `CARGO_TARGET_DIR=$HOME/.continuum/cache/cargo-target-wt cargo check|test`; the shared cache is
  touched only by `continuum reboot` from the main checkout. Never `--no-lease-required`.
- Measure on an awake, unswapped machine: check `pmset -g log`, `vm.swapusage`, and the catch-up tick
  gaps before trusting any hourly number (2026-09-07/08 lesson).

## Out of scope for the alpha round

LoRA sharing over the grid (the genome economy), the capacity plane routing work by offer
(airc#1397), live video across nodes. Each has its own doc; none is needed to grade a team round.
