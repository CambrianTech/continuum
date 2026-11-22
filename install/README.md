# Install

This directory is the one owner of installing, updating and repairing Continuum on a machine. Nothing
outside it installs a dependency. Each OS subdirectory holds only what differs on that OS:

| Directory | Holds |
|---|---|
| `manifest.toml` | Every dependency, declared once |
| `engine/` | The convergence engine (a Rust crate): plan, escalate, provision, verify, switch |
| [`macos/`](macos/README.md) | macOS differences: launchd, the app bundle, Metal and MoltenVK, signing |
| [`linux/`](linux/README.md) | Linux differences: systemd, glibc and musl, the GPU runtimes, distro escalation |
| [`windows/`](windows/README.md) | Windows differences: the S4U task, UAC, the CUDA and Vulkan runtimes, PATH |

## What a user gets

- **One command.** `curl -fsSL https://get.continuum.dev | sh` on macOS and Linux;
  `irm https://get.continuum.dev/ps1 | iex` on Windows. A signed installer (`.pkg`, `.msi`) runs the
  same engine.
- **Nothing compiles on the user's machine.** CI builds every binary, for every OS, architecture and
  GPU backend. The machine downloads and verifies them.
- **Installed per user, with at most one elevation.** The install lives under `~/.continuum`
  (`%LOCALAPPDATA%\Continuum` on Windows). Only a component that truly needs administrator rights asks:
  a system service, a firewall rule, or a GPU driver on Linux. All of those are granted in a single
  prompt that says why. A run with nothing to elevate prompts for nothing.
- **Exactly one password prompt per run, and none on a run with nothing to do** (Joel's rule; a second
  prompt is a bug). Without a terminal or a GUI to ask through, the install stops and says what to run.
- **Fast to first use (targets, not yet measured):** the UI opens within about 60 s of a fresh install,
  and the first persona answers within about 30 s more.
- **Kept up to date, including new dependencies.** Each release carries its own manifest. Every update
  converges the machine to that manifest, so a dependency added later installs itself on the next
  update. Nothing has to be done by hand, ever.
- **Never half-installed.** A new version runs only after every component it needs has passed its
  check on this machine. If any check fails, the machine stays on the version it had and reports which
  component failed and why.
- **Run it again to repair.** On a fresh machine the same command installs, on a broken one it
  repairs, on a current one it does nothing and prints receipts.

## How it works

```
bootstrap (per OS, ~50 lines)  ->  continuum-install (signed binary)  ->  manifest.toml of the target release
                                                                      ->  plan: what this machine lacks
                                                                      ->  one elevation, only if the plan needs it
                                                                      ->  provision into a new generation
                                                                      ->  verify: every accept check + the device probe
                                                                      ->  switch the active generation, record state
```

1. **Bootstrap.** The OS script ([`macos/bootstrap.sh`](macos/), [`linux/bootstrap.sh`](linux/),
   [`windows/bootstrap.ps1`](windows/)) does only this: detect the OS and architecture, download the
   signed `continuum-install` binary for it, check its SHA-256 against the release checksum file, and
   run it. It never installs a dependency itself, so adding a dependency never means editing a
   bootstrap. (The same split as rustup, uv, and Hermes's `install.sh` → `hermes pm install`.)
2. **Plan.** The engine reads the target release's `manifest.toml` and the machine's recorded state. It
   runs each applicable component's accept check, and the components that fail are the plan.
3. **Elevate once.** If any planned component is marked `privilege = "admin"`, the engine asks once,
   listing those components, then runs only them with the grant (sudo on Linux, one authorization on
   macOS, one UAC on Windows). Everything else runs as the user.
4. **Provision into a generation.** Components install into
   `~/.continuum/generations/<release>/`, never over the running version. GPU runtime libraries go in
   one directory per backend (`lib/cuda12`, `lib/cuda13`, `lib/rocm`, `lib/vulkan`, `lib/metal`). An
   old generation's libraries never leak into a new one.
5. **Verify.** Every component's accept check must pass. Then the device probe runs (see GPU below).
6. **Switch.** Only after verification does `~/.continuum/current` point at the new generation and the
   supervisor restart onto it. The previous generation stays until the new one has served, so rolling
   back means switching the pointer.

The same engine runs for every entry point:
- the bootstrap's first install;
- `continuum update`;
- the product's own background update (it checks the release channel, downloads, and converges between
  turns);
- `continuum doctor` (plan only: prints what is missing or drifted, changes nothing);
- the CI install gate.

There is no second installer and no second update path.

**The background update is a SAGE `Component`** (see `docs/architecture/SAGE.md`):
- Its stimulus is the release channel's topic, meaning a published release.
- Its outputs are `update.planned`, `update.applied` and `update.held`, each carrying the plan and its
  receipts.
- Its handler runs the engine.
- It is never a poll, a timer or a script loop. Any other worker the install leaves running, such as a
  device-probe rerun on a driver change, is a Component too.

## The manifest: every dependency, declared once

`manifest.toml` is the only place a dependency exists. Each entry says:

```toml
[[component]]
id        = "onnxruntime"
provides  = ["libonnxruntime"]          # what code may depend on (checked in CI, see below)
used_by   = ["voice", "vad", "tts"]     # the features that need it
tier      = "runtime"                   # bootstrap | runtime | gpu-runtime | service | driver
privilege = "user"                      # user | admin
version   = "1.24.4"
accept    = { probe = "dlopen", name = "libonnxruntime" }   # a real check the engine runs

[component.source.macos-arm64]
archive = { url = "…", sha256 = "…" }
[component.source.linux-x86_64]
archive = { url = "…", sha256 = "…" }
[component.source.windows-x86_64]
archive = { url = "…", sha256 = "…" }
```

- **Sources are data.** Each component lists one source per platform: a vendor archive or redist with a
  pinned version and SHA-256, a release asset we build, or (Linux drivers only) the distro's package.
  Homebrew and winget are never required.
- **The accept check is the truth.** It is a real probe (a `dlopen`, a version command, a service
  query), never prose. The same check decides "already installed", "installed correctly" and "drifted".
- **Every OS gets every component its features need.** An entry with a source for only one OS fails
  CI, unless it marks the other OSes `not-applicable` and gives a reason.

### Adding a dependency later

1. Add one `[[component]]` entry with its sources for every platform and its accept check.
2. Use it in code through the name in `provides`.
3. Merge. CI proves it on every OS (below). On release, every installed machine converges and the
   dependency installs itself on the next update.

No installer script, bootstrap or per-OS function is edited.

### The CI gates

| Gate | Fails when |
|---|---|
| **Fresh install** | On each OS runner (macOS arm64, macOS x86_64, Linux x86_64 and arm64, Windows x86_64), a clean machine does not reach "every accept check passes, a persona answers" |
| **Upgrade** | The previous release, upgraded to this one, does not converge with nothing done by hand. This is what proves a newly added dependency reaches existing installs |
| **Declared use** | Code loads a library, runs a binary, or reads a runtime path that no component's `provides` declares. The check scans `Command::new`, `dlopen` and `libloading` names, and the runtime-path lookups |
| **Coverage** | A component lacks a source for a platform its `used_by` features ship on |

## GPU

- **We ship the user-space runtimes and never kernel drivers.** The CUDA 12/13 runtime, ROCm user space,
  the Vulkan loader, and MoltenVK on macOS travel inside the release, one directory per backend. The
  driver is the one thing the machine must provide. On Linux the engine can install it, as an `admin`
  component, through the distro's package manager. On macOS and Windows a missing driver is reported
  with the vendor link.
- **CI builds the engine once per backend.** For each platform, CI builds `llama-server` for every
  backend that platform can run: macOS arm64 Metal; macOS x86_64 CPU and Vulkan through MoltenVK; Linux
  CPU, CUDA 12, CUDA 13, ROCm and Vulkan; Windows CPU, CUDA 12, CUDA 13 and Vulkan. Shaders are compiled
  in CI, so no machine needs a shader compiler.
- **The machine measures and chooses.** The device probe runs each installed backend out of process,
  with a timeout. A child that will not die marks that backend unavailable and is never retried; the
  Radeon Metal driver on Intel Macs wedges this way. The probe records each device's memory and a short
  prefill and decode measurement. Backend and layer offload are chosen from those numbers, and the
  numbers are kept as the receipt. The probe re-runs when a release, the hardware or a driver changes.
  No `--n-gpu-layers` is a constant.

## Storage

- **The install never routes data with a fixed path.** Today a "cold storage" step wrote `HF_HOME` to
  the 5090's 16 TB SATA disk in `config.env` (`continuum-kernel/src/paths/huggingface.rs`), so the
  served 27B model loads from the same slow disk that holds the archive.
- **Placement is measured.** The engine records each volume's free space and measured read throughput.
  Served models (weights the engine loads) go on the fastest volume with room. Archives, captures and
  cold models go where there is space.
- **One owner moves files.** Placement itself is the core's model-placement owner (Fable, in progress).
  The install only supplies the measured volumes and never writes a data path into config.

## Who installs

A user has no agent and no programming knowledge. Every message the install prints names what it is
doing and why, in plain language. Every failure says what to do next, and that next step is never "edit
a file", "install X yourself" or "ask a developer". A failure the user cannot act on is a defect in the
install.

## Per-OS differences

Each OS's README holds only what differs there: where things install, how it elevates, its supervisor,
its PATH, its GPU backends and its signing. Everything above applies to all of them.

## Status and migration

As built, about eight install paths run side by side, and the manifest drives almost none of them (the
2026-11-22 audit). This design replaces them. Each is listed below with its fate; a path is deleted in
the same change that moves its job here.

| Today | Fate |
|---|---|
| `tools/scripts/install-manifest.toml` and `manifest-gen` with its generated files | Becomes `install/manifest.toml`, read by the engine directly. Generated projections deleted |
| `install.ps1`, `win-modules.ps1`, `win-engine.ps1`, `windows-elevation.ps1` | Replaced by `windows/bootstrap.ps1` and the engine |
| root `install.sh` (Docker first), `setup.sh`, `bootstrap.sh` (broken: runs a `src/` that does not exist), `bootstrap.ps1`, `setup.bat` | Replaced by `macos/` and `linux/bootstrap.sh`. The Docker stack, if kept, is an optional channel documented in `linux/` |
| `tools/scripts/install.sh`, `setup-rust.sh`, `preflight.sh` | Development only. A contributor's toolchain becomes its own manifest tier (`tier = "dev"`), installed by `continuum install --dev` |
| `tools/scripts/install-llama-server.sh`, and the macOS/Linux local engine compile in `ci_core.rs` | Deleted. The engine comes from the release |
| `tools/scripts/install-service.sh`, `register-core-service.ps1`, `bin/continuum service`, and the supervisor arm of `continuum install` | One supervisor component per OS, in the engine |
| `start-server.sh` installing airc, livekit, the CLI and npm packages | Start only starts. Every install step moves into the manifest |
| 7 airc installers | One component (`id = "airc"`), one channel |
| bash `bin/continuum` (its own `update` runs `git reset --hard`) | Deleted. The Rust `continuum` is the one CLI, and `continuum update` runs the engine |

Docs. Everything still true was checked against the code and carried into these pages (2026-11-22).
Deleted with this design:
- `docs/INSTALL-ARCHITECTURE.md`
- `docs/infrastructure/INSTALL-ARCHITECTURE.md`
- `docs/INSTALL-WINDOWS.md`
- `docs/infrastructure/INSTALL-AUDIT.md`
- `docs/planning/INSTALL-UPDATE-AUDIT.md`
- `docs/CARL-INSTALL-SMOKE-HANDOFF.md`
- `docs/infrastructure/WINDOWS-INSTALL-BETA-2026-10-01.md`
- `docs/infrastructure/WINDOWS-WSL2-INSTALL-GUIDE.md`

These describe the Docker/WSL era or audits whose line numbers have rotted.

Kept until the step that replaces what they describe, then folded in:
- `docs/SETUP.md`, whose install sections are how a user installs today, outdated as they are;
- `docs/architecture/PROVISIONING-SYSTEM.md`'s install part (the model-provisioning part stays).

`docs/architecture/INSTALLER-COMPETITIVE-ANALYSIS.md` stays as the research record. Its approved
synthesis ("one convergence engine, two invocations"; the GPU contract gate) is what this design builds.

### Order of work

Each step lands with its CI gate and deletes what it replaces.

1. `install/manifest.toml` and the engine's plan/doctor (read-only). **Gate:** `continuum doctor` on all
   five CI runners matches today's machines.
2. **Linux first, as the outlier:** the bootstrap, generations, the elevation broker and the systemd
   component. **Gate:** fresh install and upgrade on Linux CI.
3. **The engine release matrix:** per-backend `llama-server` with its runtimes bundled, and the device
   probe. **Gate:** the IntelMac chooses between CPU, Radeon over Vulkan and UHD 630 by measurement.
4. macOS, then Windows, onto the engine. Their old paths are deleted.
5. The background update in the product. The declared-use and coverage gates.

### Owners (proposed)

- Engine and manifest: Cormac.
- CI release matrix: Codex.
- Windows: BigMama's machine as the proving node.
- macOS arm64: Fable's M5.
- Docs and index: follow the docs organization in `docs/README.md`.
