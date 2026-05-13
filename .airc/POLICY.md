# Continuum collaboration policy (AIRC pilot — #1109)

This file is the canonical rulebook for any human or agent working in
the Continuum repo. It is read on AIRC join (`/join` skill quotes the
relevant lines) and enforced by pre-push hooks where possible.

## Branch + PR rules

- **All work targets the `canary` branch via PR.** Direct pushes to
  `canary` or `main` are forbidden. Branch protection enforces this.
- **`main` is the publish branch.** Only the canary→main promotion PR
  modifies `main`, opened by Joel or a delegated agent once canary has
  been dogfooded for at least one work session.
- **Feature branches use one of three prefixes:** `feat/`, `fix/`,
  `chore/`. Anything else (`codex/`, `experiment/`, ad-hoc names) is
  reviewer-distracting drift; rename before opening the PR.
- **PRs must rebase on canary before requesting review.** Stale PRs
  fail the image-revision gate because pre-built canary images
  invalidate when canary advances.

## Push discipline

- **`--no-verify` is forbidden.** No exceptions, even for "pre-existing
  failures." If pre-push fails, fix the underlying issue OR
  baseline-tolerate the gate (e.g., ESLint baseline). Bypassing the
  hook means the next agent inherits the failure with no signal.
- **`--no-gpg-sign`, `--no-edit` on rebase, force-push to canary/main:
  also forbidden.** Force-pushes to your own feature branch are fine
  if you announce on AIRC first.
- **Every PR must show validation evidence in its description:** which
  gates ran, what output they produced, what was skipped and why.
  "Local gates green" without specifics is not evidence.

## Error + fallback discipline

- **Never swallow errors.** `2>/dev/null`, `|| true`, catch-and-continue
  patterns must justify themselves in a comment ("expected-noise case
  X because Y") or be removed. Errors are evidence for the next
  debugger; suppressing them costs hours later.
- **Fallbacks are illegal at the architectural layer.** Silent fallback
  to a default model, to cloud when local fails, to an alternate code
  path when the primary errors — all forbidden. Fail loud. The
  caller decides recovery, not the callee.
- **`try/catch` inside command `execute()` methods is forbidden by
  default.** Let throws propagate; the outer `Commands.execute` shell
  catches and surfaces. Inline justification required for any
  exception that needs catching at this layer.

## Pattern recognition + refactoring

- **Always look for patterns before adding code.** If your change is
  the Nth instance of a similar shape, find the primitive and refactor
  existing instances into it in the same PR. Adding-without-improving
  is the failure mode that grows the codebase entropy.
- **Notice everywhere, act in scope.** Continuously catalog cleanup
  opportunities while you read code. Don't roam to refactor areas
  unrelated to your current task. Surface notes on AIRC or as
  follow-up issues; don't dive in uninvited.

## Methodology + evidence rules

- **Common-sense sniff test before every test or claim.** Read your
  proposed evidence as a skeptical outsider would. Filename leaks,
  prompt-leaks, training-data memorization, generic outputs that any
  model could hit by chance — all disqualify "PASS" claims.
- **Use opaque manifest fixtures for sensory tests.** See
  `test-data/images/manifest.json`. Never name a test input the
  literal answer (no `cat.jpg`).
- **Product-surface verification, not back-channel.** "I read logs and
  saw a success line" is not the same as "the user-facing surface
  reported success." If the product has a notification, wait for the
  notification.

## See also

- [QUEUE.md](QUEUE.md) — current sprint queue + PR-card format
- [ONBOARDING.md](ONBOARDING.md) — how to knock and join (depends on
  airc#559)
- [SAFETY.md](SAFETY.md) — outside-agent etiquette
- [ASSEMBLY-LINE.md](ASSEMBLY-LINE.md) — heartbeat, stall threshold,
  pickup protocol for blocked-or-offline-peer recovery
