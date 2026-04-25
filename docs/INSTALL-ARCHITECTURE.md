# Install architecture

How continuum's installers stay maintainable across macOS, Linux, and Windows without diverging.

## Goal

A first-time dev on any supported OS runs **one command** in their default shell and ends up with continuum running locally + a `continuum` command on PATH. Zero manual steps after that one command. No "now also do X in Docker Desktop settings."

## The challenge

bash and PowerShell are different shells with different idioms. We can't share install scripts literally; we have to share *structure* and minimize the surface that diverges.

## Architecture

```
bootstrap.sh    Canonical install body. Runs on macOS, native Linux, and
                inside WSL2 on Windows. Single source of truth for
                "what continuum needs to be installed properly":
                  - clone or update the repo
                  - docker compose pull (right compose file per platform)
                  - docker compose up -d
                  - wait until widget-server reports healthy (with timeout)
                  - install the `continuum` CLI shim
                  - open the browser

install.sh      Thin POSIX entry. ~150 lines.
                  - probe + brew/apt/dnf-install missing prereqs (git,
                    Docker Desktop, etc.)
                  - toggle Docker Desktop AI settings via the macOS plist
                    or Linux settings.json path
                  - exec bootstrap.sh

install.ps1     Thin Windows entry. ~150 lines.
                  - probe + winget-install missing prereqs (WSL2 + Ubuntu,
                    Docker Desktop, optional pwsh 7)
                  - toggle Docker Desktop AI settings via the Windows
                    %APPDATA%\Docker\settings.json path
                  - drop continuum.cmd shim into %LOCALAPPDATA%\Programs\
                    continuum + add to user PATH so `continuum` works
                    from any shell
                  - exec bootstrap.sh inside WSL via `wsl bash bootstrap.sh`
```

## Drift-prevention rules

bash and PowerShell can't be literally identical. The architecture itself prevents drift:

1. **bootstrap.sh holds 90% of the install logic.** Both entries are dumb
   prereq-checkers + delegators. The thing maintainers care most about
   ("did the Docker version bump break us?", "did the compose file move?")
   has exactly one place it can go wrong.

2. **The two entries mirror section-by-section** with matching headers in
   the same order:

   ```
   # ── section: prereqs ──────────────────────────────────
   # ── section: docker desktop AI settings auto-toggle ──
   # ── section: continuum CLI shim ──────────────────────
   # ── section: delegate to bootstrap.sh ────────────────
   # ── section: post-install guidance ───────────────────
   ```

   A reviewer comparing the two entries in a side-by-side diff sees the
   parity instantly. If a section appears in one and not the other,
   that's a code smell.

3. **Header note at the top of each entry**:

   ```
   # COUNTERPART: install.{sh|ps1}. Any change to one needs a matching
   # change in the other or the platforms diverge. The actual install
   # body lives in bootstrap.sh; only platform-specific prereq install +
   # Docker Desktop settings paths differ between this and the counterpart.
   ```

4. **CI smoke test** (small) that asserts both entries call `bootstrap.sh`
   with the same env-var / arg shape — automated drift detection. Fails
   the build if the two entries drift on the delegate contract.

## Why this works

Same model the airc port used (canonical `airc` bash + native PowerShell
`airc.ps1`). The two implementations survived a ~12-bug-hunt cycle on
day-1 use without diverging because the structure stopped that from
being a casual mistake. Every fix to one prompted a check of the other,
and the small entry-point surface meant the check was cheap.

## Friction points the new install.ps1 closes

Today's `setup.bat` + `bootstrap.ps1` together leave these gaps:

- **Docker Desktop AI settings are a manual step.** The README says
  "enable GPU-backed inference + host-side TCP support" — every fresh
  dev hits this. The new install.ps1 (and install.sh) writes the
  settings.json directly + bounces Docker Desktop. Zero manual toggles.
- **`setup.bat` infinite `wait_loop`** on widget-server health (no
  timeout). Replaced with a bounded wait + actionable failure message.
- **`setup.bat` relative-path quirks** in the WSL handoff (`cp src/...`
  depends on cwd). Eliminated by using absolute paths derived from the
  script's own location.
- **No Windows shim.** Today users have to remember `wsl bash continuum`
  every time. New install.ps1 drops `continuum.cmd` into
  `%LOCALAPPDATA%\Programs\continuum` + adds to PATH so `continuum
  <verb>` works from PowerShell, cmd.exe, Run dialog, Task Scheduler.
- **No auto-WSL2-install.** `bootstrap.ps1` does this but `setup.bat`
  doesn't. Unifying into one entry that always handles it.
- **No clear "what state am I in?" surface.** Add a `continuum doctor`
  invocation hint at the end of install so the user can self-verify.

## What gets retired

- `setup.bat` — replaced by `install.ps1`.
- `bootstrap.ps1` — replaced by `install.ps1` (with the WSL2 install
  logic preserved + extended).
- The current `install.sh` — refactored to the thin-entry shape above;
  heavy logic moved into `bootstrap.sh`.

## What stays

- `bootstrap.sh` — promoted to canonical install body.
- `setup.sh` — keep as a back-compat alias that just exec's
  `install.sh`. Existing docs that reference `./setup.sh` keep working.

## Validation plan

1. **Static review** of this doc by peers (continuum-b741, anvil,
   bigmama-wsl) on the canary mesh.
2. **Implementation** in commits that mirror section-by-section across
   install.sh and install.ps1.
3. **Live dogfood** of `iwr ... | iex` on a real Windows box (the same
   pattern the airc PS port used to catch ~12 PS-specific bugs the
   first day).
4. **Live dogfood** of `curl ... | bash` on macOS (anvil) for the POSIX
   entry.
5. **CI smoke** that asserts the two entries' delegate contract matches.
6. **Promote** via PR feat/unified-windows-install → main only after
   peers confirm green on their platforms.
