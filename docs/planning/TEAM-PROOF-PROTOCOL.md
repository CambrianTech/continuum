# The Team Proof: Multi-Persona Teams vs Loopers (Pre-Registered)

**Status:** pre-registered 2026-08-28. The claim this protocol tests is the product
thesis itself (Joel: "the team benchmarks prove the system"): a team of Continuum
citizens resolves more real instances than a single-agent looper on the same model,
same box, same instances — because review, memory, and role differentiation are
system properties, not model properties. Written before any team round dispatches;
amendments logged in §7.

**Why this is THE benchmark:** every solo score we post is a model score our harness
merely didn't ruin. The team delta — team resolve rate minus best-solo resolve rate,
minus the looper baseline — is the first number that measures *Continuum* and cannot
be replicated by downloading the GGUF.

## 1. The evidence that predicts a win

8 of 20 solo misses in the audited rounds were reviewer-catchable (40%): wrong-file
patches, tests never run, obvious regressions a second reader flags in one pass. A
cross-review gate that converts even half of those is +20% resolve rate — larger
than any model swap on the table.

## 2. Conditions (all: same model, same engine build, same instances, same seed,
green-env coverage map, provenance-stamped verdicts)

- **A — looper baseline:** opencode (or equivalent single-agent loop) driving the
  same GGUF through its own harness. The industry-comparable control.
- **B — parallel solos:** N citizens, N disjoint card sets, no interaction. Isolates
  "more attempts" from "teamwork" — if C beats A but not B, the win was parallelism,
  not collaboration.
- **C1 — emergent team:** N citizens in ONE activity room per card, ONE rule in the
  recipe: *no submit until a teammate who did not write the patch reviews it.*
  Roles are whatever emerges (Joel: "I just witnessed a lot of emergent team
  behavior... They'd naturally choose what to do").
- **C2 — assigned slots:** same, plus the recipe declares slots (solver / reviewer /
  test-runner) with citizens assigned. Tests whether structure beats emergence at
  current model capability.

Run order: A and B first (they need no new substrate), C1, then C2. Each condition
≥ the paired-instance minimum from the comparison protocol (15), same instances
across all four.

## 3. What the substrate already has (verified in-tree 2026-08-28)

- Activities are rooms; per-solve rooms mint-or-rejoin; rounds self-drive and
  resume across reboots (the 8/26 arc).
- `ExperienceRecord.room` attribution (A6) — landed; `from_eval` stamps it.
- Multi-room subscribers; turn reads the TURN room.
- Measured-hold + derived prompt cache — N citizens time-sharing one lane is the
  restore economy's exact design load (and the Thrash Gauntlet's aggregate ≥80%
  bar is a PRECONDITION: run the Gauntlet before C1, or team wall-clock numbers
  measure thrash, not teamwork).

## 4. What must be built (the gaps, in build order)

1. **Multi-citizen membership on one solve card.** Today dispatch claims a card for
   ONE assignee. C needs: one claimer (accountable for terminal transition —
   kanban terminal stays harness-only), other teammates JOINED to the solve room
   as working members. Design: membership is room-level (join), accountability is
   card-level (claim) — no multi-claim machinery.
2. **The review gate as recipe rule.** RECIPE = content-type + RULES (#371): the
   rule half is the thin part. C1 needs exactly one enforceable rule: the submit
   path (patch → verdict) checks that a review turn by a non-author teammate
   exists in the room since the last patch edit. Gate lives in the activity's
   outcome path, not in cognition (no Rust gates around cognition).
3. **Team stamps on experience records** — `role: Option<String>`,
   `teammates: Vec<Uuid>` stamped AT CAPTURE (temporal truth, like
   `harness_build`) — built WITH gap 1, because dispatch is what knows the
   membership to stamp. Not before (unwired fields are ratchet-bait).
4. **Team scoreboard row:** condition label + team composition on the round
   receipt, so A/B/C1/C2 read side-by-side with the same provenance stamps.

## 5. Decision rule (declared now)

"Teams kick ass" is claimable when, on ≥15 paired instances:

1. C (best of C1/C2) resolves more instances than A by more than the solo A/A
   noise floor, AND
2. C beats B (teamwork beats parallelism, not just attempt-count), AND
3. ≥1 instance shows the signature: a patch that failed review, was revised, and
   the revision resolved — the mechanism observed, not inferred, AND
4. zero substrate-fault losses in C attributable to team plumbing (a room-join
   flood or review-gate deadlock pauses the run and becomes substrate work).

C1 vs C2 is reported descriptively, not gated — it informs whether roles are
learned or assigned going forward (the experience-bandit question), and either
answer is a finding.

## 6. What the learning flywheel gets

Every C turn lands in room-attributed experience records. With gap-3 stamps,
curriculum synthesis can ask: which role did this citizen play when the team won?
Roles become learned prices (experience-bandit over role × outcome), with
genome-distance as the cold-start prior — teams that learn who they are.

## 7. Amendment log

- (none yet)
