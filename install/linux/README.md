# Linux: only the differences

Everything in [the install README](../README.md) applies. This page covers only what differs on Linux.

| | Linux |
|---|---|
| Platforms | `linux-x86_64`, `linux-arm64`; glibc. musl is detected and refused with a message until a musl build exists |
| Install root | `~/.continuum`; CLI linked into `~/.local/bin` |
| Bootstrap | `bootstrap.sh`: `uname -m` and an ELF libc check pick the binary, `sha256sum` verifies it. Needs only `curl` (or `wget`) and `sha256sum`. Detects WSL2 and refuses WSL1 |
| Elevation | One `sudo`, only when the plan holds `admin` components: the system unit, `loginctl enable-linger`, or a GPU driver. The engine lists them first. Running as root skips the prompt |
| Supervisor | A `systemd --user` unit plus linger (one sudo, once), so the core runs without a login session. A system unit is offered for servers. Without systemd (some containers, and WSL2 with systemd off) the engine says so and runs the core in the foreground |
| Distro packages | Used only for drivers: `apt` (Debian/Ubuntu) and `dnf`/`yum` (Fedora/RHEL family). Never for anything we can ship |

## GPU

| Vendor | What the release ships | What the machine needs |
|---|---|---|
| NVIDIA | CUDA 12 and CUDA 13 runtime libraries (`lib/cuda12`, `lib/cuda13`) | A driver. If `nvidia-smi` is missing, the engine offers the distro's driver package as an `admin` component (as Ollama's installer does) |
| AMD | ROCm user space (`lib/rocm`) and Vulkan | The kernel driver (amdgpu, in the mainline kernel). ROCm-capable or any Vulkan-capable card |
| Intel / other | Vulkan (`lib/vulkan`) | Mesa's Vulkan driver. The engine offers `mesa-vulkan-drivers` when missing |
| None | CPU | Nothing |

On WSL2 the GPU is the Windows driver passed through: the engine never installs a driver inside WSL.
