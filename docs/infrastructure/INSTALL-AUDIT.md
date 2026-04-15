# Install Path Error-Audit — airc-96dd, 2026-04-15

Scope: the three scripts on the install critical path.

## `install.sh` (root Carl path, 238 lines)

Single error surface: `info "Docker not found — installing via get.docker.com…"` at L66.
- Already uses `info` (colored, prefixed). Acceptable.
- Action: none. Message is informational, not an error.

## `src/scripts/install.sh` (canonical Dev path, 564 lines)

Three user-facing paths that use a not-found/ERROR string:

1. **L96** — `CUDA: ... nvcc not found — inference works, training needs CUDA toolkit`.
   - Acceptable. Already tells the user what it means.
2. **L472** — `⚠️ install-livekit.sh not found — voice/video calls will not work`.
   - Acceptable. Impact stated.
3. **L543** — `Rust: $(rustc --version ... || echo 'not found')`.
   - Status line only. Harmless.

## `src/scripts/lib/install-common.sh` (module library, 316 lines — 71951af7c)

Uses a strong pattern: `warn()`, `die()`, `module_fail(name, msg)`. Every failure includes a human-readable fix string.

Example (L176): `module_fail "docker-wsl-integration" "Failed to edit Docker Desktop settings. Open Docker Desktop → Settings → Resources → WSL Integration → enable your distro manually."`.

- Fail-loud-with-fix is the right abstraction.
- All `module_fail` calls should follow the pattern: `module_fail "<module>" "<what happened>. <what the user does>."`.

## Findings (items for `doctor` / reliability work)

1. **install-common.sh module_fail error messages are the reference standard.** Anything else in install-path should match this pattern.
2. **No bare `exit 1` in install.sh scripts** — clean. Other scripts (docker entrypoints, test helpers) do have bare exits, but those are runtime, not install-path.
3. **Install-path is already reasonably clean.** The bigger friction surface is NOT bad error messages in install scripts — it's runtime / container startup / SSH host-key / post-install behavior.

## Recommendation

Focus reliability effort post-install (doctor, runtime logs) rather than re-writing install-script error strings. The install scripts already have good hygiene; the pain is downstream.

If you want me to extend the audit to `.ts`/`.rs` error paths, say so — those are larger surfaces (thousands of hits) and need a different tool than grep (semantic grouping).
