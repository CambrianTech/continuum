# Continuum on AWS — headless container deployment

The core is Rust in a container; there is **no Node on the runtime path**.
`package.json` at the repo root is a developer convenience (client builds, dev
scripts) — every load-bearing entry point is bash + cargo
(`tools/scripts/start-server.sh`, `install.sh`, `install-service.sh`).

## One-shot launch

Paste `ec2-user-data.sh` as EC2 **user data** on a fresh instance (or run it
with sudo). It branches per distro — Amazon Linux 2023 via `dnf` (+ the compose
plugin from Docker releases, since get.docker.com does not support AL2023),
Ubuntu via get.docker.com — clones the repo to `/opt/continuum`, seeds
`~/.continuum/config.env`, and brings the stack up with the CUDA overlay iff
`nvidia-smi` sees a GPU.

## Image matrix (ghcr.io/cambriantech)

| image | base | for |
|---|---|---|
| `continuum-core` | ubuntu 24.04 (glibc) | CPU serving — amd64 + arm64 (Graviton) |
| `continuum-core-vulkan` | ubuntu 24.04 | consumer GPUs (Linux desktop / WSL2) |
| `continuum-core-cuda` | ubuntu 24.04 + CUDA | g5/g6-class NVIDIA instances |

Images are built on dev machines and pushed via `scripts/push-current-arch.sh`;
CI **verifies** registry coverage, it never builds (see
`.github/workflows/docker-images.yml`).

### Why the bases are glibc, not Alpine

CUDA and Vulkan userlands, ONNX Runtime (`load-dynamic-ort`), and the GPU
llama.cpp backends are all glibc-oriented; Alpine/musl is a non-starter for the
GPU variants. The right small-image play is a **CPU-only static-musl slim
variant** (llama.cpp CPU + candle CPU compile under musl) producing a
scratch-sized image for cheap fleet instances — planned as
`continuum-core-slim`, tracked in the deploy backlog. Until then the CPU image
on AL2023/Ubuntu hosts is the common form; the *host* distro doesn't matter,
only the container base does.

## Instance guidance (2026)

- **CPU fleet**: c7g/m7g (Graviton — arm64 image exists and is cheaper per
  vCPU) or c7i/m7i. 16GB+ RAM for a 7B Q4 lane; 32GB for Devstral-class 24B.
- **GPU fleet**: g5 (A10G 24GB) runs Devstral 24B Q4 comfortably; g6/L4 for
  smaller lanes. Use the cuda overlay.
- **Disk**: 100GB gp3 minimum — model weights dominate. Mount `~/.continuum`
  on the data volume if you separate root/data.

## Operational notes

- The stack self-provisions under `~/.continuum`
  ([[managed-product-everything-self-provisions-no-operator-steps]]): config,
  model cache, logs, sockets, sessions. Back that directory up; the containers
  are disposable.
- Restart policy is `unless-stopped`; the host needs no systemd unit for the
  containerized form. (`npm run service:install` / `install-service.sh` is the
  NATIVE-host path — macOS LaunchAgent / Linux systemd — not needed on EC2.)
- Text-mode personas run with no extra profile. Voice/video needs
  `--profile live` (LiveKit services) and correspondingly larger instances.
- airc mesh across instances: each instance is a peer; rendezvous via GitHub
  (`gh` auth) or direct LAN/VPC routes. Same-VPC peers converge without any
  GitHub dependency.
