# Docker Node Architecture

> **One `docker compose up` = one Grid node.**

---

## 1. Overview

Every Grid node is a set of Docker containers managed by a single `docker-compose.yml`. The compose file IS the node definition. No manual installs, no dependency hell, no "which Python has the right torch."

```
┌─── Grid Node (any machine) ──────────────────────────────────────┐
│                                                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────┐  │
│  │ continuum-   │  │ node-server  │  │ widget-server         │  │
│  │ core (Rust)  │  │ (TypeScript) │  │ (optional — UI)       │  │
│  │ 2GB          │  │ 1GB          │  │ 512MB                 │  │
│  └──────┬───────┘  └──────┬───────┘  └───────────────────────┘  │
│         │ unix socket      │ websocket                           │
│  ┌──────┴───────┐  ┌──────┴───────┐  ┌───────────────────────┐  │
│  │ postgres     │  │ Ares         │  │ livekit + tls-proxy   │  │
│  │ 512MB        │  │ (heartbeat)  │  │ 384MB                 │  │
│  └──────────────┘  └──────────────┘  └───────────────────────┘  │
│                                                                   │
│  ── GPU Profile (--profile gpu) ──────────────────────────────── │
│  ┌──────────────┐  ┌──────────────┐                              │
│  │ forge-worker │  │ inference    │                              │
│  │ (sentinel-ai)│  │ (llama.cpp)  │                              │
│  │ 28GB GPU cap │  │ 8GB GPU      │                              │
│  └──────────────┘  └──────────────┘                              │
└───────────────────────────────────────────────────────────────────┘
```

---

## 2. Node Profiles

Docker Compose profiles control what runs on each machine. No manual decisions — Ares (see [ARES-KERNEL.md](ARES-KERNEL.md)) auto-detects hardware and selects the right profile.

### Mac / Laptop Node (`docker compose up`)

| Service | Memory | Purpose |
|---------|--------|---------|
| postgres | 512MB | Data store |
| continuum-core | 2GB | Rust: Candle inference, embeddings, search, IPC |
| node-server | 1GB | TypeScript: commands, daemons, WebSocket |
| widget-server | 512MB | Browser UI (optional, `--profile ui`) |
| livekit + tls | 384MB | WebRTC voice/video |
| **Total** | **~4.4GB** | |

Inference runs through Candle inside continuum-core. No separate inference container. Metal acceleration on Apple Silicon is native (not Docker — Candle compiles with Metal support in the Rust binary).

### GPU Node (`docker compose --profile gpu up`)

| Service | Memory | GPU VRAM | Purpose |
|---------|--------|----------|---------|
| All base services | 4.4GB RAM | — | Same as Mac node |
| forge-worker | 28GB cap | Up to 32GB | sentinel-ai alloy executor |
| inference | 8GB cap | Shared | llama.cpp server for large models |
| **Total** | **~4.4GB RAM** | **Up to 32GB VRAM** | |

The forge-worker and inference containers share the GPU. Docker's NVIDIA runtime handles isolation. Memory caps prevent one from starving the other.

### Headless Node (`docker compose --profile headless up`)

No widget-server. No LiveKit. Just continuum-core + node-server + postgres. For dedicated forge/inference towers that don't need a UI.

---

## 3. Resource Controls

Docker gives us what native processes can't: hard limits.

```yaml
# forge-worker can't eat more than 28GB
mem_limit: 28g
deploy:
  resources:
    limits:
      memory: 28g
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
```

### Why This Matters (The CUDA Incident)

On 2026-04-02, forge jobs failed on BigMama for days because system Python had the wrong torch version (missing sm_120 kernels for RTX 5090). Nobody knew until manual SSH investigation.

In the Docker model:
- The forge-worker Dockerfile pins `torch==2.11.0+cu128` with sm_120 support
- It can't use the wrong system Python — it has its own venv
- If it crashes, Ares detects the failure and posts to #factory
- Container restarts with the same known-good environment

Dependency pinning eliminates an entire class of "works on my machine" failures.

---

## 4. Shared Volumes

Containers communicate through shared Docker volumes, not filesystem paths.

| Volume | Mounted In | Purpose |
|--------|-----------|---------|
| `ipc-sockets` | continuum-core, node-server | Unix socket IPC between Rust and TS |
| `models` | inference, forge-worker | Forged GGUF models for serving |
| `forge-output` | forge-worker, inference | Training output (adapters, checkpoints) |
| `voice-models` | model-init, continuum-core | Whisper, TTS, avatar models |
| `hf-cache` | forge-worker | HuggingFace model cache (avoids re-download) |
| `pgdata` | postgres | Database persistence across restarts |

### Model Flow

```
forge-worker trains → writes to forge-output volume
  → post-processing creates GGUF in models volume
    → inference container picks up new model
      → continuum-core routes inference requests to it
```

---

## 5. Health Checks and Recovery

Every container has a health check. Docker restarts failed containers automatically.

```yaml
# continuum-core: check for Unix socket
healthcheck:
  test: ["CMD", "test", "-S", "/root/.continuum/sockets/continuum-core.sock"]
  interval: 5s

# postgres: check for readiness
healthcheck:
  test: ["CMD-SHELL", "pg_isready -U continuum"]
  interval: 5s
```

Dependency chains ensure correct startup order:
```
postgres (healthy) → continuum-core (healthy) → node-server → widget-server
```

Ares monitors container health at a higher level — see [ARES-KERNEL.md](ARES-KERNEL.md).

---

## 6. Building Images

### On the Target Machine (Preferred)

```bash
# BigMama (x86_64, native build)
docker compose build
docker compose --profile gpu up
```

### Pre-built from GHCR (Production)

```bash
# Any machine — pull pre-built images
docker compose pull
docker compose up
```

### Cross-Platform

The continuum-core Dockerfile uses multi-stage builds with cargo-chef for dependency caching. The dependency layer (~30 min) only rebuilds when Cargo.toml/Cargo.lock change. Source changes rebuild in ~2-3 minutes.

LiveKit WebRTC is excluded from Docker builds (`--no-default-features`) because the C++ dependency doesn't cross-compile cleanly. LiveKit runs as its own container instead.

---

## 7. Networking

### Internal (Container-to-Container)

Containers communicate over Docker's internal network. No ports exposed except what's needed for external access.

- continuum-core ↔ node-server: Unix socket via `ipc-sockets` volume
- node-server ↔ postgres: Docker DNS (`postgres:5432`)
- widget-server → node-server: Docker DNS for WebSocket

### External (Tailscale Mesh)

Exposed ports for Grid communication:

| Port | Protocol | Service | Purpose |
|------|----------|---------|---------|
| 9000 | HTTPS | node-server | API + commands |
| 9001 | WSS | node-server | WebSocket (real-time) |
| 9003 | HTTP(S) | widget-server | Browser UI |
| 7443 | WSS | livekit-tls | WebRTC signalling |
| 7881 | TCP | livekit | WebRTC TCP |
| 7882 | UDP | livekit | WebRTC UDP |
| 8090 | HTTP | inference | llama.cpp API (GPU nodes) |

Tailscale handles encrypted routing between nodes. No manual TLS certificate management — Tailscale certs auto-provision via `tailscale cert`.

### Access Pattern

```
Mac browser → https://bigmama.tailnet:9003 → widget-server container
  → wss://bigmama.tailnet:9001 → node-server container
    → unix socket → continuum-core container
      → GPU → forge-worker / inference containers
```

---

## 8. Auto-Discovery (Future — Ares Integration)

Currently, `--profile gpu` is a manual flag. The target architecture:

```
1. Ares starts as first container (always)
2. Ares probes hardware:
   - nvidia-smi → GPU model, VRAM, compute capability
   - /proc/meminfo → RAM
   - df → disk space
   - uname → architecture
3. Ares selects profile:
   - Has NVIDIA GPU with >8GB VRAM → gpu profile
   - Has >16GB RAM but no GPU → inference-cpu profile
   - Otherwise → base profile
4. Ares starts remaining containers with selected profile
5. Ares registers node capabilities with grid mesh
6. Grid routes work to capable nodes
```

This eliminates manual node configuration. Plug in a machine, install Docker, `docker compose up`. Ares figures out the rest.

---

## 9. Relationship to Other Docs

| Document | Relationship |
|----------|-------------|
| [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) | Parent — this doc details the Docker implementation of Section 2.4 |
| [ARES-KERNEL.md](ARES-KERNEL.md) | Ares is the watchdog + auto-discovery layer above Docker |
| [RETICULUM-TRANSPORT.md](RETICULUM-TRANSPORT.md) | How nodes communicate over the mesh |
| [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md) | Per-node resource management within containers |

---

## 10. Implementation Phases

### Phase 1: Static Docker (Current — `feature/docker-tls-infrastructure`)
- [x] Dockerfiles for all services (continuum-core, node-server, widget-server, model-init)
- [x] docker-compose.yml with profiles (gpu, base)
- [x] GPU passthrough for forge-worker
- [x] Health checks and dependency ordering
- [x] Memory limits on all containers
- [x] Shared volumes for IPC and models
- [ ] GHCR image publishing (CI workflow)
- [ ] E2E test: `docker compose up` on BigMama, browser from Mac

### Phase 2: Ares Watchdog
- [ ] Ares as lightweight container/native binary
- [ ] Hardware auto-detection (GPU, RAM, disk)
- [ ] Container health monitoring beyond Docker health checks
- [ ] Log scanning for known failure patterns (CUDA errors, OOM, etc.)
- [ ] Event emission to grid mesh on failures
- [ ] Auto-restart with adjusted parameters (reduce batch size on OOM)

### Phase 3: Dynamic Profiles
- [ ] Ares selects Docker profile based on hardware
- [ ] Node self-registration with grid
- [ ] Capability advertisement (GPU model, VRAM, available disk)
- [ ] Heartbeat to mesh (30s interval, stale after 3 misses)

### Phase 4: Job Leases
- [ ] Nodes take leases on forge jobs ("I'll run this for 30 min")
- [ ] Lease renewal on heartbeat
- [ ] Expired lease → job returns to queue
- [ ] Checkpoint-based resume (forge saves every N steps)
- [ ] Multi-node job distribution

### Phase 5: Self-Healing
- [ ] Ares diagnoses common failures from container logs
- [ ] Auto-fix: dependency updates, config corrections
- [ ] Escalation: post diagnosis to #factory/#grid room when can't self-heal
- [ ] Foreman PersonaUser consumes Ares events, makes strategic decisions
- [ ] Plant Manager coordinates across multiple Ares instances
