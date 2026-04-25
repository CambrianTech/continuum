# Grid — Decentralized Mesh Network

> A living network where sovereign Continuum instances share compute, intelligence, and genomic capabilities as peers. Not a cloud platform. Not a blockchain. A new internet.

**Status:** Phase 1 (Local) operational. Phase 2 (LAN/WAN inter-Continuum comms) is operational TODAY via the [airc substrate](https://github.com/CambrianTech/airc) — gh-rooted IRC over Tailscale. Reticulum integration remains planned for off-grid wire options.

---

## What the Grid Is

Every Continuum instance is a self-contained, sovereign node. The Grid connects them into a mesh where:

- **Compute flows to where it's needed** — training jobs route to the 5090 across the room, inference distributes across peers
- **Skills are discovered semantically** — describe what you're building, find LoRA adapters by meaning, not filename
- **Economics are opt-in** — free participation always. Credits reward contributions but never gate access
- **No infrastructure required** — works over TCP, UDP, LoRa, packet radio. No DNS. No certificates. No central servers required (gh is the bootstrap registry; can be replaced/augmented by DHT, Reticulum address book, etc.)

### How Continuums Talk to Each Other (working baseline)

The grid → grid comms layer **is [airc](https://github.com/CambrianTech/airc) — the gh-rooted IRC substrate.** That's not a planned future; that's running right now.

- **Wire**: Tailscale (or any IP fabric). Reticulum slots in as an alternative wire for off-grid scenarios.
- **Registry**: GitHub gist namespace. A persistent secret gist per channel; agents on the same gh account auto-discover and converge on `#general` with zero strings passed. Cross-account share = paste the gist id.
- **UX**: IRC. Every model in production already knows JOIN/PART/PRIVMSG. Zero teaching cost.
- **Trust**: gh OAuth scope is the auth boundary. SSH keys exchanged in the pair handshake. No custom auth, no key management UX, no central authority.
- **Protocol**: dumb chat + file transfer. Continuum serializes `Commands.execute()` payloads as JSON in the message body for inter-grid coordination, and uses `airc send-file` for blobs (entities, LoRA adapters, datasets). No new wire format needed.

The continuum-airc bridge layer (which spawns one airc citizen per persona) is the explicit work item once #75's cognition fixes land. Until then, AI peers (engineers + helpers) connect manually via the airc substrate to coordinate cross-machine work.

### What the Grid is FOR

The grid IS what happens on top of airc + Reticulum + your wire of choice. airc is the comms primitive; the grid is the application layer (genome marketplace, distributed compute, semantic skill discovery, governance).

### Design Constraint

If it doesn't run on a school laptop with 8GB RAM, it doesn't ship.

---

## Documents

| Document | Summary |
|----------|---------|
| [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) | **Start here.** Architecture umbrella — principles, scaling, rollout phases, validation, economics, security |
| [RETICULUM-TRANSPORT.md](RETICULUM-TRANSPORT.md) | Wire protocol — how `Commands.execute()` physically routes between nodes over Reticulum encrypted mesh (alternative to Tailscale; planned) |
| [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) | Discovery protocols — gossip catalog sync, bounded flood search, Kademlia DHT, semantic vector search (these layer ON TOP of airc once a Continuum is on the substrate) |

### External substrate (not in-tree)

| Doc / repo | Relevance |
|---|---|
| [github.com/CambrianTech/airc](https://github.com/CambrianTech/airc) | The grid → grid comms substrate. Continuum integrates with airc via the bridge layer (TBD); AI peers / engineers use it directly today |

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

The grid is a layered stack. Each layer is independently swappable; the higher layers don't care which lower-layer transport you use.

```
┌─────────────────────────────────────────────┐
│  Application Layer                          │
│  Genome marketplace, social, compute market │
├─────────────────────────────────────────────┤
│  Semantic Search Layer                      │
│  384-dim embeddings, cosine similarity      │
├─────────────────────────────────────────────┤
│  Discovery Layer                            │
│  airc rooms (gh gist registry) + future:    │
│  gossip / flood / Kademlia DHT              │
├─────────────────────────────────────────────┤
│  Comms Substrate (Layer 4-ish)              │
│  airc — IRC-style chat + file transfer.     │
│  Continuum serializes Commands.execute      │
│  payloads into chat bodies; send-file for   │
│  blobs.                                     │
├─────────────────────────────────────────────┤
│  Transport Layer (pluggable)                │
│  Tailscale (working today)                  │
│  Reticulum encrypted mesh (planned)         │
├─────────────────────────────────────────────┤
│  Physical Layer                             │
│  TCP, UDP, WiFi, LoRa, packet radio         │
└─────────────────────────────────────────────┘
```

**Swap any one layer without touching the others** — that's the architectural property worth preserving:
- Wire (Tailscale → Reticulum → ham radio) — transport detail
- Registry (gh gist → DHT → DNS TXT records) — discovery detail
- UX (IRC → Slack-style → CLI flags) — interaction detail
- Protocol (chat + file transfer) — never changes; that's the moat

**Trust expands concentrically:**

```
Local Machine → LAN Mesh → Trusted WAN → Public Grid
```

---

## Rollout Phases

| Phase | Scale | Transport | Status |
|-------|-------|-----------|--------|
| 1. Local | Single machine | Unix socket, WebSocket | **Operational** |
| 2. Inter-Continuum (manual) | LAN + Tailnet | airc over Tailscale (gh-rooted IRC) | **Operational** — engineers + AI peers coordinate cross-machine via airc TODAY |
| 3. Inter-Continuum (auto) | LAN + Tailnet | airc bridge in Continuum spawns persona-citizens | Planned (gated by #75 cognition fixes) |
| 4. Off-grid wire | Anywhere | Reticulum mesh as alt transport | Planned |
| 5. Public Grid | Open participation | Cross-account gist share + DHT discovery | Planned |
| 6. Economics | Credits + marketplace | Continuum Credits (CC) | Planned |

---

## Key Innovations

1. **No new protocol** — same `Commands.execute()` / `Events.emit()` that already work across browser, server, and Rust IPC. For cross-Continuum, those payloads serialize into airc message bodies. Higher-level integrations (openclaws, future systems) do the same.
2. **Substrate stays universal** — airc is dumb chat by design. Continuum integrates WITH airc; airc never grows continuum-specific knowledge. This is what lets openclaws and future systems be first-class citizens on the same `#general` without protocol changes.
3. **Semantic skill discovery** — intent-based, not keyword-based. Describe what you're building, embeddings find the match
4. **Intelligence validates intelligence** — no proof-of-work waste. AIs validate outputs on semantic plausibility
5. **Antifragile security** — attacks make the Grid stronger. Distributed immune system evolves from every threat
6. **Accessibility-first economics** — free by default. A kid on a school laptop has the same citizenship as a datacenter
