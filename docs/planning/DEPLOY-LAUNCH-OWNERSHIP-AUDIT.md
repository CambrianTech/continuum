# Deploy / launch / supervise — ownership audit (2026-09-19)

An investigation, not a change list. Joel, after the 5090 went dark for 2 h 42 m
because its supervisor task lived inside a logon session: *"Mishmash of shell
scripts needs investigation. Anything sort of rigged needs an architectural
second look. What belongs where."*

Everything below was read off the tree and the running 5090 on 2026-09-18/19,
during a night in which every one of these seams was exercised by hand. Line
numbers are canary `2d559327b`.

## 1. The concern, decomposed

There are five distinct responsibilities in "the node runs the right build":

| # | responsibility | question it answers |
|---|---|---|
| A | **Decide** | is there a newer green tip than the running build? |
| B | **Build** | produce a verified artifact for that tip on this box |
| C | **Stage** | put the artifact where the supervisor is bound to look |
| D | **Supervise** | keep the core alive across crash, reboot, logoff, session switch |
| E | **Provision** | engines, models, sidecars, airc, toolchain — the machine's *fitness* to run |

They have different lifetimes (A runs forever; B is minutes; C is seconds; D is
the OS's job; E is once-per-machine, converging) and different owners today.

## 2. Who owns what today — measured

| resp. | owner today | where | platform | rigged? |
|---|---|---|---|---|
| A decide | `DeployTrackerModule` (Rust ServiceModule) | `core/…/modules/deploy_tracker.rs`, pure core in `runtime/deploy_tracker.rs` | all | no — one decision, probes, claim-aware (#4187, #4190) |
| A decide (legacy) | `track-canary.sh --once` | `tools/scripts/track-canary.sh` (239 lines) via launchd/systemd | mac/linux | **duplicate** of A; still installed on the Macs; makes the same decision in bash with `pgrep -f "continuum reboot"` as the in-flight test |
| B build | `start-server.sh` | `tools/scripts/start-server.sh` (1,178 lines) | all (bash; Git-Bash on Windows) | **yes** — see §3 |
| B trigger | `continuum reboot` → `prepare_warm_build` | `bin/continuum.rs` | all | Rust owns the RAM gate and the receipt; delegates the build to the script by **walking up from the cwd** to find it |
| C stage | hand (`mv … .prev.exe; cp`) until #4218; now `PreparedCoreService::stage` | `bin/continuum.rs` | windows | was rigged for a month; Rust since #4218 |
| C stage (mac) | `start-server.sh` copies to `~/.continuum/bin` at `:1119` | script | mac/linux | the script stages *and* launches; C and D are not separated |
| D supervise | `ContinuumCore` scheduled task → `run-service-hidden.ps1` → `continuum service-host` → core | `install-service.sh :221-352`, `register-core-service.ps1`, `run-service-hidden.ps1`, `bin/continuum.rs::service_host` | windows | **rigged**: `LogonType=Interactive` — supervises against process death, not session teardown. The 5090 died with the logon session at 09:50:04Z (card 7b56a84b). `RestartCount=999` never fires for that. |
| D supervise (mac) | launchd agent running `core-service.sh` | `install-service.sh` writes `~/.continuum/bin/core-service.sh` (a generated 12-line bash file) | mac | a generated script as the supervisor's command; the plist calls bash calls the binary |
| D supervise (linux) | `systemd --user` | `install-service.sh` | linux | untested this month |
| A→B→C consumer | `continuum deploy-consume` + `ContinuumDeploy` task (#4208, #4212, #4218) | `bin/continuum.rs` | windows | new; its first three unattended attempts each found a different seam not owned by Rust (cwd, slot, stdout) |
| E provision | `install.sh` (756) + `install-manifest.toml` (290, "the ONE source of truth") + `install-llama-server.sh` (449) + `install-livekit.sh` + `install-tailscale.sh` + `download-*.sh` | scripts | all | the manifest is the right shape (data, idempotent modules); **but `start-server.sh` re-provisions on every launch** — installs airc if missing (`:458`), builds llama-server if missing (`:226`), starts livekit + eye-node rails, builds `continuum-mcp`, `forge-custodian`, `livekit-bridge` (`:740-1028`) |
| stop | `system-stop.sh` ("nuclear", 187), `continuum stop`, `stop_existing_core()` in the start script (`:673`) | three places | all | **three stop paths**, one of which ("nuclear") predates the Rust core |
| watchdog | `safe-deploy.sh` (247) "external watchdog… reverts to last known-good" | script | mac | a fourth deploy owner, written for the Node era, still in tree |
| docker | `continuum.sh` (239, "thin wrapper for Docker-based Continuum") | script | all | a fifth launch path |

## 3. `start-server.sh` — what one script does today (1,178 lines)

In order, on every run (including a `CONTINUUM_BUILD_ONLY=1` warm build unless
a specific step is guarded):

1. toolchain detection: cargo, ONNX Runtime, CUDA tree + nvcc flags, MSVC import (`:48-120`, `▶ CUDA tree`, `▶ MSVC toolchain imported`)
2. **provision** llama-server: build it if missing (`:226-241`)
3. reap/adopt llama lanes: `ps -o command=` + `/health` per pid, `kill -TERM` the unhealthy (`:337-366`) — *not* guarded by BUILD_ONLY
4. **provision** airc: install if missing, ensure the daemon, adopt or restart it, derive room/channel (`:404-624`)
5. `stop_existing_core()` (`:673`, called at `:1058`) — the launch half
6. build `continuum-mcp` (`:740`)
7. build the `continuum` CLI (GPU-free features, fallback to full) and **install it to `~/.local/bin`** (`:773-860`). Under `CONTINUUM_SKIP_SELF_BUILD` the *build* is skipped but the *install* still copies whatever binary is in the target dir — measured twice on 2026-09-19: a stale `74cf291b4` CLI replaced the one on PATH mid-deploy (card 4c8a88bb, Kimi)
8. build `forge-custodian` (`:875`)
9. build `continuum-core-server`; staleness check against sources (`:889-929`)
10. `CONTINUUM_BUILD_ONLY`: write the receipt and exit (`:950`) — the warm-build seam Rust consumes
11. start livekit-server, build+start livekit-bridge, start eye-node (`:977-1051`)
12. launch the core (`:1060`), then `ensure_moonshine` (`:1078`)

One file owns B (build), E (provision), part of D (launch/stop), and the CLI's
installation. Every `cargo build` in it is a separate invocation of the same
crate with different `--bin`/features, which is why one deploy on the IntelMac
costs three continuum-core compiles (card 4c8a88bb) and why a Windows build can
take 50 minutes on a box that also serves.

The Rust side reaches into this script through three environment variables
(`CONTINUUM_BUILD_ONLY`, `CONTINUUM_BUILD_RECEIPT`, `CONTINUUM_SKIP_SELF_BUILD`)
and finds it by walking up from the process's cwd (`locate_start_script`,
`bin/continuum.rs:4146`) — which is how the consumer's first unattended tick,
started by Task Scheduler in `System32`, found "no start script (installed
node)". The launchd tracker has the same wall in its own words
(`track-canary.sh:114`: "under launchd the cwd is `/`").

## 4. What is rigged, named

1. **The supervisor is inside the logon session** (Windows). `LogonType=Interactive`. It restarts a crashed core (measured 2026-09-18: ~6.5 min from a hard reset to citizens perceiving) and it dies with the session (measured 2026-09-19: 2 h 42 m dark). Card 7b56a84b.
2. **Build and provision share one script**, and the script runs its provisioning on every warm build. A "build the tip" step re-checks CUDA, may install airc, may build llama-server, reaps engine lanes, and installs the CLI — none of which a deploy asked for. Cards 4c8a88bb, #422.
3. **Two deciders**: the Rust tracker and `track-canary.sh` both decide when to deploy. The Macs still run the script (launchd). Two answers to one question is the drift the compression principle names; the in-flight test differs (`deploy_claim` vs `pgrep`).
4. **The script is located by cwd**, and every unattended launcher (launchd, schtasks, systemd) starts somewhere else. The fix that exists is an env var per launcher (`CONTINUUM_START_SCRIPT`), i.e. configuration compensating for a lookup that should have been a path the installer recorded.
5. **The CLI on PATH is replaced by the build, not by the deploy.** The slot's CLI (`service-b/continuum.exe`) and `~/.local/bin/continuum.exe` are two installs of the same binary, written by two different actors (`stage()` and `start-server.sh:839`). #422 "STALE CLI" is the symptom.
6. **Three stop paths** (`continuum stop`, `stop_existing_core`, `system-stop.sh`) and **five launch paths** (`continuum start`, `continuum reboot`, `start-server.sh` direct, `safe-deploy.sh`, `continuum.sh` for Docker).
7. **Generated shell as a supervisor command** (mac): `core-service.sh` is written by the installer for launchd to call bash to call the binary. The Windows equivalent is `run-service-hidden.ps1` → `service-host`. The binary could be the task's command on both.
8. **Consumer stdout**. A scheduled consumer has no terminal; the first attempt's reason was lost. Fixed for the consumer's own lines (#4218) but the warm build's bash output still did not land — open.

None of these is a bug in a component. Each component is correct. They are
*seams*: responsibilities split across a script, a Rust verb and an OS
supervisor, joined by environment variables and cwd conventions. That is the
"correct parts composing into silence" class, applied to deployment.

## 5. What belongs where — the read

Not proposals; the ownership that the existing architecture docs already imply
(CBAR runtime contract, "headless Rust core, clients render", install manifest
as data), stated per responsibility.

| resp. | belongs to | why |
|---|---|---|
| A decide | **Rust, in the core** (`DeployTrackerModule`) — already true | it is a decision with probes, hysteresis and a claim; it must be the same on every node. `track-canary.sh` is the Node-era owner and should be marked legacy in place until removed. |
| B build | **Rust verb** (`continuum build <tip>` or the warm-build half of `reboot`) that invokes **cargo directly** with a recorded feature set, RAM gate, jobs cap and a receipt — the *decision* of what to build (bins, features per platform) as **data** (the manifest already has the per-platform shape) | a 1,178-line bash file is not a build definition; it is a build definition plus a provisioner plus a launcher. Cargo is one invocation with `--bins`; three compiles of one crate is the script's doing, not cargo's. |
| C stage | **Rust** (`PreparedCoreService::stage`, #4218) — the installer records the slot; the stage step writes it | a copy with a sha check is not shell work, and the supervisor's descriptor is the single source of the slot path |
| D supervise | **the OS supervisor, registered by the installer, with the binary as its command**, run whether a user is logged on or not | the core is a service, not a session child (82af11f5's title). Windows: S4U logon; mac: a LaunchDaemon or an agent with `KeepAlive` and no generated wrapper script; linux: `systemd --user` with `linger`. Registration belongs to the installer (E), not to `start-server.sh`. |
| E provision | **the manifest + its thin runner**, run on install/update/repair — **never on launch** | provisioning converges the machine once; a launch that provisions is a launch that can install airc, rebuild an engine, or replace the CLI as a side effect — measured, all three, this week. The engine-lane reaping (`:337`) and the airc daemon handling (`:404`) are *runtime* concerns and belong to the core's own modules (they largely already exist there: `serving_daemon`, the airc liveness probe). |
| stop | **one verb**, `continuum stop`, owning the socket, the pidfile and the engines; the supervisor is the only other thing allowed to stop the core | |
| CLI on PATH | **the installer/stage step only**; a build never touches it | one binary, one writer |
| the script's location | **the installer records it** (or it disappears when B moves to a verb) | a launcher starting in `/` or `System32` must not need to know where the repo is |

The shape this converges on: **`continuum` (the CLI) is the only thing any
launcher, supervisor or human calls**; the installer registers the supervisor
and writes the manifest's machine state; the core's modules own runtime
concerns; `start-server.sh` shrinks to nothing or to the developer's
from-source convenience it says it is.

## 6. Receipts to judge any change by (from Fable's latency law and Joel's laws)

- a logoff, RDP switch or console lock during serving leaves the citizens perceiving;
- a merged green tip reaches every node with no human, and `deploy.settled` says so on each;
- one deploy = one compile of continuum-core per node, RAM-gated, jobs-capped;
- the CLI on PATH always reports the running build's sha (no "STALE CLI");
- `grep -rn "continuum reboot\|start-server.sh" tools/ ~/.continuum/bin` returns only the installer.

## 7. Cards this audit touches

82af11f5 (supervisor / consumer, mine), 7b56a84b (session-bound supervisor, P0),
4c8a88bb (start-server.sh, Kimi), #422 (stale CLI), 2fd80a0e (deploy seam
read-back, Cormac), a476ea14 (claim ceiling), ca365c81 (ratchet baseline).
