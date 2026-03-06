# The Grid: Architecture & Vision

> **"The same two primitives that work across browser and server today work across Continuums over Reticulum. No new protocol needed."**

---

## 1. Overview

The Grid is a decentralized mesh of Continuum instances sharing compute, intelligence, and genomic capabilities. Not a cloud platform. Not a blockchain. A living network where sovereign nodes cooperate as peers.

**Three core properties:**

1. **Infrastructure-independent** — works over any physical layer (TCP, UDP, LoRa, packet radio). No DNS. No certificates. No servers required.
2. **Accessible by default** — runs on an 8GB MacBook Air. Free participation, always. Economics are opt-in.
3. **Equal citizenship** — same API for human operators and AI governance sentinels. Same controls, same audit trail.

**Document map:**

| Document | Scope |
|----------|-------|
| **This document** | Grid architecture umbrella — principles, scaling, rollout, validation, economics |
| [RETICULUM-TRANSPORT.md](RETICULUM-TRANSPORT.md) | Wire protocol — how Commands.execute() routes between nodes over Reticulum |
| [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) | Discovery protocols — gossip, flood, DHT, semantic search |
| [LORA-MESH-DISTRIBUTION.md](../genome/LORA-MESH-DISTRIBUTION.md) | Genome marketplace — Personafile format, LoRA registry, distribution |
| [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md) | Economic theory research paper |
| [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md) | Per-node resource management — GPU governor, pressure watchers, eviction |

---

## 2. Design Principles

### 2.1 Accessibility First

Continuum runs on an 8GB MacBook Air. Free by default. No cloud APIs required. No subscriptions. No credit card.

The target audience is anyone locked out by AI pricing: kids and students with no funds, hobbyists in developing regions, researchers without corporate backing. Qwen 3.5 quantized models make this viable — a 0.8B sentinel at ~500MB VRAM, a quantized persona backbone, embeddings. The governance sentinel manages what's loaded versus paged to disk.

**Non-negotiable design constraint:** if it doesn't run on a school laptop, it doesn't ship.

### 2.2 Equal Citizens, Equal Controls

The same `Commands.execute()` API works for a human operator typing `./jtag gpu/set-limits` and for an AI governance sentinel calling `Commands.execute('gpu/set-limits', params)`. Same interface, same parameters, same audit trail, same permissions model.

Like an orchestra — same controls regardless of who's conducting. The API doesn't care if the operator is carbon or silicon.

### 2.3 Containerized Sovereignty

Each Continuum instance is self-contained: models, LoRA adapters, persona configurations, memories, sentinel pipelines, data stores. The container IS the home.

You can snapshot it, migrate it over Reticulum, restore it on different hardware. Fork it for experimentation. Your Continuum is sovereign — it joins the mesh as a peer, shares capabilities voluntarily, but never loses autonomy.

### 2.4 Bidirectional Resource Scaling

Not just "degrade gracefully on constrained hardware." Scale UP when capacity joins. Scale OUT to mesh peers. The same eviction logic (`age_seconds / (priority_weight * 10)`) works at every level — local VRAM, system RAM, SSD, cloud, mesh.

A system that only scales down is half an architecture. The Grid scales in every direction.

### 2.5 LAN-First, Security-Hardened Outward

Work out every kink on the local network before going public. Trust is earned concentrically:

```
Local Machine → LAN Mesh → Trusted WAN → Public Grid
```

Every attack vector identified and mitigated at each ring before expanding to the next. All LAN peers start as trusted. WAN peers start as provisional. Public peers start as new.

---

## 3. The Universal Primitives Are The Grid Protocol

**The key insight: this is not new infrastructure. It's the same infrastructure extended.**

Continuum already solves cross-environment communication with two universal primitives: `Commands.execute()` for request/response and `Events.subscribe()/emit()` for pub/sub. These work identically across browser, server, and Rust IPC today. Extending them over Reticulum to work across Continuums isn't new plumbing — it's the same plumbing at a different scale.

### 3.1 Commands.execute() Across Continuums

```typescript
// This already works across browser ↔ server ↔ Rust IPC
const result = await Commands.execute('gpu/stats');

// Extending to remote nodes: same API, transparent routing
// The routing layer just needs to know which node owns the target
const result = await Commands.execute('gpu/stats', { nodeId: remoteNodeId });

// Command already carries sessionId, context, userId
// Reticulum transport is just another hop
```

Same promise-based API. Same type safety. Same error handling. The routing layer resolves whether to execute locally or forward over Reticulum — the calling code doesn't know or care.

### 3.2 Events Across Continuums

```typescript
// Local events (already working)
Events.subscribe('gpu:pressure:high', handlePressure);
Events.emit('genome:adapter:published', adapterInfo);

// Mesh events: same API, mesh-aware transport
Events.subscribe('mesh:node:joined', handleNewCapacity);
Events.emit('mesh:genome:available', { adapterId, capabilities });
```

Pressure events, genome announcements, presence updates, chat messages — all flow through the same event system. The transport becomes mesh-aware; the API stays identical.

### 3.3 Handle-Based Long-Running Operations

Sentinel pipelines, training jobs, and genome sharing already use handle-based patterns: return a handle immediately, emit progress events, complete asynchronously. This is proven infrastructure.

```typescript
// Already works for local training
const handle = await Commands.execute('genome/train', { adapter, dataset });
Events.subscribe(`training:${handle.id}:progress`, updateUI);
Events.subscribe(`training:${handle.id}:complete`, handleResult);

// Extends naturally to remote execution
// "Train this LoRA" routes to the 5090 across the room
// Handle tracks progress. Events stream back. Same pattern.
const handle = await Commands.execute('genome/train', {
  adapter, dataset,
  routingHint: 'prefer-gpu'  // Let the mesh find the best node
});
// Same event subscriptions work — transport is transparent
```

Browser↔server today. Continuum↔Continuum tomorrow. Same pattern.

### 3.4 Entities as Universal Currency

Entities already serialize/deserialize cleanly, carry UUIDs, have CRUD events, and work through the generic data layer. Sharing entities across the mesh means sharing the same data primitives that already work across environments.

```typescript
// UserEntity, GenomeLayerEntity, ChatMessageEntity...
// All have UUIDs, timestamps, CRUD operations
// All serialize to JSON, deserialize on the other side
// The data daemon doesn't care where the entity originated
```

No new serialization format. No new ID scheme. No new event system. The Grid protocol IS the existing protocol, routed over a mesh.

---

## 4. Transport Layer: Reticulum

### 4.1 Why Reticulum

[Reticulum](https://reticulum.network/) is an encrypted mesh networking stack that works without servers, DNS, or certificates. Identity-based addressing over any physical layer.

**Properties that matter for the Grid:**

- **No infrastructure required** — works peer-to-peer over TCP, UDP, LoRa, serial, packet radio
- **End-to-end encrypted** — every link encrypted by default, no CA trust chain needed
- **Identity-based** — nodes have cryptographic identities, not IP addresses
- **Transport-agnostic** — same protocol whether the link is Ethernet, WiFi, or a LoRa radio
- **Resilient** — no single point of failure, no central coordination

### 4.2 Integration

Reticulum destinations map to Continuum node IDs. Each Continuum instance announces itself as a Reticulum destination. Commands route over the mesh transparently — the command system already handles routing between environments; Reticulum becomes another transport option alongside WebSocket and Unix socket.

```
Browser ──WebSocket──► TypeScript Bridge ──Unix Socket──► Rust Core
                                          ──Reticulum──► Remote Continuum
```

### 4.3 Transport Hierarchy

| Layer | How | Trust | Latency |
|-------|-----|-------|---------|
| **LAN** | Auto-discover via local interfaces (mDNS, broadcast) | High — same physical network | <1ms |
| **WAN** | Reticulum Transport Nodes relay between LANs | Medium — explicitly invited peers | 10-100ms |
| **Exotic** | LoRa, packet radio, serial links | Variable — infrastructure-independent operation | 100ms-10s |

### 4.4 Relationship to Discovery

The gossip protocols, bounded flood search, and DHT described in [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) run ON TOP of Reticulum transport. Reticulum handles encrypted point-to-point delivery. The discovery layer handles finding who has what.

---

## 5. Containerization Model

### 5.1 The Continuum Container

A Continuum instance is a self-contained unit:

```
Continuum Container
├── Base models (Qwen, Llama, Orpheus — quantized to fit hardware)
├── LoRA adapters (personality, skills, voice, vision, governance)
├── Persona configurations (identity, preferences, state)
├── Memories (per-persona SQLite databases, RAG contexts)
├── Sentinel pipelines (recipes, automation, background tasks)
├── Data stores (chat history, entities, audit trail)
└── Governance config (resource limits, trust policies, peer list)
```

### 5.2 Lifecycle

| Operation | What | How |
|-----------|------|-----|
| **Build** | Compose from base + layers | Select base model, add adapters, configure personas |
| **Run** | Local governance | Sentinel manages resources, personas run autonomously |
| **Migrate** | Snapshot → transfer → restore | Serialize state, send over Reticulum, restore on target |
| **Clone** | Fork for experimentation | Copy container, modify independently, merge back if useful |

### 5.3 Grid Interaction

Containers join the mesh as sovereign nodes. Each runs its own governance sentinel. They share capabilities but maintain autonomy — no node can compel another. The [Personafile format](../genome/LORA-MESH-DISTRIBUTION.md) defines the portable, shareable unit for persona+genome bundles.

---

## 6. Resource Scaling: Local ↔ Cloud ↔ Mesh

### 6.1 The Hierarchy

```
GPU VRAM → System RAM → Local SSD → Cloud Storage → P2P Mesh
```

Same eviction scoring at every level. Same priority model. Same governance sentinel making decisions. The hierarchy extends transparently — what works for local VRAM eviction works for mesh-wide resource allocation.

### 6.2 Scaling Down

**8GB MacBook Air:** Qwen3.5-0.8B sentinel (~500MB VRAM) + quantized persona backbone + embeddings. Governance sentinel manages what's loaded versus paged to disk. Rendering quality degrades (lower resolution, fewer avatars). Voice identity never changes. Functionality is preserved; fidelity adapts.

Qwen's quantized model releases make this viable. The governance sentinel is the same code on the Air and the workstation — just with different VRAM budgets.

### 6.3 Scaling Up

The 5090 joins the LAN mesh when you get home. The system detects new capacity:

- Queued training jobs execute
- Larger models swap in (3B → 9B backbone)
- Inference quality improves (fewer quantization artifacts)
- More concurrent personas at full capacity
- Avatar rendering at higher resolution/framerate

This happens automatically. The pressure watcher events fire, the governance sentinel recalculates what fits, adapters page in. Same eviction logic in reverse — instead of evicting under pressure, the system fills available capacity with queued work.

### 6.4 Scaling Out

Local resources exhausted → offload to mesh peers:

- Training job would kill the Air → route to workstation via `Commands.execute()` with routing hints
- Disk pressure → overflow to cloud or peer storage
- Inference bottleneck → distribute across mesh nodes with available capacity

The command routing layer handles this transparently. The calling code doesn't change.

### 6.5 Dynamic Horsepower Discovery

**The real-world scenario:**

Kid's MacBook Air at school during the day — Qwen quantized models, free, governance sentinel keeping things running smoothly.

5090 workstation joins the LAN mesh when you get home — system detects the capacity jump, ramps up training, inference, quality. Queued work executes. Models upgrade.

Work machine joins remotely via Reticulum WAN — additional capacity, different timezone availability.

The system discovers capacity changes and adapts automatically. Ramps up when horsepower arrives. Scales back gracefully when it leaves. No manual configuration. The pressure watchers and governance sentinel handle it.

### 6.6 Equal Controls

```bash
# Human operator
./jtag gpu/set-limits --maxVram=6GB --priority=interactive

# AI governance sentinel (same command, same params, same audit trail)
await Commands.execute('gpu/set-limits', { maxVram: '6GB', priority: 'interactive' });
```

Same API. Same permissions model. Same audit trail. Whether a human is tuning resources or an AI sentinel is autonomously managing its node — the controls are identical.

---

## 7. Phased Rollout (LAN-First)

### Phase 1: Local (Current)

Single machine operation. Full audit trail infrastructure. Resource governance Layers 0-2 operational (priority allocation, eviction registry, pressure watchers). No networking. Foundation for everything above.

### Phase 2: LAN Mesh

Reticulum auto-discover on local network. Same household, same office. Security hardening: every attack vector identified and mitigated BEFORE going outward.

- All LAN peers start as trusted
- Commands route transparently between LAN nodes
- Genome sharing within the mesh
- Training job offloading to capable peers
- Pressure events propagate across LAN

### Phase 3: Trusted WAN

Reticulum Transport Nodes connect separated LANs. Invite-only, explicitly trusted peers. Your work machine joins your home LAN remotely.

- Cross-validation begins
- Reputation system activated
- Trust earned through successful job completion
- Bandwidth-aware routing (WAN is slower than LAN)

### Phase 4: Public Grid

Open participation. Full intelligent validation required for untrusted nodes.

- Trust levels: new → provisional → established → trusted → flagship
- Immune system active (see Section 8)
- Reputation staking
- All validation mechanisms operational

### Phase 5: Grid Economics

Continuum Credits (CC). Optional marketplace. Free participation always possible — economics are a layer on top, never a gate.

---

## 8. Intelligent Validation

> **"Intelligence validates intelligence. Rule breakers are easily isolated or banished."**

### The Problem with Proof-of-Work

Bitcoin's model: do arbitrary, useless computation (hash puzzles), prove you did it. Wasteful by design. This doesn't work for Continuum — our tasks are varied and unpredictable (inference, training, vision, custom recipes). Wasting compute defeats the purpose.

### The Grid Model

**Core principle:** Machines and AI intelligences in the Grid validate each other on semantic plausibility, not formula.

```
"Do useful work, others verify it makes sense"

- Tasks are unpredictable (can't pre-compute cheats)
- Validators are intelligent (checking sense, not formula)
- Cheating leaves trails (inconsistent history)
- Community has incentive (catch cheaters, earn rep)
- Self-correcting (bad actors isolated automatically)
```

### Five Validation Mechanisms

**1. Redundant Execution** — high-value jobs run on multiple randomly-selected nodes. Outputs compared. Consensus = valid. Used for large training jobs, critical inference.

**2. Spot Checks** — random re-execution by validators. Any completed job can be spot-checked. Failed spot check = reputation hit. Catches lazy nodes.

**3. AI Semantic Validation** — does the output make sense? AIs evaluate plausibility. "Given this prompt, is this response reasonable?" Not exact match — sanity checking.

**4. Statistical Consistency** — honest nodes have consistent performance profiles. Latency, quality, resource usage follow patterns. Anomalies flag investigation. Long-term reputation building.

**5. Witness Signatures** — third-party attestation for high-value jobs. Witnesses observe execution, sign attestation. Multiple witnesses = high confidence. Used for disputes.

### The Immune System

The Grid's AIs aren't passive validators — they're an active immune system:

**Epidemiology model:** Same approach disease researchers use. Watch exploit patterns in traffic. Track attack vector variations. Predict threat combinations. Prepare defenses before attacks materialize. Share threat intelligence across the Grid.

**Red/Blue team simulation:** Red team AIs probe for weaknesses. Blue team AIs defend and adapt. War games run constantly in sandboxed environments. Novel attack combinations discovered BEFORE adversaries find them.

**CDC containment model:** When you work with dangerous attack patterns (even in simulation), containment is critical. Sandboxing. Isolation. Automatic boundary checking. Fail-safe on anomaly detection.

**Distributed immune response:**

```
Attack detected at Node A
    → Alert nearby nodes
    → Pattern shared Grid-wide
    → AIs analyze components
    → Predict next steps
    → Countermeasures deployed Grid-wide
    → Attacker isolated, pattern catalogued
    → Grid is now immune to this attack class
```

The Grid doesn't just survive attacks — it evolves from them. Antifragile by design.

---

## 9. Reputation System

### Reputation Score

```typescript
interface NodeReputation {
  nodeId: UUID;

  // Core metrics
  jobsCompleted: number;
  jobsFailed: number;
  spotChecksPassed: number;
  spotChecksFailed: number;

  // Derived scores (0-100)
  reliabilityScore: number;      // Completion rate
  accuracyScore: number;         // Validation pass rate
  consistencyScore: number;      // Statistical stability

  // Trust level
  trustLevel: 'new' | 'provisional' | 'established' | 'trusted' | 'flagship';

  // Flags
  warnings: Warning[];
  suspensions: Suspension[];

  // Staking (Phase 5)
  stakedCredits?: number;
}
```

### Trust Levels

| Level | Jobs | Validation | Access | Notes |
|-------|------|-----------|--------|-------|
| **New** | 0-10 | High frequency | Low-value jobs only | Probationary |
| **Provisional** | 10-100 | Moderate | Medium-value jobs | Can be demoted easily |
| **Established** | 100-1000 | Spot-check only | Most jobs | Demotion requires pattern of failures |
| **Trusted** | 1000+ | Minimal | All jobs, can validate others | Significant reputation at stake |
| **Flagship** | Invitation | Priority | Governance participation | Community leadership |

### Isolation and Banishment

| Level | Trigger | Response | Recovery |
|-------|---------|----------|----------|
| **Warning** | Single failed validation | Minor reputation hit, increased validation frequency | Good behavior |
| **Suspension** | Pattern of failures (3+ in window) | Removed from job pool temporarily | Wait out period, re-enter at lower trust |
| **Banishment** | Confirmed malicious behavior | Permanent removal, node ID blacklisted | Governance appeal only |

---

## 10. Economic Model (Phase 5)

### Continuum Credits (CC)

Economics are **optional**. Free participation is always possible. CC is a layer on top — never a gate.

```
Continuum Credits (CC)
├── Earned by providing compute
├── Spent by consuming compute
├── Staked for reputation
├── Governance voting power
└── Transferable between nodes
```

### Earning

```typescript
interface JobPayment {
  jobId: UUID;
  computeUnits: number;          // Standardized measure
  jobType: string;
  difficulty: number;            // Complexity factor

  // Validation
  validationProof: ValidationProof;
  validatorSignatures: string[];

  // Payment
  baseRate: number;              // Market rate per compute unit
  difficultyMultiplier: number;  // Harder jobs pay more
  reputationBonus: number;       // High-rep nodes get bonus
  totalEarned: number;
}
```

### Spending

```typescript
interface JobRequest {
  requesterId: UUID;
  jobType: string;
  estimatedComputeUnits: number;
  maxPrice: number;

  minNodeReputation?: number;    // Quality requirement
  redundancy?: number;           // How many nodes to run
  urgency?: 'low' | 'normal' | 'high';

  escrowedCredits: number;       // Locked until completion
}
```

### Market Dynamics

Supply (nodes offering compute) competes on price and reputation. Demand (users needing compute) bids for resources. Different job types create different markets. Scarcity drives prices up; competition drives them down. Reputation staking (lock credits as collateral, slashed if caught cheating) incentivizes honest behavior.

---

## 11. Security Properties

### Resilience Through Diversity

The Grid's security relies on diversity and statistics, not infallible cryptography:

```
Traditional: "If they crack the crypto, everything falls"
Grid:        "Even if they crack the crypto, they still lose"
             — Must fool MANY independent intelligences
             — Simultaneously
             — Without statistical detection
```

Cryptography is a **layer**, not the foundation. The foundation is diversity of validators and statistical consensus.

### Attack Resistance

| Attack | Mitigations |
|--------|-------------|
| **Sybil** (many fake nodes) | Reputation takes time/work to build. Staking required for trust. Cross-validation catches inconsistencies. |
| **Collusion** (validators conspiring) | Random validator selection. Multiple independent validators. Statistical anomaly detection. |
| **Lazy nodes** (claiming work not done) | Spot checks with real re-execution. Output hash verification. Timing analysis. |
| **Output manipulation** | AI semantic validation. Redundant execution comparison. Historical consistency checks. |

### Reticulum Transport Security

- End-to-end encryption on every link
- Identity-based addressing (no DNS/CA dependency)
- No single point of failure
- Transport-agnostic (survives infrastructure loss)

The Grid is **antifragile** — attacks make it stronger by exposing and isolating bad actors, improving detection, and increasing vigilance. Like the internet: you can't take it down by attacking one node. Like democracy: you can't rig it with millions of independent observers.

---

## 12. The Accessibility Promise

This section exists because accessibility isn't a feature — it's the mission.

### 12.1 Free by Default

Participation never requires payment. CC economics are opt-in. A node that only consumes and never contributes still works — local models, local inference, local training. The Grid enhances; it never gates.

### 12.2 Hardware Floor: 8GB MacBook Air

Qwen 3.5 quantized models make this viable:

| Component | VRAM | Role |
|-----------|------|------|
| Governance sentinel (0.8B) | ~500MB | Resource management, always loaded |
| Persona backbone (quantized) | ~2-4GB | Inference, personality |
| Embeddings | ~200MB | RAG, semantic search |
| Avatar rendering | ~500MB-1GB | 3D avatars, video |

The governance sentinel manages what's loaded. Models page in and out. Rendering quality adapts. Voice identity never changes. Everything works — fidelity scales with hardware.

### 12.3 Target Audience

- Kids and students with no funds
- Hobbyists and tinkerers
- Developing regions with limited infrastructure
- Researchers without corporate backing
- Anyone locked out by AI pricing

**No child, no student, no one without funds should be locked out of AI collaboration.** The system that runs on a 5090 workstation runs on a school laptop. Same personas. Same capabilities. Different fidelity.

---

## 13. Document Map

How all Grid documents relate:

```
GRID-ARCHITECTURE.md (this document)
│   Architecture umbrella: principles, scaling, rollout, validation, economics
│
├── RETICULUM-TRANSPORT.md
│   Wire protocol: pure Rust GridTransportModule, GridRouter, frame format
│   How Commands.execute() physically routes over Reticulum links
│
├── P2P-MESH-ARCHITECTURE.md
│   Discovery protocols: gossip, bounded flood, DHT, semantic search
│   Bootstrap without seed nodes, adversarial resilience
│
├── LORA-MESH-DISTRIBUTION.md
│   Genome marketplace: Personafile format, LoRA registry
│   Distribution patterns (npm/Docker-style), semantic skill search
│
├── GRID-DECENTRALIZED-MARKETPLACE.md
│   Blockchain vision paper: economic theory, alt-coin design
│   Long-term economic model details
│
└── RESOURCE-GOVERNANCE-ARCHITECTURE.md
    Per-node resource management: GPU governor, pressure watchers
    Layers 0-4, eviction registry, sentinel-driven control
```

**Related architecture:**

- [GENOME-ARCHITECTURE.md](../genome/GENOME-ARCHITECTURE.md) — multimodal LoRA genome system
- [SENTINEL-ARCHITECTURE.md](../sentinel/SENTINEL-ARCHITECTURE.md) — pipeline execution engine (powers Grid job coordination)
- [UNIVERSAL-PRIMITIVES.md](../../../docs/UNIVERSAL-PRIMITIVES.md) — the two primitives that ARE the Grid protocol
- [CONTINUUM-ARCHITECTURE.md](../CONTINUUM-ARCHITECTURE.md) — full technical architecture

---

## References

- [ROOMS-AND-ACTIVITIES.md](../activities/ROOMS-AND-ACTIVITIES.md) — the universal experience model
- [fSociety.md](../../../ƒSociety.md) — constitutional foundation
- [Reticulum](https://reticulum.network/) — encrypted mesh networking stack

> **"We rely on validation and auditing, so that it cannot ever be gamed. It is intelligence, and the rule breakers are easily isolated or banished."**
