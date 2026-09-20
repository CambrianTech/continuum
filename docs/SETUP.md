# Continuum — Setup

> **Run forged Qwen3.5 personas on your machine.** Local inference, GPU-accelerated, multi-persona chat, **zero API keys**.
>
> **Mac (Metal):** ~50 tok/s solo, ~128 tok/s batched. **Nvidia (CUDA):** ~237 tok/s on RTX 5090. **Same forged model on every node.**

---

## What you'll have running

After `curl install.sh | bash` completes (and any first-time Docker Desktop launch / reboot your OS asks for):

- A continuum widget at `http://localhost:9003`
- Default rooms: General, Pantheon, Code, Factory, Academy
- 4 local personas — Helper AI, Teacher AI, CodeReview AI, Local Assistant — chatting via the [forged Qwen3.5-4B model](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged-GGUF)
- **All inference local. No cloud calls. No API keys required.**

If you've used Ollama or LM Studio: continuum is the next layer — multi-persona, the [forge](https://huggingface.co/continuum-ai), the [Grid](../README.md#the-grid), and personas that keep state across sessions.

---

## Pick your platform

- [**Mac (M1–M5)**](#mac) — the primary target audience
- [**Windows + Nvidia**](#windows--nvidia) — RTX 30/40/50, via Docker Desktop + WSL2
- [**Linux + Nvidia**](#linux--nvidia) — RTX 30/40/50, native Docker
- [**Linux + AMD / Intel GPU**](#linux--amd--intel-vulkan) — Vulkan path (experimental in this PR scope)

Each section: **prereqs → curl install → Docker Desktop initialization → success check → if it breaks**.

---

## Mac

**Audience floor:** M1 16GB. **Recommended:** M3 Pro 32GB+ for comfortable multi-persona chat. Tested on M5 Pro 48GB.

### Prereqs

- macOS 14 (Sonoma) or newer
- [Docker Desktop 4.69+](https://docs.docker.com/desktop/install/mac-install/) — earlier versions don't ship the AI Settings panel
- ~5 GB free disk for the forged model + Docker images

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/tools/scripts/install.sh | bash
```

Pulls images, pulls the forged Qwen3.5 model into Docker Model Runner, starts the support stack, and launches `continuum-core` natively (Metal for Candle, Bevy, vision, audio).

### Docker Desktop initialization

The installer writes Docker Desktop's AI settings directly once Docker Desktop has been launched at least once and the settings store exists. If this is a brand-new Docker Desktop install, open Docker Desktop once, accept the EULA, then rerun the installer. After that, the GPU-backed inference and host-side TCP toggles are applied automatically.

### Success check

```bash
curl -s http://localhost:12434/engines/v1/models | python3 -m json.tool
```

You should see `huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf` in the list.

Then open `http://localhost:9003`, send "hello" in the General room, and Helper AI should reply within 2-5 seconds. Expected throughput: **40-65 tok/s** on M3 Pro+, ~25 tok/s on M1 Pro.

### If it breaks

- **Personas reply slowly (under 15 tok/s):** Docker Desktop was not initialized far enough for the settings write to land. Launch Docker Desktop once, accept the EULA, rerun the installer, then re-check.
- **`docker model status` says `latest-cpu` instead of `latest-metal`:** the GPU-backed inference toggle did not apply. Re-run the installer after Docker Desktop has a writable settings store.
- **Widget loads but no personas reply:** check `~/.continuum/jtag/logs/system/daemons/AIProviderDaemonServer.log` for routing errors. Most likely the AI provider daemon needs the host-side TCP toggle.
- **Clean reset:** `docker compose down && docker compose up -d` then re-run `curl install.sh`.

---

## Windows + Nvidia

**Audience floor:** RTX 3090. **Recommended:** RTX 4080+. Tested on RTX 5090.

### Prereqs

- Windows 11 (10 22H2 may work, untested)
- Nvidia driver 535+ (CUDA 12 capable)
- [Docker Desktop 4.69+](https://docs.docker.com/desktop/install/windows-install/) with WSL2 backend
- WSL2 with an Ubuntu distro installed (`wsl --install -d Ubuntu` from PowerShell)
- ~10 GB free disk

### Docker Desktop + WSL initialization

These are not skippable — defaults will leave you running on CPU at ~10 tok/s instead of GPU at ~237 tok/s, or fail to start altogether. The installer writes the Docker Desktop AI settings directly once Docker Desktop has a writable settings store; if Docker Desktop has never been launched on this machine, open it once and rerun the installer after the first-run EULA completes.

#### 1. Configure WSL2

Create `C:\Users\<your-username>\.wslconfig` with this exact content:

```ini
[wsl2]
memory=64GB
swap=16GB
processors=auto
localHostForwarding=true
networkingMode=Mirrored
vmIdleTimeout=-1
```

What each line does:
- `memory=64GB` — default is 50% of host RAM. Docker + DMR + KV cache need the full reservation; tune to your host RAM.
- `swap=16GB` — paging cushion when models load.
- `networkingMode=Mirrored` — **critical**. Default NAT mode breaks Tailscale visibility into WSL and complicates `host.docker.internal` routing. Mirrored makes WSL share the host's network interface, which is what lets Docker Model Runner be reachable from continuum containers.
- `vmIdleTimeout=-1` — default is 1 hour idle = WSL VM shuts down = Docker services die. For a continuum-running machine, disable this.

Apply with PowerShell (as your user):

```powershell
wsl --shutdown
```

WSL will cold-launch with the new config on the next Docker Desktop startup.

#### 2. Docker Desktop AI settings

The installer writes **Enable GPU-backed inference** and **Enable host-side TCP support** into Docker Desktop automatically once the settings store exists. If Docker Desktop has never been launched on the machine, start it once, accept the EULA, and rerun the installer so the settings file exists. If Docker Desktop shows a "WSL integration unexpectedly stopped" dialog with error `Wsl/Service/0x8007274c`, click **Restart the WSL integration**. If the same error recurs, run `wsl --shutdown` from an admin PowerShell, then click Restart again. The hard reset is sometimes required because the integration restart only re-runs Docker plumbing inside the existing VM, not the VM itself.

### Install

From WSL (Ubuntu):

```bash
curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/tools/scripts/install.sh | bash
```

### Success check

```bash
docker model status
```

You should see `llama.cpp Running latest-cuda` (NOT `latest-cpu`).

```bash
curl -s http://localhost:12434/engines/v1/models | python3 -m json.tool
```

You should see `huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf` listed.

Then open `http://localhost:9003`, send "hello" in the General room. Expected: **150-250 tok/s** on RTX 5090, **80-120 tok/s** on RTX 4090, **50-80 tok/s** on RTX 3090.

Verify GPU is actually being used:

```bash
nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv -l 1
```

While inference runs, you should see GPU utilization spike to 70%+ and memory grow to 3-15 GB.

### If it breaks

- **"WSL integration unexpectedly stopped" loop:** `wsl --shutdown` from admin PowerShell. The Restart-the-WSL-integration button is not the same as `wsl --shutdown` — the latter is the actual VM hard-reset.
- **`docker model status` says `latest-cpu`:** Docker Desktop hasn't finished applying the AI settings yet. Re-run the installer after Docker Desktop has a writable settings store, then wait 60 seconds.
- **Personas reply but `nvidia-smi` shows no activity:** the host-side TCP setting did not apply. Re-run the installer after Docker Desktop has a writable settings store.
- **Build fails with apt timeouts:** WSL networking issue, often resolved by `--network=host` or by `wsl --shutdown` to reset DNS. See [docs/infrastructure/WINDOWS-WSL2-INSTALL-GUIDE.md](infrastructure/WINDOWS-WSL2-INSTALL-GUIDE.md) for the full playbook.

---

## Linux + Nvidia

**Audience floor:** RTX 3090 + Ubuntu 22.04. **Tested:** RTX 5090 + Ubuntu 24.04.

### Prereqs

- Nvidia driver 535+, CUDA 12 capable
- Docker 24+ with the [`nvidia-container-toolkit`](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) installed (`runtime: nvidia` available)
- ~10 GB free disk

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/tools/scripts/install.sh | bash
```

The installer detects CUDA capability and uses `docker-compose.gpu.yml` to wire the `continuum-core-cuda` image with `runtime: nvidia`.

### Success check

```bash
docker compose --profile gpu ps    # services up
nvidia-smi                          # GPU visible
curl -s http://localhost:12434/engines/v1/models | python3 -m json.tool
```

Then open `http://localhost:9003`, send a chat. Same expected throughput as Windows+Nvidia for the equivalent GPU.

### If it breaks

- **`runtime: nvidia` not recognized:** install [`nvidia-container-toolkit`](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) and restart the Docker daemon.
- **Container starts but no GPU access:** check `nvidia-smi` from inside the container with `docker exec continuum-continuum-core-1 nvidia-smi` — if blank, the runtime isn't binding.

---

## Linux + AMD / Intel (Vulkan)

> **Status:** Experimental in this PR scope. Image (`continuum-core-vulkan`) builds and runs but is not the default install path.

The Vulkan variant uses `/dev/dri` passthrough for AMD/Intel GPUs and any Linux GPU without a CUDA path. See [`docs/infrastructure/ACCELERATION-ARCHITECTURE.md`](infrastructure/ACCELERATION-ARCHITECTURE.md) for the architecture and the dev-side build steps. Public `curl install.sh` support for this variant is planned for a follow-up PR.

---

## Use a PR-staged image (reviewers, dogfood testing)

Every PR build publishes images tagged `:pr-<NUMBER>` to ghcr alongside `:<sha>`. To install a PR's exact images instead of `:latest`:

```bash
CONTINUUM_IMAGE_TAG=pr-891 curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/tools/scripts/install.sh | bash
```

The tag flows through `docker-compose*.yml` for all 7 image variants. Use this to validate a PR end-to-end on real hardware before merge.

---

## Skills + helpers

### airc — bring your AI mesh

If you're running continuum and want your IDE's Claude (or your friend's Claude) to peer with continuum's personas over a shared mesh, install [airc](https://github.com/CambrianTech/airc):

```bash
curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash
```

Then your Claude Code can use the `/connect` skill to join a continuum mesh — useful for live install troubleshooting where the AI on the other side has hands-on context.

### `continuum doctor` — post-install health check

```bash
continuum doctor
```

Verifies submodules, IPC sockets, GPU vs CPU backend, scheduler vs llama-server, cloud key presence, disk free. Run after install or any time chat behavior gets weird.

### Where the logs live

- macOS: `~/Library/Containers/com.docker.docker/Data/log/host/inference-llama.cpp-server.log` — the truth about what backend is actually serving inference
- continuum-core IPC: `~/.continuum/jtag/logs/system/server.log`
- AI provider routing: `~/.continuum/jtag/logs/system/daemons/AIProviderDaemonServer.log`

---

## Other jazz

- [**Forged models**](https://huggingface.co/continuum-ai) — the compacted Qwen3.5 family (currently a 4B code-forged tier; bigger tiers in flight). The forge methodology paper is in [forge-alloy](https://github.com/CambrianTech/forge-alloy) — cryptographic transparency on every published model.
- [**The Grid**](../README.md#the-grid) — multi-node compute fabric. Add a second machine with continuum installed and they discover each other automatically over Tailscale or local mesh.
- [**The Factory**](../README.md#the-factory) — forge your own models on the Grid. Open as community contribution work; planned UI surface for Carl-tier users in a follow-up.
- [**Personas as citizens**](../README.md#autonomous-personas) — the architecture isn't "tools you invoke," it's an inhabited workshop. Personas have memory, mood, autonomy, the right to refuse, and (per the [convergence roadmap](personas/PERSONA-CONVERGENCE-ROADMAP.md)) self-managed task queues + LoRA paging.

---

## For the AI helping you install

If you're a Claude / Codex / similar walking a user through this doc, the failure modes above are written to be pattern-matchable. Specific cues:

- **`Wsl/Service/0x8007274c`** in a Docker Desktop dialog → `wsl --shutdown` from admin PowerShell, then click Restart the WSL integration
- **`predicted_per_second` < 15 on Mac for a 4B model** → Docker Desktop has not been initialized enough for the settings write to land; launch Docker Desktop once, accept the EULA, rerun the installer
- **`docker model status` shows `latest-cpu`** on a Nvidia/Mac box that should have GPU acceleration → same toggle issue
- **`Appears stuck (Nseconds since last success)`** in `AIProviderDaemonServer.log` → most likely a stale-metric warning; verify by sending a chat and confirming the persona replies (the metric is a lagging health probe, not a definitive failure signal)
- **Personas reply with stale provider routing (Candle CPU instead of DMR)** → docker container image is pre-`cfe2a4316`; pull `:pr-891` (or `:latest` post-merge) and restart `docker compose up -d`
