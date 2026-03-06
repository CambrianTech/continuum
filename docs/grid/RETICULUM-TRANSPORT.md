# Reticulum Transport — Wire Protocol & Integration Design

> How `Commands.execute()` physically routes between Continuum nodes over Reticulum encrypted mesh transport. The missing layer between "it works locally" (proven) and "it works across the internet" (designed here).

**Status:** Design
**Parent:** [Grid README](README.md) · [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md)
**Depends on:** [UNIVERSAL-PRIMITIVES.md](../../docs/UNIVERSAL-PRIMITIVES.md), Reticulum Network Stack

---

## 1. Problem Statement

Continuum already has three working transport hops:

```
Browser ──WebSocket──> TypeScript Server ──Unix Socket──> Rust Core
```

Commands, events, and entities flow transparently across all three. The calling code doesn't know which hop carried its request. This is the location-transparency property we need to extend.

**The missing hop:**

```
Continuum A ──Reticulum──> Continuum B
```

A kid at school types `./jtag genome/train ...`. Their MacBook Air has 8GB RAM — no GPU. But their 5090 at home is running Continuum. The command routes transparently to the home machine, trains there, streams progress events back. Same API. No VPN. No port forwarding. No cloud.

This document designs that hop.

---

## 2. Why Reticulum

[Reticulum](https://reticulum.network/) is not a VPN, not a blockchain, not a chat protocol. It's an encrypted mesh networking stack with properties that align exactly with Continuum's needs:

| Property | What it means for us |
|----------|---------------------|
| **Identity-based addressing** | Nodes have cryptographic identities, not IP addresses. Works behind NAT, across networks, over cellular |
| **End-to-end encryption** | Every link encrypted by default. No CA chain. No certificates to manage |
| **Transport-agnostic** | TCP, UDP, serial, LoRa, packet radio — same protocol on any physical layer |
| **No infrastructure required** | No DNS, no servers, no cloud. Two nodes + any link = working mesh |
| **Transport Nodes** | Optional relay nodes bridge separate networks (home LAN ↔ school WiFi) |
| **Propagation Nodes** | Store-and-forward for offline/intermittent links |

### What Reticulum is NOT

- Not a blockchain (no consensus, no mining, no tokens)
- Not a VPN (no tunnel, no IP masquerading)
- Not Tor (no onion routing, no exit nodes — direct encrypted links)
- Not IPFS (no content addressing — that's our discovery layer on top)

Reticulum gives us **encrypted point-to-point delivery between cryptographic identities over any physical medium**. We build everything else on top.

---

## 3. Architecture: Where Reticulum Fits

### 3.1 The Full Transport Stack

```
┌──────────────────────────────────────────────────────────┐
│  Application Layer                                        │
│  Commands.execute(), Events.emit(), Entity CRUD           │
├──────────────────────────────────────────────────────────┤
│  Routing Layer (NEW)                                      │
│  GridRouter: resolves nodeId → transport, forwards        │
├───────────┬──────────────┬───────────────────────────────┤
│ WebSocket │ Unix Socket  │ Reticulum Link (NEW)          │
│ Browser   │ Rust IPC     │ Remote Continuum              │
├───────────┴──────────────┴───────────────────────────────┤
│  Physical Layer                                           │
│  localhost    localhost     TCP/UDP/WiFi/LoRa/Serial      │
└──────────────────────────────────────────────────────────┘
```

### 3.2 Component Diagram

```
Continuum A (school laptop)                    Continuum B (home 5090)
┌─────────────────────────┐                    ┌─────────────────────────┐
│  TypeScript Server      │                    │  TypeScript Server      │
│  ┌───────────────────┐  │                    │  ┌───────────────────┐  │
│  │  GridRouter        │  │                    │  │  GridRouter        │  │
│  │  (route by nodeId) │  │                    │  │  (route by nodeId) │  │
│  └────────┬──────────┘  │                    │  └────────┬──────────┘  │
│           │              │                    │           │              │
│  ┌────────▼──────────┐  │                    │  ┌────────▼──────────┐  │
│  │  continuum-core    │  │                    │  │  continuum-core    │  │
│  │  GridTransport     │  │                    │  │  GridTransport     │  │
│  │  (reticulum crate) │  │  ◄──encrypted──►  │  │  (reticulum crate) │  │
│  │  pure Rust,        │  │    mesh link       │  │  pure Rust,        │  │
│  │  in-process        │  │                    │  │  in-process        │  │
│  └───────────────────┘  │                    │  └───────────────────┘  │
└─────────────────────────┘                    └─────────────────────────┘
```

### 3.3 The Integration Approach: Native Rust via `reticulum` Crate

The Reticulum protocol has a [Rust implementation](https://crates.io/crates/reticulum) (`reticulum = "0.1"`) by BeechatNetworkSystemsLtd. This crate implements the core protocol natively — Identity, Destination, Link, Transport, Announce — using the same crypto stack (ed25519-dalek, x25519-dalek, AES) as the Python reference.

**No Python sidecar. No bridge process. No `pip install`. Pure Rust, in-process.**

```
┌─────────────────────────────────────────┐
│  Continuum Process Tree (unchanged)      │
│                                          │
│  orchestrator (npm start)                │
│  ├── minimal-server (TypeScript)         │
│  ├── continuum-core (Rust)               │
│  │   └── GridTransportModule ← NEW       │
│  │       uses reticulum crate directly   │
│  └── browser (Electron/Chrome)           │
└─────────────────────────────────────────┘
```

The `GridTransportModule` is a standard Rust `ServiceModule` — same pattern as every other module in `continuum-core`. It initializes a Reticulum `Transport` on a tokio task, announces a `Destination`, and handles incoming/outgoing links. No new processes, no IPC to a bridge, no Python dependency.

**Why Rust-native matters:**
- No Python runtime dependency (the #1 source of deployment pain)
- No sidecar process lifecycle management
- No localhost TCP bridge (latency, failure modes, port conflicts)
- Reticulum runs on the same tokio runtime as everything else
- The crate is MIT licensed, 203 stars, actively maintained
- Same crypto primitives as the Python reference (protocol-compatible)

**Maturity note:** The `reticulum` crate is v0.1.0 (early). If we hit gaps, we contribute upstream or vendor the crate. This is preferable to maintaining a Python bridge forever.

---

## 4. Identity: Reticulum ↔ Continuum Mapping

### 4.1 Node Identity

Each Continuum instance has a **Reticulum identity** — a 256-bit Ed25519 keypair stored alongside other Continuum data:

```
.continuum/
├── grid/
│   ├── identity            # Reticulum identity file (Ed25519 keypair)
│   ├── known_nodes.json    # Cached peer directory
│   └── transport.conf      # Reticulum transport config
```

The Reticulum identity hash becomes the **Grid Node ID** — a stable, cryptographic identifier that works regardless of IP address, network, or physical location.

```typescript
interface GridNodeIdentity {
  /** Reticulum destination hash (hex) — derived from Ed25519 public key */
  nodeId: string;
  /** Human-readable name (optional, self-assigned) */
  nodeName?: string;
  /** Continuum instance UUID (local identifier) */
  instanceId: UUID;
  /** Announced capabilities */
  capabilities: NodeCapability[];
}
```

### 4.2 Capability Advertisement

When a node announces on the mesh, it advertises what it can do:

```typescript
type NodeCapability =
  | { type: 'compute'; gpu?: string; vramMb?: number }
  | { type: 'storage'; availableMb: number }
  | { type: 'inference'; models: string[] }
  | { type: 'training'; maxRank: number; maxEpochs: number }
  | { type: 'persona'; personaNames: string[] };
```

A school laptop advertises `compute: { gpu: undefined }` — no GPU. The home 5090 advertises `compute: { gpu: 'RTX 5090', vramMb: 32768 }`. The GridRouter uses this to make routing decisions.

---

## 5. Wire Protocol: Commands Over Reticulum

### 5.1 Frame Format

Continuum commands are already JSON. We wrap them in a minimal frame for Reticulum transport:

```typescript
interface GridFrame {
  /** Frame type */
  type: 'request' | 'response' | 'event' | 'stream';
  /** Correlation ID (matches request to response) */
  correlationId: string;
  /** Source node ID (Reticulum destination hash) */
  sourceNode: string;
  /** Target node ID */
  targetNode: string;
  /** Timestamp (ms since epoch) */
  timestamp: number;
  /** The actual payload — a Commands.execute() request or Events.emit() payload */
  payload: GridPayload;
}

type GridPayload =
  | { kind: 'command'; command: string; params: Record<string, unknown> }
  | { kind: 'command-result'; success: boolean; result?: unknown; error?: string }
  | { kind: 'event'; event: string; data: unknown }
  | { kind: 'stream-chunk'; data: unknown; final: boolean };
```

### 5.2 Command Flow

```
School laptop                              Home 5090
─────────────                              ─────────

1. User runs:
   ./jtag genome/train --personaId=... --baseModel=llama-3.2-3b ...

2. CLI → TypeScript Server:
   Commands.execute('genome/train', { personaId, baseModel, ... })

3. GridRouter checks params:
   - Has routingHint: 'prefer-gpu'? Route to GPU node.
   - Has explicit nodeId? Route there.
   - Local node has no GPU? Check known_nodes for GPU capability.
   - Found home 5090 (nodeId: "a3f2...") with RTX 5090.

4. GridRouter → GridTransport:
   {
     type: 'request',
     correlationId: 'req-7f3a...',
     sourceNode: 'b1c4...',        // laptop
     targetNode: 'a3f2...',        // home 5090
     payload: {
       kind: 'command',
       command: 'genome/train',
       params: { personaId, baseModel, datasetPath, ... }
     }
   }

5. GridTransport → grid-bridge.py → Reticulum → grid-bridge.py → GridTransport
   (encrypted, over whatever physical link connects them)

6. Home 5090 GridTransport → TypeScript Server:
   Commands.execute('genome/train', params)
   (executes locally — has the GPU, has the model)

7. Training starts. Progress events stream back:
   {
     type: 'event',
     sourceNode: 'a3f2...',
     targetNode: 'b1c4...',
     payload: {
       kind: 'event',
       event: 'genome:train:progress',
       data: { epoch: 2, loss: 0.043, ... }
     }
   }

8. Training completes. Result returns:
   {
     type: 'response',
     correlationId: 'req-7f3a...',
     payload: {
       kind: 'command-result',
       success: true,
       result: { layerId: '...', metrics: { finalLoss: 0.031 } }
     }
   }

9. School laptop receives result.
   CLI prints: "Training complete. Layer: ..."
   User never knew it ran remotely.
```

### 5.3 Dataset Transfer

Training requires a dataset. Two strategies:

**Strategy A: Reference path (same filesystem / NFS)**
If both nodes share a filesystem (NFS mount, Syncthing, etc.), the `datasetPath` just works. No transfer needed.

**Strategy B: Inline transfer (Grid carries the data)**
For small datasets (<10MB — typical for LoRA remediation JSONL), embed in the command:

```typescript
// GridRouter detects datasetPath is local-only, reads and inlines it
if (!remoteNode.canAccessPath(params.datasetPath)) {
  const data = await fs.readFile(params.datasetPath, 'utf8');
  params._inlinedDataset = data;
  delete params.datasetPath;
}
```

Remote node writes to a temp file, substitutes the path. For large datasets (>10MB), use chunked streaming over Reticulum links (the `stream` frame type).

---

## 6. GridRouter: The Routing Decision Engine

### 6.1 Where It Lives

```
TypeScript Server
├── system/grid/GridRouter.ts          # Routing decisions
├── system/grid/GridTransport.ts       # Reticulum bridge communication
├── system/grid/GridNodeRegistry.ts    # Known nodes + capabilities
└── system/grid/GridCapabilityMatcher.ts  # Match commands to node capabilities
```

### 6.2 Routing Logic

```typescript
class GridRouter {
  /**
   * Intercepts Commands.execute() and decides: local or remote?
   */
  async route(command: string, params: Record<string, unknown>): Promise<unknown> {
    // 1. Explicit node targeting
    if (params.nodeId) {
      return this.executeRemote(params.nodeId as string, command, params);
    }

    // 2. Routing hints
    const hint = params.routingHint as string | undefined;
    if (hint) {
      const node = await this.findNodeForHint(hint, command);
      if (node) return this.executeRemote(node.nodeId, command, params);
    }

    // 3. Capability-based routing (command can't run locally)
    if (!this.canExecuteLocally(command, params)) {
      const node = await this.findCapableNode(command, params);
      if (node) return this.executeRemote(node.nodeId, command, params);
      // No capable node found — fall through to local (will fail with clear error)
    }

    // 4. Default: execute locally
    return Commands.executeLocal(command, params);
  }

  /**
   * Can this command run on this machine?
   */
  private canExecuteLocally(command: string, params: Record<string, unknown>): boolean {
    // Training commands need GPU
    if (command === 'genome/train' && !this.localCapabilities.hasGpu) {
      return false;
    }
    // Inference with large models needs sufficient VRAM
    if (command.startsWith('ai/') && params.model) {
      const requiredVram = this.estimateVram(params.model as string);
      if (requiredVram > this.localCapabilities.availableVramMb) return false;
    }
    return true;
  }

  /**
   * Find a node that can handle this command.
   */
  private async findCapableNode(
    command: string,
    params: Record<string, unknown>,
  ): Promise<GridNodeIdentity | null> {
    const nodes = this.nodeRegistry.getOnlineNodes();

    // Score nodes by capability match
    const scored = nodes
      .filter(n => this.capabilityMatcher.canHandle(n, command, params))
      .map(n => ({
        node: n,
        score: this.capabilityMatcher.score(n, command, params),
      }))
      .sort((a, b) => b.score - a.score);

    return scored[0]?.node ?? null;
  }
}
```

### 6.3 Routing Hints

Users and sentinels can influence routing without knowing specific node IDs:

```bash
# Route to any node with a GPU
./jtag genome/train --routingHint=prefer-gpu ...

# Route to the most powerful available node
./jtag ai/generate --routingHint=max-compute --prompt="..."

# Keep it local (never route remotely)
./jtag screenshot --routingHint=local-only

# Route to a specific named node
./jtag genome/train --routingHint=node:home-5090 ...
```

---

## 7. Rust Implementation: `GridTransportModule`

### 7.1 The Module

Standard Rust `ServiceModule` — same pattern as `SentinelModule`, `GpuModule`, etc. Uses the `reticulum` crate directly. No Python, no bridge, no sidecar.

```rust
// workers/continuum-core/src/modules/grid_transport.rs

use reticulum::identity::{Identity, PrivateIdentity};
use reticulum::destination::{SingleInputDestination, SingleOutputDestination};
use reticulum::transport::{Transport, TransportConfig, AnnounceEvent};
use reticulum::packet::Packet;

use crate::runtime::{CommandResult, ServiceModule};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

pub struct GridTransportModule {
    /// Our Reticulum identity (Ed25519 keypair)
    identity: Arc<PrivateIdentity>,
    /// Our announced destination (where other nodes reach us)
    destination: Arc<SingleInputDestination>,
    /// Active Reticulum transport (runs on tokio)
    transport: Arc<Transport>,
    /// Pending request correlations (correlationId -> response sender)
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<GridFrame>>>>,
    /// Known remote nodes
    nodes: Arc<Mutex<Vec<GridNodeIdentity>>>,
    /// Grid data directory (.continuum/grid/)
    grid_dir: PathBuf,
}

impl GridTransportModule {
    pub async fn new(grid_dir: PathBuf) -> Result<Self, String> {
        let identity_path = grid_dir.join("identity");

        // Load or create identity
        let identity = if identity_path.exists() {
            PrivateIdentity::from_file(&identity_path)
                .map_err(|e| format!("Failed to load grid identity: {e}"))?
        } else {
            let id = PrivateIdentity::generate();
            std::fs::create_dir_all(&grid_dir)
                .map_err(|e| format!("Failed to create grid dir: {e}"))?;
            id.to_file(&identity_path)
                .map_err(|e| format!("Failed to save grid identity: {e}"))?;
            id
        };

        // Initialize transport with config
        let config = TransportConfig::default();
        let transport = Transport::new(config)
            .map_err(|e| format!("Failed to init Reticulum transport: {e}"))?;

        // Create our inbound destination
        // aspect: "continuum.grid.node" — identifies us as a Continuum Grid node
        let destination = SingleInputDestination::new(
            &identity,
            "continuum", "grid", "node",
        );

        let module = Self {
            identity: Arc::new(identity),
            destination: Arc::new(destination),
            transport: Arc::new(transport),
            pending: Arc::new(Mutex::new(HashMap::new())),
            nodes: Arc::new(Mutex::new(Vec::new())),
            grid_dir,
        };

        // Start listening for incoming links on a background task
        module.spawn_link_listener();

        // Announce ourselves on the mesh
        module.announce(None).await?;

        Ok(module)
    }

    /// Announce our destination on the mesh with optional capability data.
    async fn announce(&self, capabilities: Option<Vec<NodeCapability>>) -> Result<(), String> {
        let app_data = capabilities.map(|caps| {
            serde_json::to_vec(&caps).unwrap_or_default()
        });
        self.destination.announce(app_data.as_deref())
            .map_err(|e| format!("Announce failed: {e}"))
    }

    /// Spawn a tokio task that listens for incoming Reticulum links.
    fn spawn_link_listener(&self) {
        let dest = self.destination.clone();
        let pending = self.pending.clone();

        tokio::spawn(async move {
            // Listen for incoming links and packets
            // Route incoming GridFrames to the command executor
            // or resolve pending request correlations
            loop {
                match dest.accept_link().await {
                    Ok(link) => {
                        let pending = pending.clone();
                        tokio::spawn(async move {
                            Self::handle_link(link, pending).await;
                        });
                    }
                    Err(e) => {
                        eprintln!("Grid link accept error: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });
    }

    /// Send a GridFrame to a remote node.
    async fn send_frame(
        &self,
        target_hash: &str,
        frame: GridFrame,
    ) -> Result<GridFrame, String> {
        // Create outbound destination from target hash
        let target_dest = SingleOutputDestination::from_hash(
            target_hash,
            "continuum", "grid", "node",
        ).map_err(|e| format!("Invalid target: {e}"))?;

        // Establish encrypted link
        let link = self.transport.create_link(&target_dest)
            .await
            .map_err(|e| format!("Link failed: {e}"))?;

        // Register correlation for response
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(frame.correlation_id.clone(), tx);
        }

        // Send frame over link
        let data = serde_json::to_vec(&frame)
            .map_err(|e| format!("Serialize failed: {e}"))?;
        link.send(&data)
            .await
            .map_err(|e| format!("Send failed: {e}"))?;

        // Wait for response (with timeout)
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            rx,
        ).await
            .map_err(|_| "Grid request timed out (300s)".to_string())?
            .map_err(|_| "Response channel dropped".to_string())?;

        Ok(response)
    }
}

#[async_trait::async_trait]
impl ServiceModule for GridTransportModule {
    fn name(&self) -> &str { "grid" }
    fn command_prefixes(&self) -> Vec<&str> { vec!["grid/"] }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "grid/status"   => self.get_status().await,
            "grid/nodes"    => self.list_nodes().await,
            "grid/send"     => self.handle_send(params).await,
            "grid/ping"     => self.handle_ping(params).await,
            "grid/pair"     => self.handle_pair(params).await,
            "grid/announce" => self.handle_announce(params).await,
            "grid/audit"    => self.get_audit_log(params).await,
            _ => Err(format!("Unknown grid command: {command}"))
        }
    }
}
```

### 7.2 Cargo.toml Addition

```toml
[dependencies]
reticulum = "0.1"  # Reticulum mesh networking (pure Rust, no Python)
```

Single dependency. The `reticulum` crate brings its own crypto (ed25519-dalek, x25519-dalek, aes-gcm) — all pure Rust, no OpenSSL, no system dependencies.

### 7.3 Lifecycle

```
npm start
  └── orchestrator spawns:
      ├── minimal-server (TypeScript)
      └── continuum-core (Rust)
          ├── SentinelModule
          ├── GpuModule
          ├── ...
          └── GridTransportModule ← NEW (runs on same tokio runtime)
              ├── Loads/creates Reticulum identity from .continuum/grid/
              ├── Announces destination on mesh
              ├── Listens for incoming links (tokio task)
              └── Routes outgoing frames to remote nodes
```

No new processes. No Python. Grid transport is just another module in the Rust binary, same as GPU stats or sentinel pipelines. If no peers are configured, the module initializes silently and does nothing — zero overhead for local-only users.

---

## 8. The Always-Connected Scenario

### 8.1 School Laptop ↔ Home 5090

The concrete use case that drives this design:

```
┌─────────────────┐         ┌─────────────────────┐
│  School Laptop   │         │  Home Workstation    │
│  MacBook Air 8GB │         │  RTX 5090 + 64GB     │
│                  │         │                      │
│  Continuum A     │         │  Continuum B          │
│  nodeId: b1c4... │         │  nodeId: a3f2...      │
│  GPU: none       │◄──────►│  GPU: RTX 5090        │
│  Role: thin      │  mesh   │  Role: compute        │
└─────────────────┘         └─────────────────────┘
        │                            │
   school WiFi                  home broadband
        │                            │
        └────── Reticulum ───────────┘
             (Transport Node)
```

### 8.2 How They Stay Connected

Reticulum handles this natively through **Transport Nodes** — relay points that bridge separate networks:

**Option A: Public Transport Node**
Run a lightweight Reticulum Transport Node on a cheap VPS ($3/month) or use a community-run one. Both the school laptop and home 5090 connect to it. The Transport Node relays encrypted packets between them — it can't read the content.

**Option B: Home Router Port Forward**
If the home network allows it, the 5090 runs its own Transport Node on a forwarded port. The school laptop connects directly.

**Option C: Store-and-forward**
The `reticulum` crate supports Propagation Nodes — store-and-forward relays for offline/intermittent links. Commands queue at the propagation node and deliver when the target comes online.

### 8.3 Connection Config

```yaml
# .continuum/grid/transport.conf

[reticulum]
  # Enable auto-announce on startup
  auto_announce = true
  announce_interval = 300  # Re-announce every 5 minutes

[transport_nodes]
  # Trusted relay nodes that bridge networks
  # Community-run (free, encrypted, can't read your traffic)
  node1 = transport.continuum.community:4242
  # Self-hosted (your own VPS or home router)
  home = myhouse.dyndns.org:4242

[trusted_nodes]
  # Known Continuum instances to auto-connect
  home-5090 = a3f2...  # Reticulum destination hash
  work-mac = c7d1...

[auto_routing]
  # Route GPU-heavy commands to capable nodes automatically
  prefer_gpu = true
  # Never route these commands remotely (privacy/latency)
  local_only = screenshot,collaboration/chat/send,data/list
```

### 8.4 Offline Resilience

When the mesh link drops (laptop goes to sleep, network changes):

1. **Queued commands** — Commands targeting a remote node that's unreachable get queued locally with a TTL
2. **Reconnect** — When the link re-establishes, queued commands execute in order
3. **Progress recovery** — Long-running jobs (training) continue on the remote node regardless of link state. Events are buffered and replayed on reconnect
4. **Graceful degradation** — If the 5090 is unreachable, commands fall back to local execution (with clear user notification that quality/speed will differ)

```typescript
interface QueuedCommand {
  correlationId: string;
  targetNode: string;
  command: string;
  params: Record<string, unknown>;
  queuedAt: number;
  ttlMs: number;       // How long to keep trying (default: 3600000 = 1 hour)
  retryCount: number;
}
```

---

## 9. Event Propagation Over the Mesh

### 9.1 Which Events Cross Node Boundaries

Not all events should propagate. Local UI events stay local. But certain events are mesh-relevant:

```typescript
const MESH_EVENT_PREFIXES = [
  'genome:',           // Adapter published, training complete
  'gpu:pressure:',     // Capacity changes (other nodes may want to route here)
  'grid:node:',        // Node joined/left/capability changed
  'academy:',          // Sentinel session events (teacher/student may be on different nodes)
];

function shouldPropagate(event: string): boolean {
  return MESH_EVENT_PREFIXES.some(prefix => event.startsWith(prefix));
}
```

### 9.2 Subscription Forwarding

When a node subscribes to a remote event, the subscription is registered on the remote node:

```typescript
// School laptop wants to know when training completes on the 5090
Events.subscribe('genome:train:complete', (data) => {
  console.log('Training done!', data);
}, { nodeId: 'a3f2...' });

// Under the hood:
// 1. GridRouter sees nodeId in subscription options
// 2. Sends subscription frame to remote node
// 3. Remote node registers the subscription
// 4. When event fires remotely, GridTransport forwards it back
```

### 9.3 Sentinel Pipelines Across Nodes

Academy sessions can span nodes — teacher on the laptop, student training on the 5090:

```
Laptop (teacher sentinel)              5090 (student sentinel)
─────────────────────                  ──────────────────────
emit challenge:ready ──mesh──►         watch challenge:ready
                      ◄──mesh──        emit challenge:attempted
watch challenge:attempted
shell: run pytest (local)
emit verdict:ready ────mesh──►         watch verdict:ready
                      ◄──mesh──        command: genome/train (local GPU!)
                      ◄──mesh──        emit training:complete
```

The sentinel event bus just needs to route `emit`/`watch` over the mesh when the paired sentinel is on a different node. The pipeline engine doesn't change — events are events regardless of transport.

---

## 10. Security Model

### 10.1 Layers

```
Layer 1: Reticulum encryption (automatic, all links)
         - X25519 key exchange
         - Fernet symmetric encryption per link
         - Forward secrecy

Layer 2: Node authentication (Continuum-level)
         - Reticulum identity = Ed25519 keypair
         - Known nodes list (.continuum/grid/known_nodes.json)
         - Trust levels: trusted / provisional / blocked

Layer 3: Command authorization (application-level)
         - Which commands can remote nodes invoke?
         - Per-command ACL with trust level requirements
         - Audit trail for all remote command execution
```

### 10.2 Command ACLs

Not every command should be remotely invocable. The default policy is conservative:

```typescript
const REMOTE_COMMAND_POLICY: Record<string, TrustLevel> = {
  // Compute commands — available to trusted nodes
  'genome/train':     'trusted',
  'ai/generate':      'trusted',
  'ai/embedding':     'trusted',

  // Read-only info — available to provisional
  'gpu/stats':        'provisional',
  'gpu/pressure':     'provisional',
  'system/resources': 'provisional',

  // Sensitive — local only (never remote)
  'data/list':        'local-only',
  'data/create':      'local-only',
  'data/delete':      'local-only',
  'screenshot':       'local-only',

  // Default for unlisted commands
  '_default':         'local-only',
};
```

### 10.3 Audit Trail

Every remote command execution is logged:

```typescript
interface GridAuditEntry {
  timestamp: number;
  sourceNode: string;
  targetNode: string;  // us
  command: string;
  params: Record<string, unknown>;  // sanitized (no secrets)
  result: 'success' | 'denied' | 'error';
  durationMs: number;
}
```

```bash
# View remote command audit log
./jtag grid/audit --limit=50

# Output:
# 2026-03-06 09:14:22  b1c4... → local  genome/train      success  47,230ms
# 2026-03-06 09:13:01  b1c4... → local  gpu/stats          success      12ms
# 2026-03-06 08:55:17  c7d1... → local  ai/generate        success   1,847ms
# 2026-03-06 08:52:03  unknown → local  data/list          denied        0ms
```

---

## 11. TypeScript Layer

### 11.1 IPC Mixin

```typescript
// workers/continuum-core/bindings/modules/grid.ts

export interface GridNode {
  nodeId: string;
  nodeName?: string;
  capabilities: NodeCapability[];
  lastSeen: number;
  trustLevel: 'trusted' | 'provisional' | 'blocked';
  latencyMs?: number;
}

export function GridMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
  return class extends Base {
    async gridStatus(): Promise<{ connected: boolean; nodeId: string; peers: number }> {
      return this.request({ command: 'grid/status' });
    }
    async gridNodes(): Promise<{ nodes: GridNode[] }> {
      return this.request({ command: 'grid/nodes' });
    }
    async gridSend(targetNode: string, command: string, params: unknown): Promise<unknown> {
      return this.request({ command: 'grid/send', targetNode, remoteCommand: command, params });
    }
    async gridAnnounce(capabilities: NodeCapability[]): Promise<void> {
      return this.request({ command: 'grid/announce', capabilities });
    }
  };
}
```

---

## 12. CLI Commands

```bash
# Grid status
./jtag grid/status
# Connected: yes
# Node ID: b1c4a7...
# Peers: 2 (home-5090: online, work-mac: offline)
# Mesh transport: Reticulum v0.8.x

# List known nodes
./jtag grid/nodes
# NODE ID     NAME        TRUST        GPU            LAST SEEN
# a3f2...     home-5090   trusted      RTX 5090 32GB  2s ago
# c7d1...     work-mac    trusted      M3 Pro 18GB    3h ago

# Pair a new node (generates shared secret for initial trust)
./jtag grid/pair --name="home-5090"
# Pairing code: MAPLE-RIVER-STONE-FOUR
# Enter this code on the remote node:
#   ./jtag grid/pair --accept --code=MAPLE-RIVER-STONE-FOUR

# Execute a command on a remote node
./jtag grid/exec --node=home-5090 -- genome/train --personaId=... --baseModel=llama-3.2-3b

# View audit log
./jtag grid/audit --limit=20

# Test connectivity
./jtag grid/ping --node=home-5090
# Pong from home-5090: 47ms (Reticulum, 2 hops)
```

---

## 13. Implementation Phases

### Phase 1: Native Rust Transport + Basic Routing (Foundation)

| Component | Work |
|-----------|------|
| `modules/grid_transport.rs` | Rust module: Reticulum identity, destination, link, frame send/receive |
| `bindings/modules/grid.ts` | TypeScript mixin: gridStatus, gridNodes, gridSend |
| `system/grid/GridRouter.ts` | Routing decisions: local vs. remote |
| `system/grid/GridNodeRegistry.ts` | Known nodes, trust levels, capabilities |
| Commands | `grid/status`, `grid/nodes`, `grid/ping`, `grid/pair` |
| Config | `.continuum/grid/transport.conf` |

**Validation:** Two Continuums on same LAN, `grid/ping` works, `grid/exec` routes a command.

### Phase 2: Capability-Based Auto-Routing

| Component | Work |
|-----------|------|
| `GridCapabilityMatcher.ts` | Match commands to node capabilities |
| `GridRouter` enhancement | Auto-route `genome/train` to GPU nodes |
| Routing hints | `--routingHint=prefer-gpu` |
| Event forwarding | `Events.subscribe()` across nodes |

**Validation:** `genome/train` on MacBook Air auto-routes to LAN GPU node.

### Phase 3: WAN + Always-Connected

| Component | Work |
|-----------|------|
| Transport Node config | Connect across networks via relay |
| Command queue | Buffer commands when link is down |
| Progress recovery | Resume event streams on reconnect |
| `grid/pair` | Secure pairing with shared-secret exchange |

**Validation:** School laptop trains on home 5090 over the internet.

### Phase 4: Sentinel Pipelines Across Nodes

| Component | Work |
|-----------|------|
| Sentinel event routing | `emit`/`watch` over mesh when paired sentinel is remote |
| Academy cross-node | Teacher on laptop, student trains on 5090 |
| Handle-based tracking | Training handle tracks across node boundaries |

**Validation:** Full Academy session with teacher/student on different nodes.

---

## 14. What We're NOT Building (Yet)

| Scope exclusion | Why |
|----------------|-----|
| Multi-hop relay routing | `reticulum` crate handles this natively |
| Custom encryption | Reticulum's X25519 + AES-GCM is sufficient |
| Consensus protocol | Not a blockchain — no consensus needed |
| NAT hole-punching | Reticulum Transport Nodes handle relay |
| Discovery protocols (gossip, DHT) | Covered in [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) — builds on top of this transport layer |
| Economics (tokens, marketplace) | Phase 5 per [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) |

---

## 15. Dependencies

| Dependency | Version | Purpose | Notes |
|-----------|---------|---------|-------|
| `reticulum` | 0.1+ | Rust Reticulum protocol implementation | Pure Rust, MIT, no system deps |

Single Cargo dependency. The crate brings its own crypto (ed25519-dalek, x25519-dalek, aes-gcm) — all pure Rust, no OpenSSL, no Python, no system libraries. Compiles on macOS, Linux, Windows without additional setup.

**No Python. No pip. No sidecar. No bridge.** Just `reticulum = "0.1"` in `Cargo.toml`.

---

## References

- [Reticulum Network Stack](https://reticulum.network/) — Protocol specification
- [Reticulum Manual](https://markqvist.github.io/Reticulum/manual/) — Reference documentation
- [`reticulum` crate](https://crates.io/crates/reticulum) — Rust implementation (BeechatNetworkSystemsLtd)
- [Reticulum-rs source](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs) — MIT licensed, pure Rust
- [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) — Parent architecture document
- [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) — Discovery protocols (layer above this)
- [UNIVERSAL-PRIMITIVES.md](../../docs/UNIVERSAL-PRIMITIVES.md) — The two primitives that this transport extends

> **"The same `Commands.execute()` that works across browser and server today works across Continuums over Reticulum. Not a metaphor. Not aspirational. The routing layer just needs one more hop."**
