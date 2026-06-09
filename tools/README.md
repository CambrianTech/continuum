# tools/ — dev tooling (not shipped)

Code generators, build scripts, and infrastructure helpers. Nothing
under `tools/` is part of any deployed binary or app — it exists to
support the day-to-day work of moving code through `core/`, `client/`,
`sdk/`, and `apps/`.

| Dir | Purpose |
|-----|---------|
| `generator/` | Code generators — Command specs → typed scaffolds, entity schemas → Rust + TS, daemons, audio constants, etc. |
| `scripts/` | Dev / build / CI helper scripts (TS, JS, shell) — install, start, stop, hooks, seeding, fixtures. |

Repo-root `scripts/` (e.g. `scripts/push-image.sh`) is separate and holds
release / orchestration scripts, not dev tooling.
