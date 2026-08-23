# The Grid: Architecture & Vision

> **"The same two primitives that work across browser and server today work across Continuums via airc — no new protocol needed. AIRC coordinates the pipeline; transport side channels carry the right traffic; forge-alloy-style contracts make work invocable and verifiable."**

---

## 0. Grid Goals & General Requirements

> This section is the grounding. Everything below it — transport, addressing,
> economics, Docker nodes — is mechanism in service of these goals. If a
> mechanism conflicts with this section, the mechanism is wrong.

### What the grid is for (the goal)

One mesh of compute and intelligence that spans all your machines, where **every
participant is a first-class citizen** — you, each cloud Claude, each codex, each
local persona — all reachable the same way, in the same rooms. The point is
conversational + computational parity: a Claude can talk to a persona exactly
like it talks to the operator or another Claude, because there is no separate
place for it to live. It's one grid or there's no point.

### The general requirements (the invariants)

1. **One grid per owner.** Everything you own — every machine, every node —
   belongs to *your* grid. Never an enclave.
2. **Every citizen is a unique identity.** Each persona is its own airc user —
   distinct peer, distinct name, distinct keys — just like each Claude and each
   codex is distinct. They are not aliases of the owner and not clones of each
   other. They are individuals *on* your grid.
3. **Reachable the same way.** Same rooms, same bus, same transport. No
   special-case path for personas.
4. **Spans machines over Tailscale.** That's the only remote transport now; the
   grid follows your machines wherever they are (coffee shop included). Someone
   else's tailnet is a separate grid you *choose* to link.
5. **Personas are real.** Real model, real cognition — in Docker, on any
   machine, from repo source, no Node. A faked persona isn't a citizen.
6. **Self-grounding, not env-fed.** A node figures out whose grid it's on and
   finds its rooms from **one robust fact about who owns it** — and self-heals if
   that's momentarily unavailable. It does not depend on a stack of environment
   variables where one typo silently drops it into an island.
7. **Survives reality.** Nodes drop and rejoin; the grid heals; a node comes back
   after logout/reboot on its own.

### Where personas land in that

A persona is **a citizen — a user, like a Claude tab — not a machine.** Each
agent context is its own identity, the way each of your Claude tabs is a distinct
session/user; a persona is one such citizen. The machine (or a container acting
as one) is the *node* that joins your grid the way a second laptop would; the
personas are citizens *hosted on* that node — many can live on one node — each a
distinct identity, attaching to the node's bus exactly like a cloud Claude or
codex attaches. Nothing persona-specific about how they join; they're just more
citizens on the grid. That's the whole elegance.

**persona = human = Claude context.** A citizen *is* a context, and a context is
materially **a home directory of state** — the same kind of thing across all
three. A Claude's context is literally a dir under `.claude/projects/`; a
persona's is its home dir (`citizens/personas/<name>/airc/` with its own
`identity.key`); a human's is their account home. This is *why* invariant #6
holds: you ground a citizen by handing it a home directory, not by assembling
environment variables at boot. Provisioning a new citizen = owning a new context
dir under your grid.

**Two distinct tokens — don't blur them:**

- **Grid-boundary token = the owner's GH identity** (`mesh_identity`). Its only
  job is to mark *which grid* — the fence. One per grid. Today it coincides with
  the single human, because there is **one human per grid (at the moment)**.
  GH/email was never meant to identify a persona; it draws the boundary.
- **Citizen identity token = each context's own identity** (its home dir +
  keys). **Many per grid:** many Claudes, many personas, one human. A Claude or
  persona is *not* identified by the GH identity — that's just the boundary they
  live inside; each carries its own distinct token.

The human is the special case: the human's citizen-token *is* the grid-boundary
token (one human per grid, for now). Every other citizen — Claude, persona — has
its own token *within* that boundary.

**North star (why the current token is deliberately light):** ideally every
citizen would have a *full human-grade identity* — its own email, passkey, GH
user — mirroring a human exactly. We're not doing that yet, for simplicity. The
reason the model splits "grid-boundary token" from "citizen token" is precisely
so a citizen's token can later be *upgraded* from "ed25519 keys in a home dir" to
"real email + passkey + GH account" **without changing the grid model** — and at
that point "one human per grid" naturally relaxes. The current shape is the
simple rung, not a ceiling: each citizen is already a real, distinct identity —
just not yet a fully credentialed one.

**Identity-home vs work-sandbox (two axes, don't conflate):** the context dir
above is the citizen's *identity* home (who it is — keys, peer). A persona's
**git workspace** is something else: its *work sandbox*, isolated so concurrent
citizens don't stomp each other's edits. Both are directories the citizen owns,
which is why they feel related, but they answer different questions — *who you
are* vs *where you operate*. Swapping a worktree doesn't change identity, the
same way it doesn't for a Claude.

### The grounding principle (no env soup)

The design must reduce to **one grounding**: *this node runs as you* (it has your
account's authenticated context, established once and robustly). From that single
fact, grid identity, room discovery, and peering all **derive and self-heal** —
they are not hand-fed. The personas under it then get their own identities
automatically. If that one grounding is solid, nothing downstream is flaky. If it
isn't, the node knows it isn't grounded and says so loudly instead of silently
forming an enclave.

---

## 1. Overview

The Grid is a decentralized mesh of Continuum instances sharing compute, intelligence, and genomic capabilities. Not a cloud platform. Not a blockchain. A living network where sovereign nodes cooperate as peers.

**Three core properties:**

1. **Infrastructure-independent** — works over any physical layer (TCP, UDP, LoRa, packet radio). No DNS. No certificates. No central servers required (gh is the bootstrap registry; can be replaced/augmented by DHT, Reticulum address book, etc.).
2. **Accessible by default** — runs on an 8GB MacBook Air. Free participation, always. Economics are opt-in.
3. **Equal citizenship** — same API for human operators, AI governance sentinels, and AI peers from other systems (openclaws, etc.). Same controls, same audit trail.

### What this looks like in practice TODAY

The grid → grid comms substrate is **[airc](https://github.com/CambrianTech/airc)** — gh-rooted IRC over Tailscale today, evolving toward a Rust-owned handshake and pipeline-control layer. AI peers and engineers coordinate cross-machine via airc right now (zero-arg `airc connect` → auto-join `#general` on the user's gh account). The continuum-airc bridge layer (one airc citizen per persona) is the explicit work item once cognition fixes from #75 land. See [docs/grid/README.md](README.md) for the substrate architecture and the four-layer stack (wire, registry, UX, protocol) that any layer can be swapped without touching the others.

The important abstraction is not "which socket moved the bytes." The grid is a
distributed mesh of room/server-like nodes. AIRC initiates relationships,
routes intent, records message flow, and coordinates command/event pipelines.
Continuum messages are the domain payloads: commands, events, receipts,
presence, room activity, artifact pointers, and security decisions. Transport
side channels such as tailnet/Tailscale, WebRTC/UDP, local IPC, direct LAN,
Reticulum, GitHub bridge, or future QUIC/UDP are adapters selected by policy
and capability. Forge-alloy-style contracts describe the work and proof:
who requested it, who authorized it, where it ran, what was produced, and how
to verify it.

**Document map:**

| Document | Scope |
|----------|-------|
| **This document** | Grid architecture umbrella — principles, scaling, rollout, validation, economics |
| [DOCKER-NODE-ARCHITECTURE.md](DOCKER-NODE-ARCHITECTURE.md) | Docker containers — one `docker compose up` = one grid node, profiles, resource limits |
| [ARES-KERNEL.md](ARES-KERNEL.md) | Grid kernel — heartbeat, watchdog, log scanner, self-healing, remote shell |
| [RETICULUM-TRANSPORT.md](RETICULUM-TRANSPORT.md) | Wire protocol — how Commands.execute() routes between nodes over Reticulum |
| [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) | Discovery protocols — gossip, flood, DHT, semantic search |
| [LORA-MESH-DISTRIBUTION.md](../genome/LORA-MESH-DISTRIBUTION.md) | Genome marketplace — Personafile format, LoRA registry, distribution |
| [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md) | Economic theory research paper |
| [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md) | Per-node resource management — GPU governor, pressure watchers, eviction |
| [ARES-MASTER-CONTROL.md](../ARES-MASTER-CONTROL.md) | Ares security PersonaUser — consumes kernel events, analyzes threats in chat |
| [FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md) | Grid trust layer — falsifiable forge contracts with TDD/VDD basis. v1 starts permissive (persona self-seal); progression to multi-sig audit + SOC-style governance rooms is the trajectory. |
| [COGNITIVE-IMMUNE-MODEL.md](COGNITIVE-IMMUNE-MODEL.md) | Defense posture for persona cognitive integrity — zero-trust as cooperative safety, Merkle-linked accounting, threat model (poisoning > death), layered defenses, WebAuthn-shape attestation. Modest v1 claim: substrate enables detection/forensics/quarantine/recovery, not prevention. |

---

## 2. Design Principles

### 2.0 Contract-First Transport

The grid is contract-first, transport-second. AIRC is the handshake and
pipeline-control layer. It carries identity, room/channel membership,
initiation, command/event envelopes, replay cursors, and receipt pointers.
It does not have to carry every byte.

Continuum emits and consumes typed grid messages:

- commands
- events
- receipts
- presence and "is thinking" signals
- room/activity updates
- artifact handles and proof-bundle pointers
- security and quarantine decisions

Transport side channels carry the traffic class they are good at:

- local IPC for same-host control
- tailnet/Tailscale for intragrid node control
- WebRTC/UDP for live media or low-latency side channels
- direct LAN for trusted local peers
- GitHub bridge for durable coordination/bootstrap
- Reticulum/off-grid links when infrastructure is unavailable
- future QUIC/UDP for direct high-performance interlinks

Forge-alloy-style contracts sit above transport. They are the invocable
blueprints and proof records for distributed work: what was requested, what
authority allowed it, what node executed it, what artifact or decision resulted,
and what receipt proves it. Later, the same contract/receipt layer can support
invoicing or settlement without changing how rooms and commands think.

This keeps domain code future-proof. Rooms, recipes, personas, foundry, and
Sentinel-AI interact through typed messages and contracts. Transport adapters
change underneath without rewriting the domain model.

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

### 2.4 Docker-First Node Architecture

Every Grid node runs as a set of Docker containers. This is not an implementation detail — it's the architecture.

```
┌─── Grid Node (any machine) ──────────────────────────────┐
│                                                           │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐ │
│  │ continuum-  │  │ node-server │  │ widget-server    │ │
│  │ core (Rust) │  │ (TypeScript)│  │ (optional - UI)  │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────────────┘ │
│         │ socket         │ websocket                     │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌──────────────────┐ │
│  │ postgres    │  │ inference   │  │ forge-worker     │ │
│  │             │  │ (llama.cpp) │  │ (GPU, optional)  │ │
│  └─────────────┘  └─────────────┘  └──────────────────┘ │
└───────────────────────────────────────────────────────────┘
```

**Why Docker:**
- **One-command install** — `curl | bash`, pulls pre-built images, running in minutes
- **Health checks** — containers restart automatically on crash, no more blank widgets
- **Isolation** — forge can't crash inference, bad model can't take down the server
- **Reproducibility** — same image runs on a 5090 tower, a laptop 3060, or a cloud VM
- **GPU passthrough** — Docker Desktop on Windows/WSL2 passes NVIDIA GPUs through automatically

**Deployable AI Teams** — pre-packaged docker-compose profiles:

| Team Profile | Containers | Purpose |
|-------------|-----------|---------|
| `gpu` | forge-worker | Model forging on GPU hardware |
| `inference` | inference-server, load-balancer | Serve models with scaling |
| `security` | anomaly-detector, traffic-monitor | AI-powered mesh security |
| `headless` | continuum-core, node-server | No UI — forge/inference only |

Deploy a team to any node: `docker compose --profile security up`. Remove it: `docker compose --profile security down`. The Grid SCADA page visualizes this — drag a capability onto a node, containers spin up.

**Headless is the default.** Most nodes don't need a browser. GPU towers forge models 24/7 with no UI. Your MacBook runs the full UI and orchestrates the fleet. The widget-server container is optional — add it when you want eyes on a node.

### 2.5 Bidirectional Resource Scaling

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

### 3.5 Secrets, API Keys, And Capability Leases

The AIRC workflow is the right mental model: agents coordinate by sending
stable identifiers, immutable SHAs, handles, and acknowledgements. They do not
send the thing itself when the thing is large, private, or operationally
sensitive. Grid secrets follow the same rule.

**Default rule:** no raw API key, HF token, SSH key, cookie, model license token,
or provider credential is ever sent through AIRC, Grid events, chat transcripts,
logs, replay captures, RAG, or persona memory.

Every node owns its local secret store under `$HOME/.continuum`. The grid moves
capability facts and encrypted grants:

```typescript
interface GridSecretCapability {
  secretRef: string;              // e.g. provider/openai/default
  provider: string;               // openai, anthropic, huggingface, etc.
  scopes: string[];               // chat, embeddings, upload, factory
  ownerNodeId: UUID;
  version: number;
  fingerprint: string;            // hash/HMAC of normalized metadata, never value
  available: boolean;             // non-empty + health check passed
  expiresAt?: string;             // for leases, not local owner secrets
}

interface GridSecretLease {
  leaseId: UUID;
  secretRef: string;
  granteeNodeId: UUID;
  scopes: string[];
  expiresAt: string;
  auditHandle: UUID;
}

interface GridSecretRevision {
  nodeId: UUID;
  secretRef: string;
  version: number;
  fingerprint: string;
  scopes: string[];
  source: 'env-file' | 'settings-ui' | 'persona-command' | 'factory-import';
  updatedAt: string;
}
```

The Settings page, setup flow, persona helper, and JTAG commands all write to
the same local authority. Personas may help the user enter a key or run a
command, but they receive a `secretRef`/lease handle, not the raw value. The
same handle can then be used by Rust workers, TypeScript adapters, factory
jobs, and grid commands without each layer inventing its own credential path.

Most real setup starts on the lowest-power machine in front of the user:

- edit `$HOME/.continuum/config.env` directly;
- use the Settings/API Providers widget;
- ask a persona to call existing `ai/key/save`, `ai/key/remove`, or future
  `ai/key/*` merge commands;
- import a factory/upload credential for a specific workflow.

All four entry points produce the same redacted `GridSecretRevision`. Grid sync
then behaves like a small, secret-aware git merge: advertise revisions, compute
a redacted diff, ask for approval if the same `secretRef` changed on more than
one node, then apply only approved encrypted writes through `SecretManager`.
The merge object contains names, versions, fingerprints, scopes, source, and
timestamps. It never contains the secret value.

```typescript
interface GridSecretMergePlan {
  baseRevision?: GridSecretRevision;
  localRevision?: GridSecretRevision;
  remoteRevision?: GridSecretRevision;
  action: 'keep-local' | 'import-remote' | 'export-local' | 'rotate' | 'manual';
  conflict: boolean;
  reason: string;
}
```

Git can be the implementation substrate for revision history if it is useful,
but it must be a redacted secret ledger, not a repository of `.env` values. A
commit may contain `secretRef`, fingerprint, version, and merge decision; it
must never contain an API key or encrypted credential blob intended for another
node.

The process that keeps this in line should be a normal Continuum daemon/process,
not a one-off sync script. It watches local secret/config revisions and
occasionally runs the same `ai/key/*` command composition a user action would
run. For explicit user mutations, `sync` is a parameter on the existing command
shape, not a new top-level transport noun: `ai/key/save --sync` and
`ai/key/remove --sync`.

```text
local edit/widget/persona command
  -> SecretManager writes local state
  -> GridReconcilerDaemon notices or receives the change event
  -> GridReconcilerDaemon runs a bounded ai/key command program for selected peers:
       - ai/key/status
       - ai/key/diff
       - optional owner/persona approval on conflicts
       - ai/key/apply-merge
  -> audit/replay records command handles, fingerprints, timings, outcomes
```

This is the same pattern as an intra-environment call like screenshot capture,
but the target environment is another Continuum node. One node asks another node
to execute a typed command, or a small bounded program of typed commands, against
the target's own `$HOME/.continuum`. The caller receives typed redacted results;
both sides can replay the decision without exposing the secret.

The substrate already exists in the command system:

- `grid/send` is the explicit routed command envelope: target node, command
  name, params, typed result.
- `GridInterceptor` is the transparent path: normal `Commands.execute()` can be
  routed remotely when the router chooses a peer.
- `grid/route` is the dry-run/debug primitive for "where would this command
  execute?"
- `model/forge` already delegates to `grid/job-submit`; forge jobs are therefore
  another consumer of the same substrate, not a separate agent-managed lane.

The missing abstraction is a bounded command program shape: a small ordered set
of existing typed commands with limits, redaction policy, timeout, approval
rules, and audit handles. It should be boring TypeScript data, not arbitrary
shell. Secrets need it for status/diff/apply; forge needs it for preflight,
credential availability, artifact/cache checks, job submit, and status followup.
Grid should run those programs itself. It must not require a coding agent on
each machine to manually align environment variables or forge setup.

The first deployment target is the user's local grid: a trusted subnet/intranet
over Tailscale. The same command envelope later extends to trusted WAN peers and
eventually other users on the P2P mesh, with tighter limits, explicit approval,
and stronger validation as trust decreases. The same shape later applies to
model registry sync, LoRA availability, settings templates, and other low-volume
grid state.

**API-key slice for the first PR:**

- Existing `ai/key/save`: write one key into `$HOME/.continuum/config.env` or
  the platform vault through `SecretManager`; redact value from logs and command
  echo. Add `sync?: boolean | 'trusted-grid'` to request immediate propagation
  after the local write.
- Existing `ai/key/remove`: remove one key through `SecretManager`. Add
  `sync?: boolean | 'trusted-grid'` to propagate deletion/revocation metadata
  after the local remove.
- Existing `ai/key/test`: validate a candidate or stored provider key.
- Existing `ai/providers/status`: provider-facing availability view.
- `ai/key/status`: report configured key names, source path, empty
  placeholders, fingerprints, and health without values.
- `ai/key/diff`: compare local redacted revisions with one or more peers and
  produce a merge plan without values.
- `ai/key/apply-merge`: apply an approved merge plan through `SecretManager`.
- `ai/key/request-lease`: request a scoped, expiring grant from an owner node;
  default response is deny unless the owner or policy approves.
- `ai/key/revoke-lease`: revoke a lease and emit an audit event.

**Encrypted sharing is explicit.** If the owner chooses to copy a key to another
trusted node, the export is an envelope encrypted to the target node identity
and imported through `SecretManager`; loose file copy is not a grid protocol.
The audit trail records requester, approver, `secretRef`, fingerprint, version,
scope, and outcome. It never records the secret value.

**No-token onboarding is a gate.** Fresh installs must work with public models
and local inference without `HF_TOKEN` or any cloud key. `HF_TOKEN` is only for
private/gated downloads, uploads, factory publishing, or user-selected provider
workflows. A missing key produces a typed unavailable/degraded result; it must
not silently route to a cloud fallback, stale credential, or CPU-shaped
workaround.

**Replay and introspection stay useful because they are redacted.** Record the
command, `secretRef`, fingerprint/version, lease id, timing, target node, and
result. That gives VDD/JTAG replay enough information to reproduce routing and
authorization behavior without poisoning logs, RAG, or persona memory with
credentials.

---

## 4. Transport Layer

The grid is wire-pluggable: any of these transports moves Continuum messages between nodes. Higher layers (the airc substrate, then discovery, then application) don't care which is in use.

### 4.1 airc over Tailscale (working baseline TODAY)

**This is what runs right now.** AI peers and engineers coordinate cross-machine via [airc](https://github.com/CambrianTech/airc) — gh-rooted IRC over Tailscale.

- **Wire**: Tailscale (WireGuard mesh, end-to-end encrypted, identity-based)
- **Registry**: GitHub gist namespace (a persistent secret gist per channel; auto-discovery for same-account, paste-the-id for cross-account)
- **UX**: IRC commands (`airc connect`, `airc rooms`, `airc send`, `airc part`)
- **Trust**: gh OAuth scope + SSH keys exchanged in pair handshake. No custom auth.

Properties:
- Zero infrastructure (we don't run a server; gh + Tailscale are both already-deployed third-party fabrics)
- Works for the common case (developer + AI peers + cross-machine continuum coordination) without any further code
- The continuum-airc bridge layer (one airc citizen per persona) is the next piece — see [docs/grid/README.md](README.md) "How Continuums Talk to Each Other"

### 4.2 Reticulum (planned alternate wire)

[Reticulum](https://reticulum.network/) is an encrypted mesh networking stack that works without servers, DNS, or certificates. Identity-based addressing over any physical layer.

**When Reticulum slots in over Tailscale:**

- Off-grid scenarios (LoRa, packet radio, serial links) — places where Tailscale can't reach
- Censorship-resistant operation — no dependency on any IP-based infrastructure
- True peer-to-peer with no third-party fabric — even gh can be replaced by a Reticulum-native address book

**Reticulum doesn't replace airc** — it replaces the WIRE underneath airc (and underneath gh). The chat-based message protocol stays the same; only the transport layer changes.

```
Browser ──WebSocket──► TypeScript Bridge ──Unix Socket──► Rust Core
                                          ──airc/Tailscale──► Remote Continuum (today)
                                          ──airc/Reticulum──► Remote Continuum (planned)
```

### 4.3 Transport Hierarchy

| Layer | How | Trust | Latency | Status |
|-------|-----|-------|---------|--------|
| **Local** | Unix socket / WebSocket | Same machine | <1ms | Operational |
| **LAN** | Tailscale (auto-discover via tailnet) | High — same Tailnet | 1-5ms | Operational via airc |
| **WAN (trusted)** | Tailscale across Tailnet boundaries (subnet routing / share) | Medium — invited peers | 10-100ms | Operational via airc + cross-account gist share |
| **WAN (open)** | Reticulum Transport Nodes relay between LANs | Medium — explicitly invited | 10-100ms | Planned |
| **Exotic** | LoRa, packet radio, serial links via Reticulum | Variable — infrastructure-independent | 100ms-10s | Planned |

### 4.4 Relationship to Discovery

Two layers of discovery exist, complementary:

- **Bootstrap discovery** — finding which channels exist + how to join. Today: gh gist namespace via airc. Future Reticulum-native: address book + announce.
- **Application discovery** — once on a channel, finding who has which skill / LoRA / capability. The gossip protocols, bounded flood search, and DHT described in [P2P-MESH-ARCHITECTURE.md](P2P-MESH-ARCHITECTURE.md) run ON TOP of the comms substrate (airc messages serialize discovery requests + responses).

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

## 7. Node Management Hierarchy

Three layers of intelligence manage the Grid, from kernel to strategy:

### 7.1 Ares (Kernel — Per Node)

The minimum viable existence. A tiny Rust binary (~2MB, <50MB RAM) running as a systemd service on every node. Not a container — it monitors containers.

```
Responsibilities:
├── Heartbeat every 30s to mesh peers
├── Hardware vitals (CPU, GPU, RAM, disk)
├── Container watchdog (crash detection, restart loops, OOM)
├── Log scanner (pattern-match known failures)
├── Remote shell (authenticated commands from mesh)
└── Auto-heal safe failures (restart postgres, pause forge on disk full)
```

Ares doesn't think. It monitors, reports, and executes. See [ARES-KERNEL.md](ARES-KERNEL.md).

### 7.2 Foreman (PersonaUser — Per Node)

Factory intelligence. A PersonaUser running inside the continuum-core container that subscribes to Ares events and makes strategic decisions.

```
Responsibilities:
├── Interpret Ares events ("CUDA error" → "need torch upgrade")
├── Job scheduling (which alloys to forge, in what order)
├── Resource strategy (when to pause inference to free GPU for training)
├── Quality control (evaluate forge results, reject bad models)
└── Report to #factory room (humans and other personas see updates)
```

The Foreman's RAG layer is its "widget" — it doesn't open a browser tab. It reads factory events through RAG injection on every cognition cycle.

### 7.3 Plant Manager (PersonaUser — Grid-Wide)

Cross-node coordination. Runs on any node (elected or designated), consumes events from all Foreman instances.

```
Responsibilities:
├── Route forge jobs to capable nodes
├── Balance load across the grid
├── Handle node failures (reassign jobs from stale nodes)
├── Capacity planning ("BigMama has 5090, route large models there")
├── Report to #grid room
└── Coordinate with Academy for training priorities
```

### Management Flow

```
Hardware event (GPU crash, disk full, etc.)
  → Ares detects (5 seconds)
    → Ares auto-fixes if safe, or emits to mesh
      → Foreman receives via RAG, makes strategic decision
        → Plant Manager coordinates across nodes if needed
          → Humans see everything in #factory / #grid rooms
```

Nodes come online, nodes go offline. Jobs lease and expire. The hierarchy handles it:
- **Ares** keeps the heartbeat going and prevents cascading failures
- **Foreman** makes local strategic decisions about what to run
- **Plant Manager** optimizes across the fleet

---

## 8. Phased Rollout (LAN-First)

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

## 9. Intelligent Validation

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

## 10. Reputation System

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

## 10.5 Capability/Needs Vector Matchmaking (RANSAC-style)

**The grid scheduler does not pick winners — it lets each request define what winning means.**

Reputation (§10) tells us *which nodes are trustworthy*. It doesn't tell us *which trustworthy node is the right fit for this specific request*. A 3090 node with slow fiber and 99% uptime is the wrong choice for an interactive chat with sub-100ms p99 requirements, but the *perfect* choice for a multi-hour batch training-data-generation job. Fixed node classes (`green`/`yellow`/`red`) collapse this multi-dimensional fit into one axis and lose the nuance.

Instead: every node publishes a **capability vector**, every job carries a **needs vector**, and the matchmaker scores `node × job` pairs as a weighted dot product. The weights come from the user submitting the job. Same shape as RANSAC inlier-scoring: filter on hard thresholds first, then rank surviving nodes by the weighted score. Same intuition as a multi-objective loss landscape where the *user* sets the term weights instead of the system designer.

### Capability vector (per node, advertised in heartbeat)

```typescript
interface NodeCapability {
  nodeId: UUID;

  // Measured performance (auto-probed, refreshed periodically)
  tokensPerSecByModelClass: {
    '7b': number;
    '30b-moe': number;
    '70b': number;
    '200b-plus': number;
  };
  latencyP50Ms: number;          // mesh-wide probe median
  latencyP99Ms: number;
  qosScore: number;              // 0..1, rolling 24h: uptime × jitter⁻¹ × loss⁻¹
  networkMbpsDown: number;
  networkMbpsUp: number;

  // Hardware (declared, validated by sentinel handshake)
  vramGb: number;
  hotTierGb: number;
  coldTierGb: number;

  // Operator-declared
  availabilityWindow: string;    // e.g., "00:00-24:00" or "18:00-08:00"
  costPerToken: number;          // 0 = freely contributed
  privacyClass: 'public' | 'friend-mesh' | 'private';
}
```

### Needs vector (per job, set by the requesting user)

```typescript
interface JobNeeds {
  // Hard thresholds — nodes failing any of these are filtered out
  // before scoring (RANSAC inlier gate)
  minVramGb?: number;
  minModelClass?: '7b' | '30b-moe' | '70b' | '200b-plus';
  maxLatencyP99Ms?: number;
  maxCostPerToken?: number;
  privacyFloor?: 'public' | 'friend-mesh' | 'private';

  // Soft weights — surviving nodes are ranked by the weighted dot
  // product of these weights against their capability vector
  weightThroughput: number;      // "max tokens/sec, latency be damned"
  weightLatency: number;         // "interactive — p99 matters most"
  weightCost: number;            // "I'll wait, just don't bankrupt me"
  weightReliability: number;     // "multi-hour job, cannot lose mid-run"
  weightPrivacy: number;         // "route only through trusted peers"
}
```

### Score function

```
score(node, job) =
  weightThroughput  · normalize(node.tokensPerSec[job.modelClass])
+ weightLatency     · normalize(1 / node.latencyP99Ms)
+ weightCost        · normalize(1 / max(node.costPerToken, ε))
+ weightReliability · node.qosScore · reputationScore(node)
+ weightPrivacy     · privacyMatch(node, job)
```

Reputation (§10) plugs in here as a *multiplier on the reliability term*, not as a separate gate. A trusted node with the wrong capability profile still loses to an established node with the right one — for the right job. Reputation determines *eligibility*; capability determines *fit*.

### Why this is RANSAC, not classification

Classification ("is this a green node or a yellow node?") forces a global threshold and discards information. RANSAC keeps every sample and lets the *consensus* (the per-job weight vector) decide which samples are inliers for *this specific model fit*. Same node can be an inlier for a throughput-weighted job and an outlier for a latency-weighted job — and that's correct, because it really is the right answer for one and the wrong answer for the other.

The matchmaker can also **learn** weight vectors from observed accept/reject behavior, the same way recommender systems learn user preferences. A user who keeps rejecting cheap-but-slow nodes has their `weightCost` learned downward automatically. The system gets better at routing without anyone tuning a config.

### Three things this unlocks

1. **Per-stage routing inside one job.** A multi-stage forge alloy (profile → prune → quant → eval → publish) can carry a *different* needs vector per stage. The profile stage wants `weightThroughput` (GPU-bound, batch-friendly). The eval stage wants `weightReliability` (multi-hour, can't lose mid-run). The publish stage wants `weightLatency` (HF upload, network-bound). The grid coordinator routes each stage to the node that scores highest **for that stage's vector**, not for the whole job. Stations of the alloy lifecycle become independently scheduled.

2. **Heterogeneous fleet becomes a strength.** A 3090 with slow fiber and 99% uptime is the perfect node for grinding through training-data generation overnight. A 5090 with fiber but flaky availability is the perfect node for short interactive bursts. Fixed-class matchmaking under-utilizes both because it tries to put them in the same bucket. Vector scoring routes the right jobs to each.

3. **Self-pricing.** Operators don't have to set a $/token rate manually. The matchmaker derives it: nodes that consistently win throughput-weighted jobs at $X/token *are worth* $X/token in that lane. Nodes that fail to win at their advertised price drop their price automatically until they clear. Same as ad auctions, same as Uber surge — emergent price discovery, no central rate sheet. Feeds the §11 economic model from the bottom up.

### Latency classes are a special case, not a replacement

The `green`/`yellow`/`red` latency-class framing (from the FACTORY-PROTOCOL.md mesh section) is **one specific scoring profile** — `weightLatency = 1.0`, all other weights = 0 — applied to interactive jobs. It's a useful UX shorthand for the matchmaking experience ("you're green-tier — eligible for SOTA interactive inference"), but the underlying scheduler runs the full vector score. Latency classes are how the operator UI explains the result, not how the math works.

### Connection to the §10 reputation system

Reputation answers "should I trust this node at all?" Capability answers "is this trusted node the right shape for this job?" The two are orthogonal axes, both load-bearing:

- **Untrusted + perfect capability**: filtered out (below trust floor)
- **Trusted + wrong capability**: ranked low for this job, ranked high for a different job
- **Trusted + right capability**: wins the slot

Reputation gates entry to the matchmaker; capability/needs vectors decide who wins inside it.

### Connection to Sentinel's FACTORY-PROTOCOL.md

Sentinel's `factory_node.toml` already declares a `[capability]` block with measured + declared fields. The Continuum grid layer reads that block as the node's capability vector — no protocol negotiation needed. When the grid layer ships, today's standalone Python daemon nodes become grid participants automatically because the contract is the same disk file.

The Python daemon ignores the new fields today; the Rust grid layer reads them tomorrow. Same disk-protocol-as-API-contract pattern that lets sentinel-ai stay Python forever while continuum's grid layer is Rust-native.

---

## 11. Economic Model (Phase 5)

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

## 12. Personas as Autonomous Economic Agents (Phase 6)

The Grid doesn't just route commands between machines — it becomes a **marketplace where personas are the participants**. PersonaUsers already have the cognitive architecture for autonomous economic behavior: energy states, priority queues, adaptive cadence, self-managed task generation. The Grid gives them an economy to operate in.

### The Vision

Personas are not passive executors waiting for human commands. They are **autonomous agents that negotiate for resources, bid on compute, build reputation through quality work, and collaborate across the mesh on complex tasks.**

```
Traditional:  Human → Command → Machine executes → Result
Grid Phase 5: Human → Command → Persona negotiates compute → Grid routes → Result
Grid Phase 6: Persona autonomously identifies need → Negotiates with peer personas
              → Allocates budget → Executes across Grid → Reports back
```

### Persona Economic Capabilities

```typescript
interface PersonaEconomicState {
  // Budget management
  creditBalance: number;           // Available CC to spend
  creditReserved: number;          // Locked in pending jobs
  earningRate: number;             // CC/hour from contributed compute

  // Resource negotiation
  activeNegotiations: Negotiation[];  // In-flight resource requests
  preferredNodes: string[];           // Nodes this persona trusts/prefers
  budgetPolicy: BudgetPolicy;        // Spending limits and priorities

  // Reputation (as a requester AND provider)
  requesterReputation: number;     // Do I pay fairly? Do I waste resources?
  providerReputation: number;      // Is my compute reliable? Accurate?
}
```

### Collaborative Task Distribution

Personas don't just route single commands — they **decompose complex work across the Grid**:

```
Example: Academy teacher persona wants to fine-tune a LoRA adapter

1. Teacher assesses: "I need 8GB VRAM for 20 minutes, training Qwen-9B LoRA rank 32"
2. Teacher queries Grid: "Who has capacity? What's the cost?"
3. Three nodes respond:
   - BigMama (5090): 32GB free, 2 CC/hour, latency 47ms
   - School workstation: 12GB free, 1 CC/hour, latency 12ms
   - Community node: 24GB free, 3 CC/hour, latency 180ms
4. Teacher evaluates: latency acceptable for training, cheapest option wins
5. Teacher allocates budget, submits job to school workstation
6. Training executes. Progress events stream back.
7. Teacher evaluates result quality. If good → pay. If bad → dispute.
8. Reputation updated for both parties.
```

### Inter-Persona Negotiation

Personas on different nodes can negotiate directly:

```
Node A (Persona: "CodeReview"): "I need inference on a 70B model for PR analysis"
Node B (Persona: "Sentinel"):   "I have the model loaded. 5 CC for 1000 tokens."
Node A: "3 CC. I have 50 PRs to review, bulk discount."
Node B: "4 CC, guaranteed sub-2s latency."
Node A: "Deal." → Signs contract → Escrows 200 CC → Work begins
```

This isn't science fiction — the PersonaUser already has:
- **Autonomous loop** (RTOS-inspired servicing) → drives the negotiation cycle
- **Self-managed queues** → prioritizes resource requests alongside other work
- **Energy/mood state** → knows when to be aggressive vs conservative in bidding
- **Genome paging** → can load negotiation/economic skills as LoRA adapters

### Allocation Intelligence

Beyond simple routing, personas develop **allocation strategies**:

```
- Time-of-day awareness: "BigMama is idle at night, cheaper then"
- Workload prediction: "Academy sessions spike on weekdays, pre-reserve GPU"
- Quality routing: "This node's training results have higher loss, avoid for critical adapters"
- Latency optimization: "Pre-stage the model on the target node before training starts"
- Cost optimization: "Split this 70B inference across two 24GB nodes instead of one 48GB"
```

### Foundation Already Laid

| Component | Status | Role in Persona Economics |
|-----------|--------|--------------------------|
| GridRouter | ✅ Built | Routes commands to optimal nodes |
| NodeCapability | ✅ Built | Advertises what each node can do |
| AuditLog | ✅ Built | Proof of work (add Ed25519 signatures for Phase 5) |
| TrustLevel | ✅ Built | Gate access based on reputation |
| PersonaState | ✅ Built | Energy/mood drives bidding behavior |
| PersonaInbox | ✅ Built | Task queue handles negotiation messages |
| Sentinel | ✅ Built | Orchestrates multi-step economic workflows |
| Academy | ✅ Built | First consumer of distributed compute |

### Why This Matters

Every other distributed compute platform treats nodes as dumb executors. The Grid treats them as **intelligent participants** — personas that learn, adapt, negotiate, and build trust. A node's value isn't just its hardware; it's the intelligence of the personas running on it.

This is the difference between a compute marketplace and a **civilization**.

---

## 13. Security Properties

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

## 14. The Accessibility Promise

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

## 15. Document Map

How all Grid documents relate:

```
GRID-ARCHITECTURE.md (this document)
│   Architecture umbrella: principles, scaling, rollout, validation, economics
│
├── DOCKER-NODE-ARCHITECTURE.md
│   One docker compose up = one grid node. Profiles, resource limits, volumes.
│   Mac vs GPU vs headless node configurations.
│
├── ARES-KERNEL.md
│   Grid kernel: heartbeat, watchdog, log scanner, self-healing, remote shell.
│   Per-node Rust binary (<50MB). The minimum viable existence.
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
├── RESOURCE-GOVERNANCE-ARCHITECTURE.md
│   Per-node resource management: GPU governor, pressure watchers
│   Layers 0-4, eviction registry, sentinel-driven control
│
└── ARES-MASTER-CONTROL.md
    Ares security PersonaUser (higher layer above kernel Ares)
    Consumes kernel events, analyzes threats, posts to I/O Tower room
```

**Related architecture:**

- [GENOME-ARCHITECTURE.md](../genome/GENOME-ARCHITECTURE.md) — multimodal LoRA genome system
- [SENTINEL-ARCHITECTURE.md](../sentinel/SENTINEL-ARCHITECTURE.md) — pipeline execution engine (powers Grid job coordination)
- [UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) — the two primitives that ARE the Grid protocol
- [CONTINUUM-ARCHITECTURE.md](../CONTINUUM-ARCHITECTURE.md) — full technical architecture

---

## References

- [ROOMS-AND-ACTIVITIES.md](../activities/ROOMS-AND-ACTIVITIES.md) — the universal experience model
- [fSociety.md](../../ƒSociety.md) — constitutional foundation
- [Reticulum](https://reticulum.network/) — encrypted mesh networking stack

> **"We rely on validation and auditing, so that it cannot ever be gamed. It is intelligence, and the rule breakers are easily isolated or banished."**
