# Persona TypeScript Cognition Ratchet — Lane F

Mechanical gate that prevents the persona-cognition TypeScript layer from
growing while the Rust runtime takes over. See
[`docs/planning/ALPHA-GAP-ANALYSIS.md`](../../docs/planning/ALPHA-GAP-ANALYSIS.md)
§"Lane F: TS Cognition Deletion Ratchet" for the design rationale.

This is Lane F **PR-1** — the local script. PR-2 (`persona-ts-ratchet-ci`)
will wire it into `pre-push` and CI. PR-3 (`forbidden-provider-scan`) adds
deprecated-provider/fallback-comment scanning on top.

## What it checks

Two ratchets, both enforced together:

1. **LOC ratchet** — total `.ts` line count under each watched cognition
   directory must not exceed its committed baseline.
2. **New-file ratchet** — any new `.ts` file appearing under a watched
   directory must either be in the baseline file-set OR match a glob in
   the allowlist.

The ratchet only moves down. After legitimate TS deletion lands, refresh
the baseline (next section) so future PRs can't silently regrow.

## Watched directories

- `src/system/user/server/modules/cognition`
- `src/system/user/server/modules/cognitive`
- `src/system/user/server/modules/consciousness`
- `src/system/user/server/modules/being`
- `src/system/user/server/modules/central-nervous-system`
- `src/system/user/server/attention`
- `src/system/ai/server`

## Usage

```bash
# Check — fails the build if the ratchet is violated. CI mode.
scripts/ratchet/persona-ts-ratchet.sh check

# Refresh — regenerate the baseline after legitimate TS deletion.
# Commit the updated persona-ts-baseline.txt with your deletion PR.
scripts/ratchet/persona-ts-ratchet.sh refresh

# Run the test suite.
scripts/ratchet/test-persona-ts-ratchet.sh
```

## Allowlist

`persona-ts-allowlist.txt` holds path-globs for the categories of TypeScript
that ARE allowed to land in cognition directories (without burning ratchet
budget on the new-file count):

- Generated artifacts (`**/*.generated.ts`, `**/*.gen.ts`, `**/generated/**`)
- Type-only files (`**/*.types.ts`)
- Schemas (`**/*.schema.ts`, `**/schemas/**`)

Allowlist matches do NOT exempt the file from the LOC ratchet — they only
exempt it from the new-file ratchet. A new generated file still counts
toward LOC; if its addition pushes a directory above its baseline LOC,
the ratchet fails. That's deliberate: the lane is a deletion lane, not a
generated-bloat lane.

## When the ratchet fails

The script emits the specific violations and three options:

1. Move the new behavior into Rust (the lane's goal).
2. If the file is genuinely generated / a schema / a UI type, add a
   path-glob for it to `persona-ts-allowlist.txt`.
3. If you deleted TS, run `refresh` and commit the new baseline.

## Why Bash, not Rust

This ratchet is build infrastructure, not runtime behavior. The
[Lane F design](../../docs/planning/ALPHA-GAP-ANALYSIS.md) targets runtime
cognition migration. Build tooling (this script, `git-prepush.sh`,
`main-promotion-gate.sh`) lives in shell because it runs outside the
runtime and shell is the standard tool. The thing being enforced — that
runtime logic must be Rust — is separate from the enforcer's language.
