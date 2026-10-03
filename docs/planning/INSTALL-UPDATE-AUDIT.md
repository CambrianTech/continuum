# Install and Update Audit

Read-only source audit of `origin/canary` at `d014bad83`, 2026-10-03. Line references are on that commit.
Status notes marked **(update)** were added the same day as the work moved.

**Finding:** none of the three paths a non-programmer takes ends in a running core that keeps itself
up to date. The Mac one-liner never starts a core but says it is running. `continuum update` is not an
update verb in the Rust CLI. On macOS launchd refuses every update, so a Mac user can never get past
the first build they installed.

## The rules this is checked against

Our users are not programmers. They run the documented one-liner install, then `continuum update`, or
the app updates itself on start, as Claude Code and Codex do. Nothing else.

- **No manual steps.** Every repair happens inside install or update. "Re-run the install" is the most we ask.
- **One cached prompt.** Elevation only through declared installer steps, asked once, with a plain reason, cached.
- **Attention is rare.** Never prompt when nobody is there; never repeat a prompt.
- **Plain words.** No raw OS codes, `launchctl` output or log paths in user-facing text.
- **Every user.** Nothing that depends on our machines, a source checkout, cargo or our fleet tooling.
- **Self-healing.** An update that fails repairs itself or stays on the working version.

## Critical: cannot function at all

### C1 · macOS one-liner never builds or starts the core, then reports it is running
`install.sh:1068` runs `cd "$INSTALL_DIR/src" && npm start`, but `src/` no longer exists, so it only
prints a warning. On macOS the compose file runs no core container. `install.sh:1173` still prints
"Continuum is running". Nothing in the one-liner registers launchd.
**Fix:** install a prebuilt core and run `continuum install`; fail plainly if no core answers.
**(update)** Owner: Cormac, falls out of card 30a8b3ac (published binaries + download instead of compile).

### C2 · `continuum update` is not an update command
The Rust CLI has no `update` arm (`continuum.rs:195-338`), so the word is sent to the core as an unknown
command. The old bash CLI (`bin/continuum:726-753`) does `git reset --hard origin/main` and a Docker
rebuild, and never touches the native Mac core. Which `continuum` a user gets depends on install order
(`install-common.sh:428-470` vs `start-server.sh:886-892`).
**Fix:** one Rust CLI on PATH with a real `update` verb that fetches a release and hands it to the
supervisor; retire the bash update.

### C3 · macOS: launchd refuses every new build
`continuum install`, `reboot` and the automatic update all stage the new binary into the launchd slot
(rename the old aside, copy the new to a fresh inode) and kickstart it (`launchd.rs:519-562`,
`continuum.rs:1545-1567`). On the M5 every new build since 2026-10-03 07:10Z was refused with
`OS_REASON_CODESIGNING` and rolled back. Locally built cores are ad-hoc linker-signed with no Team ID,
so each build has a new CDHash; the job is a Background Task Management "legacy daemon" with a managed
LWCR. Each refused attempt costs a full build and up to ten minutes of downtime.
**Fix:** make the update path produce a launch launchd accepts for every user. Candidates: a Developer
ID signed core with a stable Team ID (card 30a8b3ac slice 3), re-registering the job with elevation
declared once at install, or a stable signed launcher that swaps only what it starts.
**(update)** Mechanism still being confirmed from the system log (Fable).

### C4 · macOS: the update kickstarts a system-domain job without root
The default install registers `system/com.continuum.core` (`continuum.rs:3738-3743`). Updates then run
`launchctl kickstart -k` as the user; the code's own comment says that needs root
(`continuum.rs:3818-3821`).
**(update)** Confirmed on the M5, 2026-10-03 15:02 local: the rollback's
`launchctl kickstart -k system/com.continuum.core` failed with `1: Operation not permitted`; the node
came back only because launchd restarted the job itself.
**Fix:** the handoff must not need a privilege the update does not have: a per-user job, or a
privileged step declared once at install.

### C5 · automatic updates cannot fire for ordinary users
The tracker reads CI status through `gh api` (`deploy_tracker.rs:215-220`); without `gh` logged in it
waits forever, and the Mac/Linux one-liner never installs `gh`. It follows `canary` by default and
checks out the canary tip, silently moving the user off the front-door branch.
**Fix:** a release channel the CLI reads without `gh`, defaulting to the front-door branch.

### C6 · every install and update compiles from source on the user's machine
The one-liner installs rustup and builds; the Windows installer provisions Build Tools, CUDA, CMake and
LLVM; the install core arm builds HEAD. The one-liner's header still says "no compilation needed".
**Fix:** publish prebuilt releases per platform; install and update download, verify and stage them.
**(update)** Owner: Cormac, card 30a8b3ac (#4691 is slice 1, macOS canary artifacts).

### C7 · Linux has no supervised update path
`continuum install` reports the Linux arm as missing and points at a script; `reboot --service` is
unsupported on Linux. An update starts the core outside the systemd unit, and `Restart=on-failure`
leaves a clean stop down.

## High

- **H1 · Windows: re-running the one-liner does not update.** The bootstrap clones only when the folder
  is absent; pulling needs `-Update`, which refuses with git language.
- **H2 · Windows: one `continuum install` can raise up to four separate admin prompts.** `-Verb RunAs`
  for the supervisor, gsudo for release and engine registration (a new elevation session per PowerShell
  child), and another `-Verb RunAs` for teardown. None share a cache.
- **H3 · macOS: the sudo prompt comes with no reason.** Before `sudo` (`launchd.rs:665-668`) the user sees
  only Rust debug output such as `NotOwned { job_pid: … }`.
- **H4 · macOS: three competing supervisor installers under two labels.** Bash `continuum service install`
  (`homes.continuum.node`), `install-service.sh` and Rust `continuum install` (both
  `com.continuum.core`, in different forms). The bash `service status` asks for a password.
- **H5 · `continuum install` refuses on a busy node with developer instructions** ("rerun with
  `continuum reboot --force`", `benchmark/round-stop`).
- **H6 · macOS: each refused update keeps a full core copy forever.** `launchd.rs:186-201` keeps a
  `.failed-<ms>` copy per refused attempt (179 MB each) with no eviction decision.

## Manual steps and raw internals in user-facing text

- `install.sh` failure guidance asks users to ping GitHub, `rm -rf` the install, `docker login`, read
  logs and file an issue, and prints exit codes and log tails.
- Install steps hand work to the user: install git, cmake or Node by hand; set up Docker Desktop in five
  manual steps; run `hf download`; start podman; `apt-get install … vulkan`.
- The Rust lifecycle shows `OS_REASON_CODESIGNING`, raw `launchctl` stderr, "the node is DARK", and
  "inspect ~/.continuum/logs/service.err.log".
- Windows prints gsudo internals and tells users to install winget from the Microsoft Store.
- The docs disagree on what the one-liner is: README, `docs/SETUP.md`, `bootstrap.sh` and
  `continuum.homes/install` each give a different command.

## Elevation inventory

Neither the macOS nor the Windows automatic update calls sudo, gsudo or RunAs, so nothing prompts while
a user is away. The macOS automatic path does rely on the unelevated system-domain kickstart in C4.

| # | Site | Path | One declared prompt? | Cached? | Unattended? |
|---|------|------|----------------------|---------|-------------|
| 1 | `launchd.rs:666` sudo install --elevated | macOS `continuum install` | Single, no reason shown | sudo timestamp | No |
| 2 | `launchd.rs:738-739` sudo bootout + rm | `continuum uninstall` | Two calls | sudo timestamp | No |
| 3 | `install-service.sh` | README | Not batched | sudo timestamp | No |
| 4 | `bin/continuum` service install/status | Bash CLI | Not declared; status prompts | sudo timestamp | No |
| 5 | `install.sh:139-140` get.docker.com, usermod | Linux one-liner | Outside the warm-up | sudo timestamp | No |
| 6 | `install-common.sh:76-118` ensure_sudo_warmed | Installer | Declared, one prompt | Keepalive for the run | No |
| 7 | Docker Desktop vmnetd | Mac prerequisite | Outside the installer | n/a | No |
| 8 | `install-common.ps1:140` Invoke-Elevated | Windows installer | Declared, one UAC, with a reason | gsudo, installer process | No |
| 9 | `windows-service.ps1:524` | Release and engine registration | New session per child | Per process | No |
| 10 | `supervisor_install.rs:685` RunAs | Windows `continuum install` | Separate UAC | No | No |
| 11 | `elevated_teardown.rs:225` RunAs | Install core teardown | Separate UAC | No | No |

## Order

1. The macOS launchd refusal (C3, C4): confirm the mechanism, fix it inside the shared update path,
   verify on a second Mac and on a fresh install followed by an update. (Fable)
2. A real `continuum update` (C2): one CLI on PATH; the bash update retired.
3. The Mac one-liner starts and verifies a core (C1): never "running" without an answering core. (Cormac, via 30a8b3ac)
4. Updates that need no `gh` (C5) and follow the front-door branch.
5. Prebuilt releases (C6): removes compilation from every user's machine. (Cormac, 30a8b3ac)
