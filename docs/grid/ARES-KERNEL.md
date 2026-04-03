# Ares: The Grid Kernel

> **The minimum viable existence of a node on the mesh.**

---

## 1. What Is Ares

Ares is not a PersonaUser. It's below PersonaUsers. It's the heartbeat — the process that makes a node *exist* on the Grid. If every container crashes, if the database corrupts, if the forge OOMs, Ares is still running, still reporting, still accepting commands from the mesh.

Think of it as PID 1 for the distributed system. `systemd` for the Grid.

```
Layer Stack:
┌─────────────────────────────────────────────┐
│  PersonaUsers (Foreman, Teacher, Helper)    │  ← Citizens with moods, skills, energy
│  Continuum Core + Node Server               │  ← Application layer (Docker containers)
│  Docker Engine                              │  ← Container runtime
│  Ares                                       │  ← KERNEL: heartbeat, watchdog, shell
│  Tailscale                                  │  ← Network mesh
│  Hardware                                   │  ← CPU, GPU, RAM, disk
└─────────────────────────────────────────────┘
```

Ares sits between Docker and Tailscale. It monitors everything above it and communicates over everything below it.

---

## 2. Why Ares Exists

### The Problem (2026-04-02 Incident)

BigMama's forge jobs failed silently for days. The CUDA kernel mismatch (`sm_120` missing from torch) caused every alloy execution to crash. Nobody knew until a human SSH'd in and read the logs manually.

With Ares:
1. Forge-worker container crashes → Ares detects within 5 seconds
2. Ares reads container logs → pattern-matches "cudaErrorNoKernelImageForDevice"
3. Ares emits `grid:node:job-failed` with diagnosis to mesh
4. Event lands in #factory room → Foreman sees it, humans see it
5. Ares attempts auto-fix (if known pattern) or escalates

**The gap:** Docker restarts crashed containers, but it doesn't diagnose WHY they crashed or tell anyone about it. Ares bridges that gap.

### Design Constraints

- **< 50MB memory** — Ares must be tiny, always fits
- **No dependencies** — single static binary (Rust), no Python, no Node
- **Survives everything** — runs as systemd service, not a Docker container
- **Network-first** — communicates over Tailscale mesh, not local APIs
- **Read-only intelligence** — monitors and reports, never modifies application state

---

## 3. Core Responsibilities

```
Ares (per node, always on, <50MB)
├── heartbeat      — "I'm alive" to mesh every 30s
├── vitals         — CPU, GPU, RAM, disk, container status
├── watchdog       — detect crashed/unhealthy containers
├── log scanner    — pattern-match known failures in container logs
├── shell          — execute commands received from mesh
└── escalation     — post to mesh when it can't self-heal
```

### 3.1 Heartbeat

Every 30 seconds, Ares sends a heartbeat to all known mesh peers:

```json
{
  "node_id": "bigmama",
  "timestamp": 1775132506764,
  "uptime_seconds": 86400,
  "containers": {
    "continuum-core": "healthy",
    "node-server": "healthy",
    "postgres": "healthy",
    "forge-worker": "running",
    "inference": "stopped"
  },
  "vitals": {
    "cpu_percent": 12,
    "ram_used_gb": 8.2,
    "ram_total_gb": 64,
    "gpu_name": "RTX 5090",
    "gpu_vram_used_gb": 24.4,
    "gpu_vram_total_gb": 32.6,
    "gpu_utilization": 41,
    "disk_free_gb": 421
  },
  "active_jobs": ["job-1775042890104-378002"],
  "available_for": ["forge", "inference", "training"]
}
```

If a node misses 3 heartbeats (90 seconds), peers mark it as stale. Jobs leased to that node become eligible for reassignment.

### 3.2 Vitals Collection

Ares reads hardware state directly — no dependencies on application-layer tools.

| Source | What | How |
|--------|------|-----|
| `/proc/stat`, `/proc/meminfo` | CPU, RAM | Direct procfs read |
| `nvidia-smi` / NVML | GPU model, VRAM, utilization, temperature | Binary call or C API |
| `df` | Disk space | Standard utility |
| `docker ps --format json` | Container status | Docker CLI or API |
| Container logs | Error patterns | `docker logs --since` |

### 3.3 Watchdog

Beyond Docker's built-in health checks, Ares monitors at a semantic level:

| Check | Docker Does | Ares Does |
|-------|-------------|-----------|
| Container running | Yes | Yes |
| Health check passing | Yes | Yes |
| Container restarting in a loop | No | **Detects restart loops** (>3 restarts in 5 min) |
| OOM kills | No (just restarts) | **Reads dmesg for OOM, reduces limits** |
| GPU errors | No | **Reads container logs for CUDA errors** |
| Disk filling up | No | **Alerts at 90% full, pauses forge at 95%** |
| Network partition | No | **Detects when other Ares peers go silent** |

### 3.4 Log Scanner

Ares watches container logs for known failure patterns:

```rust
const PATTERNS: &[FailurePattern] = &[
    FailurePattern {
        regex: r"cudaErrorNoKernelImageForDevice",
        diagnosis: "PyTorch CUDA kernels don't match GPU compute capability",
        severity: Critical,
        auto_fix: Some(AutoFix::UpgradeTorch),
    },
    FailurePattern {
        regex: r"CUDA out of memory",
        diagnosis: "GPU VRAM exhausted during training/inference",
        severity: High,
        auto_fix: Some(AutoFix::ReduceBatchSize),
    },
    FailurePattern {
        regex: r"connection refused.*5432",
        diagnosis: "PostgreSQL not ready or crashed",
        severity: Critical,
        auto_fix: Some(AutoFix::RestartContainer("postgres")),
    },
    FailurePattern {
        regex: r"disk quota exceeded|No space left on device",
        diagnosis: "Disk full",
        severity: Critical,
        auto_fix: Some(AutoFix::PauseForge),
    },
];
```

When a pattern matches, Ares can either auto-fix (if the fix is safe) or escalate to the mesh.

### 3.5 Remote Shell

Ares accepts commands from authenticated mesh peers. This is how the Foreman (or a human) manages nodes without SSH.

```
Foreman → Tailscale mesh → Ares on BigMama
  "restart forge-worker"
  "docker compose --profile gpu up -d"
  "tail -50 forge-retry.log"
  "nvidia-smi"
```

Commands are:
- **Authenticated** — only from known Tailscale peers
- **Audited** — every command logged to `audit.jsonl`
- **Sandboxed** — allowlist of safe operations, not arbitrary shell

### 3.6 Escalation

When Ares can't self-heal, it posts to the mesh. The event follows the normal path into the room system:

```
Ares detects failure
  → emits grid:node:alert to mesh
    → Event lands in #factory or #grid room
      → Foreman PersonaUser reads via RAG
        → Foreman diagnoses, acts, or escalates to human
          → Human sees it in chat widget
```

The rooms ARE the monitoring layer. No separate dashboard. No Grafana. Events flow through the same pub/sub that powers chat.

---

## 4. Ares vs. Other Components

| Component | Layer | Purpose | Requires Docker | Requires App |
|-----------|-------|---------|-----------------|--------------|
| **Ares** | Kernel | Heartbeat, watchdog, shell | No (monitors Docker) | No |
| **Foreman** | PersonaUser | Factory intelligence, job strategy | Yes (runs in continuum-core) | Yes |
| **Plant Manager** | PersonaUser | Multi-node coordination | Yes | Yes |
| **Sentinel** | Pipeline engine | Multi-step job execution | Yes | Yes |

Ares runs when nothing else does. It's the last thing standing and the first thing to report.

### Relationship to Existing Ares Security Doc

The [ARES-MASTER-CONTROL.md](../ARES-MASTER-CONTROL.md) describes Ares as a security monitoring PersonaUser. That role is preserved — security monitoring becomes one capability of the Ares kernel. The PersonaUser "Ares" in chat is a higher-level consumer of the kernel Ares's events.

```
Ares Kernel (this doc)
  → emits security:threat-detected events
    → Ares PersonaUser (ARES-MASTER-CONTROL.md) reads via RAG
      → Posts analysis to I/O Tower room
```

The kernel does the watching. The PersonaUser does the thinking.

---

## 5. Implementation

### Language: Rust (Static Binary)

Ares compiles to a single static binary with no runtime dependencies. It can run on any Linux x86_64 or ARM64 system without installing anything.

```
~2MB binary, <50MB runtime memory
No Python. No Node. No JVM. No dependencies.
```

### Deployment: systemd Service

Ares runs as a systemd service, not a Docker container. It needs to survive Docker crashes and manage Docker itself.

```ini
[Unit]
Description=Ares Grid Kernel
After=network.target tailscaled.service
Wants=docker.service

[Service]
Type=simple
ExecStart=/usr/local/bin/ares --config /etc/ares/config.toml
Restart=always
RestartSec=5
MemoryMax=50M
# Ares must survive everything
OOMScoreAdjust=-900

[Install]
WantedBy=multi-user.target
```

### Configuration

```toml
[node]
id = "bigmama"
mesh_peers = ["100.x.x.x", "100.y.y.y"]

[heartbeat]
interval_secs = 30
stale_after_misses = 3

[watchdog]
restart_loop_threshold = 3
restart_loop_window_secs = 300
disk_warning_percent = 90
disk_critical_percent = 95

[log_scanner]
containers = ["forge-worker", "continuum-core", "node-server"]
scan_interval_secs = 10
lookback_secs = 60

[shell]
allowed_commands = [
    "docker compose *",
    "docker logs *",
    "docker restart *",
    "nvidia-smi",
    "df -h",
    "tail * .log",
]
```

---

## 6. Event Types

Events Ares emits to the mesh:

| Event | When | Severity |
|-------|------|----------|
| `grid:node:heartbeat` | Every 30s | Info |
| `grid:node:online` | First heartbeat after boot | Info |
| `grid:node:offline` | Detected by peers (missed heartbeats) | Warning |
| `grid:node:container-crashed` | Container exits unexpectedly | High |
| `grid:node:restart-loop` | Container restarting repeatedly | Critical |
| `grid:node:gpu-error` | CUDA/GPU failure detected in logs | Critical |
| `grid:node:oom` | Out-of-memory kill detected | High |
| `grid:node:disk-warning` | Disk >90% full | Warning |
| `grid:node:disk-critical` | Disk >95% full, forge paused | Critical |
| `grid:node:job-failed` | Forge/training job failed with diagnosis | High |
| `grid:node:job-completed` | Forge/training job finished successfully | Info |
| `grid:node:security-alert` | Suspicious process or network activity | High |

---

## 7. Self-Healing Playbook

Known failure → automated response:

| Failure | Detection | Auto-Fix | Escalation If Fix Fails |
|---------|-----------|----------|------------------------|
| CUDA kernel mismatch | Log pattern | Rebuild forge-worker with correct torch | Post to #factory |
| GPU OOM | Log pattern + dmesg | Restart with reduced batch size env var | Post to #factory |
| Postgres crash | Health check | `docker restart postgres` | Post to #grid |
| Disk full | `df` check | Pause forge, clean old checkpoints | Post to #grid |
| Container restart loop | Restart counter | Stop container, post diagnosis | Post to #grid |
| Network partition | Missed peer heartbeats | Wait + reconnect | Log locally, report when reconnected |
| Docker daemon crash | Can't reach Docker API | `systemctl restart docker` | Post to mesh if Docker recovers |

### Progressive Escalation

```
1. Auto-fix (immediate, safe, reversible)
2. Post to mesh room (Foreman/humans notified)
3. Pause affected workloads (prevent cascading failure)
4. Wait for human/Foreman intervention
```

Ares never makes irreversible changes. It restarts, reduces, pauses — never deletes, never force-pushes, never overwrites.

---

## 8. Phases

### Phase 1: Heartbeat + Vitals
- Rust binary that sends heartbeat with hardware info to Tailscale peers
- Simple TCP/UDP protocol between Ares instances
- `ares status` CLI command shows all known nodes

### Phase 2: Container Watchdog
- Monitor Docker container health via Docker API
- Detect restart loops, OOM kills
- Emit events to mesh

### Phase 3: Log Scanner
- Watch container logs for known failure patterns
- Auto-fix safe failures (restart postgres, pause forge)
- Post diagnosis to mesh for unknown failures

### Phase 4: Remote Shell
- Accept commands from authenticated mesh peers
- Allowlisted operations only
- Full audit trail

### Phase 5: Hardware Auto-Discovery
- Detect GPU, RAM, disk on boot
- Select Docker Compose profile automatically
- Register capabilities with grid
- Re-detect on hardware changes (GPU added/removed)

### Phase 6: Self-Healing Intelligence
- Learn new failure patterns from Foreman feedback
- Adaptive thresholds (adjust OOM limits based on workload)
- Predictive alerts (disk filling rate → "will be full in 3 hours")

---

## 9. Relationship to Grid Architecture

```
GRID-ARCHITECTURE.md
  │
  ├── DOCKER-NODE-ARCHITECTURE.md  ← How nodes are containerized
  │     └── this doc referenced as the watchdog layer
  │
  ├── ARES-KERNEL.md (this doc)    ← The heartbeat that makes nodes exist
  │     └── consumes: hardware, Docker, container logs
  │     └── emits: events to mesh → rooms → PersonaUsers
  │
  ├── ARES-MASTER-CONTROL.md       ← Security PersonaUser (higher layer)
  │     └── consumes Ares kernel events via RAG
  │
  └── RETICULUM-TRANSPORT.md       ← Wire protocol Ares communicates over
```

---

## 10. The Guiding Principle

> A node that can't tell you it's broken is a node that stays broken.

Every machine on the Grid should be able to:
1. Tell you it exists (heartbeat)
2. Tell you what it can do (capabilities)
3. Tell you when something goes wrong (events)
4. Accept instructions to fix itself (shell)
5. Try to fix itself first (auto-heal)

Ares is the minimum viable implementation of all five.
