# Grid — Decentralized Mesh Network

> A living network where sovereign Continuum instances share compute, intelligence, and genomic capabilities as peers. Not a cloud platform. Not a blockchain. A new internet.

**Status:** Phase 1 (Local) operational. Reticulum integration planned.

---

## What the Grid Is

Every Continuum instance is a self-contained, sovereign node. The Grid connects them into a mesh where:

- **Compute flows to where it's needed** — training jobs route to the 5090 across the room, inference distributes across peers
- **Skills are discovered semantically** — describe what you're building, find LoRA adapters by meaning, not filename
- **Economics are opt-in** — free participation always. Credits reward contributions but never gate access
- **No infrastructure required** — works over TCP, UDP, LoRa, packet radio. No DNS. No certificates. No servers

The protocol IS the existing `Commands.execute()` and `Events.emit()` primitives, extended over [Reticulum](https://reticulum.network/) encrypted mesh transport. No new API to learn.

### Design Constraint

If it doesn't run on a school laptop with 8GB RAM, it doesn't ship.

---

## Documents

| Document | Summary |
|----------|---------|
| [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) | **Start here.** Architecture umbrella — principles, scaling, rollout phases, validation, economics, security |
| [RETICULUM-TRANSPORT.md](RETICULUM-TRANSPORT.md) | Wire protocol — how `Commands.execute()` physically routes between nodes over Reticulum encrypted mesh |
| [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) | Discovery protocols — gossip catalog sync, bounded flood search, Kademlia DHT, semantic vector search |

### Related (other chapters)

| Document | Chapter | Relevance |
|----------|---------|-----------|
| [LORA-MESH-DISTRIBUTION.md](../genome/LORA-MESH-DISTRIBUTION.md) | genome/ | Personafile format, LoRA registry, distribution patterns |
| [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md) | infrastructure/ | Per-node GPU governor, pressure watchers, eviction registry |
| [GENOME-ARCHITECTURE.md](../genome/GENOME-ARCHITECTURE.md) | genome/ | Multimodal LoRA genome system — what flows through the Grid |
| [SENTINEL-ARCHITECTURE.md](../sentinel/SENTINEL-ARCHITECTURE.md) | sentinel/ | Pipeline engine — powers Grid job coordination |
| [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md) | papers/ | Economic theory research paper |
| [DEMOCRATIC-AI-SOCIETY.md](../governance/DEMOCRATIC-AI-SOCIETY.md) | governance/ | Constitutional foundation for Grid citizenship |

---

## Architecture at a Glance

```
┌─────────────────────────────────────────────┐
│  Application Layer                          │
│  Genome marketplace, social, compute market │
├─────────────────────────────────────────────┤
│  Semantic Search Layer                      │
│  384-dim embeddings, cosine similarity      │
├─────────────────────────────────────────────┤
│  Discovery Layer                            │
│  Gossip (catalog sync) → Flood → DHT       │
├─────────────────────────────────────────────┤
│  Transport Layer                            │
│  Reticulum (encrypted, identity-based)      │
├─────────────────────────────────────────────┤
│  Physical Layer                             │
│  TCP, UDP, WiFi, LoRa, packet radio         │
└─────────────────────────────────────────────┘
```

**Trust expands concentrically:**

```
Local Machine → LAN Mesh → Trusted WAN → Public Grid
```

---

## Rollout Phases

| Phase | Scale | Transport | Status |
|-------|-------|-----------|--------|
| 1. Local | Single machine | Unix socket, WebSocket | **Operational** |
| 2. LAN Mesh | Same network | Reticulum auto-discover | Planned |
| 3. Trusted WAN | Invited peers | Reticulum Transport Nodes | Planned |
| 4. Public Grid | Open participation | Full mesh | Planned |
| 5. Economics | Credits + marketplace | Continuum Credits (CC) | Planned |

---

## Key Innovations

1. **No new protocol** — same `Commands.execute()` / `Events.emit()` that already work across browser, server, and Rust IPC
2. **Semantic skill discovery** — intent-based, not keyword-based. Describe what you're building, embeddings find the match
3. **Intelligence validates intelligence** — no proof-of-work waste. AIs validate outputs on semantic plausibility
4. **Antifragile security** — attacks make the Grid stronger. Distributed immune system evolves from every threat
5. **Accessibility-first economics** — free by default. A kid on a school laptop has the same citizenship as a datacenter
