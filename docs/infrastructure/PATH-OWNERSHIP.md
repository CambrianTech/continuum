# Path Ownership

Continuum has multiple state roots because some data belongs to the repo, some to the current checkout, and some to the local user or machine. Code must make that ownership explicit. A path that depends on one developer's username, home directory, package manager, host layout, or SSH account is a bug.

## Owned Roots

| Root | Owner | Purpose | Commit Policy |
| --- | --- | --- | --- |
| `.airc/` | Repository | Project collaboration policy, onboarding, and queue documentation | Tracked only when the file is intentional project documentation |
| `src/.airc/` | Local AIRC runtime | Scoped AIRC state created by commands, lanes, monitors, and tool integrations | Ignored; never commit runtime state or secrets |
| `src/.continuum/` | Local Continuum runtime | App, test, generated, socket, session, and scratch state for this checkout | Ignored unless a generated artifact is deliberately promoted through the generator pipeline |
| `$HOME/.continuum/` | Local user | User config, secrets, model caches, machine-local logs, large artifacts, and long-lived local state | Never commit; paths must be configurable and must not assume a username |
| `$AIRC_HOME`, `~/.airc-*`, `.airc-worktrees/` | Local AIRC install/runtime | AIRC install, mesh state, and isolated worktrees | Never commit from Continuum |

## Rules

- Do not hardcode `/Users/joelteply`, `/home/joel`, `joel@`, Homebrew paths, or machine-specific mount points in executable code.
- Use `SystemPaths` or a small domain-specific path helper for Continuum-owned state. Add a helper before adding another one-off `path.join(process.cwd(), '.continuum', ...)`.
- Use `os.homedir()`, `process.env.HOME`, `PathBuf`, or an explicit environment/config value for user-owned state.
- Use command lookup through `PATH` for tools such as `espeak-ng`; allow an override such as `ESPEAK_NG_BIN` when local installs need it.
- Remote SSH commands must use `CONTINUUM_SSH_USER`, then safe local defaults such as `USER` or `LOGNAME`. They must not assume a developer account name.
- Scripts that need large local artifacts should accept a path override and default under `$HOME/.continuum`, not a personal home path.
- Generated TypeScript/Rust boundary files belong in the established generated output tree and should come from `ts-rs` or the generator, not handwritten parallel types.
- Tests should write under ignored checkout-local temp/state roots or OS temp directories. Fixture emails and display names are fine; machine paths and real usernames are not.

## Current Overrides

| Variable | Meaning |
| --- | --- |
| `CONTINUUM_HOME` | Preferred future override for user-level Continuum state |
| `CONTINUUM_ROOT` | Preferred future override for checkout-level Continuum state |
| `CONTINUUM_SSH_USER` | SSH account for grid and remote model commands |
| `CONTINUUM_COMPACTION_MODEL` | Local model path for compaction profiling |
| `ESPEAK_NG_BIN` | `espeak-ng` executable path when it is not on `PATH` |

## Review Checklist

- New code has no personal absolute path, host-specific path, or hardcoded SSH user.
- The root of every new path is visibly repo-owned, checkout-local, user-local, or OS temp.
- The path can work on macOS, Linux, and Windows/WSL unless the feature is explicitly platform-gated.
- Runtime output is ignored by Git.
- If the same path construction appears twice, move it into `SystemPaths` or the relevant Rust path module before merging.
