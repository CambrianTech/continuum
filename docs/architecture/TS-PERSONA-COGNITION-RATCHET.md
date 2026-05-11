# TS Persona Cognition Deletion Ratchet

**Lane F** (PR #1084 alpha workstreams). Enforces the Rust-first alpha
contract (PR #1070, `docs/planning/ALPHA-GAP-ANALYSIS.md` — "Rust core
owns behavior"): every PR touching the persona surface must keep the
total TypeScript line count flat or shrink it.

## What's measured

The ratchet counts non-test `.ts` files under `src/system/user/server/`:

```
find src/system/user/server -type f -name '*.ts' \
  -not -name '*.test.ts' -not -name '*.spec.ts' \
  -exec cat {} + | wc -l
```

This includes the persona orchestration layer (`PersonaUser.ts`,
`PersonaResponseGenerator.ts`, `PersonaMessageEvaluator.ts`,
`RustCognitionBridge.ts`, etc.) — the surface that must shrink as Rust
runtime takes ownership of cognition.

## Why a single total, not per-file

Refactors that move code between files within the surface are common
and shouldn't trip the ratchet. What matters is the SURFACE total. A
PR can grow one file by 200 lines AS LONG AS it deletes 200+ lines
elsewhere in the surface.

## Baseline

`scripts/ratchets/ts-persona-cognition-baseline.json` carries the
high-water mark. The CI gate fails any PR whose current count exceeds
this number.

## Lowering the baseline

After a PR that legitimately shrinks the surface (e.g., deletes a
TS-side cognition path because Rust now owns that responsibility),
the **author** updates the baseline:

```bash
bash scripts/ratchets/check-ts-persona-cognition.sh --update-baseline
git add scripts/ratchets/ts-persona-cognition-baseline.json
git commit -m "ratchet: lower TS persona-cognition baseline to <new>"
```

This is intentionally a manual step. The baseline only ratchets DOWN —
mechanical write-on-merge would lose the deletion-pressure signal.

## What CI does

`.github/workflows/ts-persona-cognition-ratchet.yml` runs:

- On PRs to `canary`/`main` that touch the surface OR the ratchet config.
- On direct pushes to `canary`/`main`.
- Fast: shell + python only, ~10s.
- Independent gate (doesn't block on TS compile or Rust build).

Failure output names the actionable next step:

```
━━ ❌ TS persona-cognition RATCHET FAILED ━━
  Baseline: 27160 lines
  Current : 27200 lines
  Delta   : +40 (growth)

  Per Rust-first alpha contract (PR #1070, docs/planning/ALPHA-GAP-ANALYSIS.md),
  the TS persona surface must SHRINK or stay flat. New cognition logic belongs
  in Rust:
    workers/continuum-core/src/persona/
    workers/continuum-core/src/cognition/
```

## Local pre-PR check

Before pushing a PR that touches the surface:

```bash
bash scripts/ratchets/check-ts-persona-cognition.sh --verbose
```

Prints the per-file LOC table so you see which file changed and by how much.

## Out of scope (followups)

- **Forbidden-strings check**: detect `"fallback"`, direct adapter
  instantiation, or other anti-patterns Joel has flagged. Per #1084
  Lane F success criteria. Will land as a separate gate next to this
  one.
- **Verb-shape detection**: identify cognition VERBS (e.g.,
  `shouldRespond`, `scoreRelevance`) being added in TS even when total
  LOC drops. Heuristic, harder to define rigorously — lower priority
  than the LOC ratchet which catches the gross case.
- **Pre-commit hook integration**: today's gate is CI-only. Adding to
  pre-commit would catch growth before push, faster signal. Reserve
  for after the LOC ratchet has been live for ~1 week so we know the
  shape isn't going to oscillate.
