# One-Command Round — initiation and resume are the SAME verb, and it self-establishes

**Joel, 2026-08-24 (the binding mandate):** *"If this is not fixed, Opus will
fail. It must work automatically entirely after initiation or resume (same
command I imagine). New repo users can't be expected to cleanse their
environment and have some agent fix what the benchmark or any other system
requires. Airc must automatically work, along with any other dependency, and
agents must be able to join."*

The 8/23–24 marathon proved the substrate; it also proved the OPERATING
RITUAL was hand-rolled: roster-hold file writes, kill→deploy→relaunch shell
chains, session-local health monitors, capture post-mortems. A weaker driver
or a fresh clone gets none of that. Per [[foolproof-over-instructions]],
every one of those ritual steps is a defect this verb deletes.

## The verb

    continuum benchmark/round --benchmark mirrorcode
    # …interrupted for ANY reason (reboot, crash, lid, Ctrl-C)…
    continuum benchmark/round --benchmark mirrorcode   # ← RESUMES. Same command.

No flags for resume. The verb reads the round's last values (the run marker +
the per-task grade ledger) and continues from the first ungraded task —
resume-is-recall, [[continuity-is-the-default-reset-is-the-exception]].
`--fresh` is the explicit, rare reset.

## Self-establishment (every step the ritual did by hand, in dependency order)

1. **Core**: the verb IS a continuum command — autostart already covers it.
2. **airc**: health-checked; daemon started/repaired automatically. A user
   NEVER runs airc commands to make a round possible. Agents can join the
   round room mid-flight (it is a normal room).
3. **Gym**: fetched if absent, re-materialized if fingerprint-stale (today's
   manual `benchmark/fetch` folds in), GOLD-GATED before first use
   (GOLD-GATE-EVERY-GYM.md) — env-fail vs honest-fail distinguished before a
   citizen ever sits the exam.
4. **Serving**: waits decode-ready (the existing await_ready_serving gate).
5. **Citizens**: resident personas spawn if absent; the ARENA (exclusive
   hold + quiesce lease) is taken and released by the verb — never a
   hand-written roster-hold.json.
6. **Run**: learn-mode eval over the set, per-task lesson streaming, artifact
   preservation, health VERDICTS emitted as probes (the session monitor's
   thresholds become substrate: pace, stall, cache, grade cadence — bench
   ViewState renders them; the desktop shows the round breathing).
7. **Report**: the verb's OUTPUT is the round report — grades, preserved
   solutions, the confusion catalog (refused acts by cause — the 8/24
   glass-box that found 23 refused Python runs), pace stats. One artifact a
   stranger can read.

## Failure doctrine

Every failure names ITSELF and its one fix command — never "cleanse your
environment." An interrupted round is NOT a failure: it is a paused round the
same command continues. The 2×-fired 7200s hang backstop class becomes a
named per-act receipt (the bounded tool await) — but if any backstop fires,
the round SURVIVES it: infra-graded task, round continues, report says so.

## Status

SPEC (2026-08-24). Implementation is the arc after the morning round answers
the first-pass question. Composes existing parts only: cognition/eval,
roster_hold, quiesce leases, benchmark/fetch + verify, LessonSink, artifact
preservation, the probe verdict thresholds. No new subsystem — the ritual,
promoted to a verb.
