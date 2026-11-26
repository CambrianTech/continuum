# macOS: only the differences

Everything in [the install README](../README.md) applies. This page covers only what differs on macOS.

| | macOS |
|---|---|
| Platforms | `macos-arm64` (Apple Silicon), `macos-x86_64` (Intel) |
| Install root | `~/.continuum` (generations, libraries, models); CLI at `~/.continuum/bin`, linked into `~/.local/bin` |
| Bootstrap | `bootstrap.sh`: `uname -m` picks the binary, `shasum -a 256` verifies it. Needs only `curl` and `shasum`, which every macOS has |
| Signed channel | A notarized `.pkg` that runs the same engine (no Gatekeeper prompt on a downloaded binary) |
| Elevation | One authorization, and only for the system LaunchDaemon (see Supervisor). A per-user LaunchAgent install needs none |
| Supervisor | `system/com.continuum.core` LaunchDaemon. It runs at boot without a login, the way the residents need it to. The LaunchAgent (`--user`) is offered for machines that refuse daemons |
| PATH | Appends `~/.local/bin` to the user's shell profile (zsh by default) and says so |
| Build tools | None for users. Xcode Command Line Tools are a `tier = "dev"` component, for contributors only |

## GPU

| Platform | Backends shipped | Notes |
|---|---|---|
| macos-arm64 | Metal | Unified memory. Offload is measured, normally all layers |
| macos-x86_64 | CPU (Accelerate BLAS), Vulkan through MoltenVK | The release bundles MoltenVK and the Vulkan loader (`lib/vulkan`), and their shaders are compiled in CI. Measured 2026-10-31: the Radeon 560X wedges under the Metal backend (unkillable at first allocation), so Metal is never probed on Intel Macs. The UHD 630 under Metal was slower than the CPU. The probe decides between CPU, the Radeon over Vulkan, and the UHD 630 over Vulkan |

## Differences in the update

- **The switch.** The background update switches `~/.continuum/current`, stops the core through its own
  graceful stop, then runs a plain `launchctl kickstart system/com.continuum.core`. The owning user may
  start a stopped system job, but `kickstart -k` is refused ("Operation not permitted", the M5,
  2026-10-03; `core/continuum-cli-lifecycle/src/launchd.rs:624-631`). The plist changes only when its
  contents change, which is the one case that needs the authorization again.
- **Launch constraints.** Background Task Management pins the job to the code hash of the binary it last
  ran. The first spawn of a new ad-hoc-signed build is killed with `OS_REASON_CODESIGNING`, and the next
  spawn re-pins and runs. launchd will not make that second spawn itself (`KeepAlive{Crashed}`), so the
  update triggers it (`launchd.rs:160-170`). Developer ID signing of release binaries removes this, and
  is required.
- **No refused builds left behind.** Today each refused build is kept as `<slot>.failed-<ms>` (about
  179 MB each) and never deleted (`launchd.rs:240-251`). Generations replace that: a refused generation
  is removed once the previous one is serving again.
