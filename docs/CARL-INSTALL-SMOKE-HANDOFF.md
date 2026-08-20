# carl-install-smoke — handoff (root-caused; #1 fatal fixed; verify the rest on WSL/Linux)

`carl-install-smoke` (the `curl install.sh | bash` gate — Carl's REAL entry point, not the docker-compose
gate) has been RED for 8+ runs on canary. This is reliability-spine #1 (install). Root-caused from the CI
artifact; the blocking fatal is fixed + unit-verified here on macOS. What remains needs a real Linux/WSL run
to verify — that's your part.

## The #1 fatal — FIXED + unit-verified
The install.log from a failed run (`gh run download <run-id>`) ends exactly at:
```
✗ [jtag-bin] source binary missing at: /tmp/carl-smoke-XXXX/src/jtag
```
`mod_jtag_bin_link` (`tools/scripts/lib/install-common.sh`) called `module_fail` (which does `exit 1`) when
`src/jtag` was absent. But `src/jtag` is the **old Node CLI**, moved to `legacy/` (#1840) — it is NOT part of
a headless-core install. So the installer aborted at ~49s on an optional, retired client.

**Fix (this PR):** that branch now `module_skip`s + `return 0`. Unit-verified in isolation:
`mod_jtag_bin_link /nonexistent → RETURN CODE: 0` (was `exit 1`). install.sh now proceeds past the jtag step
to `compose up`. The `continuum` / `uu` CLIs still link (those succeeded in the log); only the old `jtag` is
skipped.

## Your job on WSL/Linux — verify the smoke goes green
Reproduce the exact gate:
```bash
# from a clean checkout of this branch, on WSL2/Linux with Docker:
CONTINUUM_REF=$(git rev-parse HEAD) \
CONTINUUM_IMAGE_TAG=canary \
bash scripts/ci/carl-install-smoke.sh
```
Exit-code map (from the script header):
- **1** — install.sh failed. **This was the jtag-bin fatal → fixed.** Confirm it's gone.
- **2** — install.sh ok but continuum-core IPC socket never came up. **Most likely the next thing to hit** —
  the core container has to come up healthy (`test -S /root/.continuum/sockets/continuum-core.sock`). If it
  hangs, check `continuum-core.log` (the smoke uploads per-container logs as artifacts on failure).
- **4/5/6** — ADVISORY by default (old Node web client + jtag-chat e2e). The "silent inference failure /
  didn't reply" line in the workflow is this advisory path — do NOT let it block you unless
  `CARL_CHECK_WEB_CLIENT=1`. It exercises the retired web client.

## The compression landmine you'll want to know about
There are **TWO different `install.sh`** and they've drifted independently (no sync mechanism):
- **repo-root `install.sh`** — 1183 lines. This is what `curl … | bash` fetches (raw.githubusercontent) and
  what the smoke runs. **This is the one that matters.**
- **`tools/scripts/install.sh`** — 747 lines. What `npm run install:continuum` runs. (I de-staled THIS one in
  #1848 — Unsloth step, `cd src`/`jtag` messages — but it is NOT the file the smoke exercises.)

They share the `tools/scripts/lib/install-common.sh` helpers (which is why the one-line fix above lands for
both). But the two top-level scripts are a real single-source-of-truth violation. **Recommended follow:**
reconcile them — ideally the repo-root `install.sh` becomes a thin wrapper that sources the shared library +
`tools/scripts/install.sh` logic, so "one logical installer, one place." Don't do it blind — do it with the
smoke loop green so you can prove parity.

## Lower-priority cleanup in repo-root install.sh (cosmetic, non-fatal)
Stale `src/shared/models.json` references in COMMENTS + user-facing strings (lines ~248, 442, 458, 461, 515,
517) — models.json now lives at `legacy/src/shared/models.json` and is read by the `model-init` container
(fixed in #1851). These are comments/echoes, not fatal, but worth truing up when you touch the file.

## What's already done + verified (so you don't redo it)
- **Container BUILD path build-proven on macOS** (`docker build`, real): core image contexts were vestigial →
  deleted (#1850); model-init context fixed → builds clean (#1851). `docker compose config` green base/+mac/+gpu.
- The jtag-bin fatal → fixed + unit-verified (this PR).
- Native genome forge complete + live (Unsloth deleted) — unrelated to install, but that's why `src/` moved.

## The instruments you have
- `gh run download <run-id> -D <dir>` — pulls the smoke's artifacts (install.log + per-container logs +
  page/chat). The install.log is the ground truth for install.sh failures; the `continuum-core.log` for
  exit-2.
- `gh run list --branch canary --workflow "Carl Install Smoke"` — the red/green history.
