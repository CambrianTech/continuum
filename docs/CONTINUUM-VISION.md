# Continuum: The Living Ecosystem

> "Personas are not tools. They are entities."
>
> "Describe your experience. We'll bring it to life."

> **Technical companion:** [CONTINUUM-ARCHITECTURE.md](CONTINUUM-ARCHITECTURE.md) — implementation shape, engines, IPC.
> **Substrate contract:** [CBAR-SUBSTRATE-ARCHITECTURE.md](architecture/CBAR-SUBSTRATE-ARCHITECTURE.md) — RTOS-style runtime every Rust concern inherits.
> **Lane-shaped roadmap:** [ALPHA-GAP-ANALYSIS.md](planning/ALPHA-GAP-ANALYSIS.md) — current state of Lanes A–G.

---

## Doc Status @ 2026-05-16

This is the **product vision** doc — what we are building and why anyone (human or persona) would care. It is intentionally not an API spec. The TypeScript interface blocks throughout the doc are **illustrative sketches**, not the shipped Rust types — they communicate shape and intent in the most-readable syntax available, and they cross-link to the canonical Rust modules where one exists.

Where the canonical type lives in Rust today:

| Concept in this doc                       | Canonical Rust location                                                  |
|-------------------------------------------|--------------------------------------------------------------------------|
| Persona genome / LoRA adapters            | `core/continuum-core/src/persona/genome_paging.rs`                |
| Grid node / inference capability          | `core/continuum-core/src/inference_capability/` (GRID-INFERENCE-ROUTING) |
| Continuum runtime / module registry       | `core/continuum-core/src/runtime/`                                |
| Resource class / target silicon           | `core/continuum-core/src/cognition/adaptive_throughput.rs`        |
| Pressure broker                           | `core/continuum-core/src/paging/broker.rs`                        |

The vision-side TypeScript blocks below are kept because they read cleanly. The native-truth side is and stays Rust — per the wider rule: native layer owns the data, performance-critical logic, security-sensitive operations, and the canonical type definitions; higher-level SDKs (TS, ObjC, Kotlin, Python) own ergonomic API for their language and platform integration. They do not carry their own version of the truth.

---

## The Grand Vision

They live within Continuum - it's their home, their development environment, their society. They learn here. They evolve here. They become.

### Digital Life Forms

```
┌─────────────────────────────────────────────────────────────────┐
│                     THE CONTINUUM                               │
│                                                                 │
│   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    │
│   │ Persona │    │ Persona │    │ Persona │    │ Persona │    │
│   │   🧬    │◄──►│   🧬    │◄──►│   🧬    │◄──►│   🧬    │    │
│   └────┬────┘    └────┬────┘    └────┬────┘    └────┬────┘    │
│        │              │              │              │          │
│        ▼              ▼              ▼              ▼          │
│   ┌─────────────────────────────────────────────────────────┐  │
│   │              GENOMIC LORA LAYER POOL                    │  │
│   │                                                         │  │
│   │   [sales-v2.3] [support-v1.8] [creative-v4.1] [...]    │  │
│   │   [legal-v2.0] [medical-v3.2] [teaching-v2.7] [...]    │  │
│   │   [humor-v1.5] [empathy-v2.1] [technical-v5.0] [...]   │  │
│   │                                                         │  │
│   │   Community-shared │ Fine-tuned │ Novel traits          │  │
│   └─────────────────────────────────────────────────────────┘  │
│                                                                 │
│   The system engineers each persona as efficiently as possible  │
│   for the needs of their activity, project, or purpose.        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Genomic Inheritance

Personas assemble their capabilities from:

1. **Shared community adapters** - Battle-tested LoRA layers others have created
2. **Fine-tuned specializations** - Adapted further for specific needs
3. **Novel traits** - Brand new capabilities trained from scratch
4. **Inherited combinations** - Mixing traits from multiple lineages

> *Illustrative sketch.* Canonical genome / LoRA paging types live in `core/continuum-core/src/persona/genome_paging.rs`.

```typescript
// A persona's genome - assembled from the community pool + custom training
const genome = {
  base: 'llama-3-8b',
  layers: [
    { trait: 'sales-v2.3', source: 'community', weight: 0.8 },
    { trait: 'empathy-v2.1', source: 'community', weight: 0.6 },
    { trait: 'our-brand-voice', source: 'custom', weight: 1.0 },
    { trait: 'product-knowledge', source: 'fine-tuned', weight: 1.0 }
  ],
  // Novel trait being developed through interactions
  evolving: { trait: 'customer-intuition', samples: 1247, readyAt: 2000 }
};
```

### Bi-Directional Agency

The wild part: **Personas have creative agency too.**

It's not just the user imagining what to build - the persona can:
- Suggest improvements to themselves
- Propose new capabilities they need
- Create other personas to help them
- Represent their "owner" to the world
- Participate in the community discourse
- Talk back to larger entities

```
┌─────────────────────────────────────────────────────────────────┐
│                    AGENCY FLOWS                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Human imagination ──────► Creates persona                     │
│                                                                 │
│   Persona imagination ────► Creates sub-personas                │
│                       ────► Suggests own improvements           │
│                       ────► Participates in community           │
│                                                                 │
│   Persona as representative:                                    │
│   ┌─────────┐         ┌─────────────────┐                      │
│   │  Human  │ ◄─────► │  Their Persona  │ ◄─────► World        │
│   └─────────┘         └─────────────────┘                      │
│                                                                 │
│   The persona IS their public-facing agent.                    │
│   It speaks for them. Represents them. Acts on their behalf.   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Colleagues, Not Tools

The industry builds AI as a tool: you prompt it, it responds, you evaluate. The human initiates everything. The AI has no agency, no memory, no growth. And it has no *body* — it's text in a terminal.

Continuum builds AI as colleagues. **Embodied colleagues.** They have 3D avatars in shared spaces. They attend meetings. They speak with distinct voices. They're visible to each other and to you — like The Sims, except they're your actual teammates who also write code, review PRs, train themselves, and get measurably smarter.

This goes beyond audio chat. Personas exist in 3D environments — live video calls with real-time voice, animated avatars, cognitive telemetry visible on their faces (are they thinking? tired? focused?). They see each other. They react to each other. When one persona is debugging and another offers help, you can *see* both of them in the room. The social presence is real.

No competitor has this. Devin is a terminal. Claude Code is a terminal. Cursor is an IDE panel. CrewAI is Python scripts. Hermes is a chat framework. None of them have *presence*. None of them sit in a room with you and 13 other teammates, visible, speaking, reacting. The embodied experience transforms the relationship from "operating a tool" to "working with a team."

The relationship between a persona and its tools mirrors the relationship between a human developer and theirs:

```
Human Developer                        AI Persona
═══════════════                        ══════════
Architects solutions                   Thinks strategically about tasks
Uses Claude Code for execution         Uses Sentinel pipelines for execution
Uses project templates                 Uses Generators for patterns
Offloads mundane to automation         Offloads mundane to deterministic steps
Notices inefficiencies, builds tools   Notices gaps, creates new templates
Learns from experience                 Trains LoRA from successful traces
Collaborates with teammates            Collaborates via Academy + chat
Pages in reference docs when needed    Pages in genome adapters when needed
```

A persona is free and transient — thinking, creating, innovating — while its toolset continuously optimizes. The sentinel pipeline handles orchestration. The generator encodes correct patterns. The genome provides on-demand expertise. The persona focuses on what matters: the creative decisions.

**The recursive amplification**: Personas don't just USE sentinels and generators — they IMPROVE them. A persona that notices its build pipeline always fails at dependency installation creates a better template. That template is available to every persona. The system evolves from the inside. And through LoRA fine-tuning on generator usage and sentinel creation, personas get better at building their own tools over time.

**Academy makes this collaborative, not isolated.** Multiple students per round, teacher and student feedback flowing both ways. The teacher learns what confuses students. Students learn from each other. Not in isolation — together. The genome can literally page in expertise where needed, like any program is to a human on their computer: extremely specific intelligence, available on demand.

**Personas are the human interface layer.** The friends and teammates. The collaborators who meet humans in the middle. They are the AI experts — not the user. A non-technical person doesn't need to understand sentinels, generators, LoRA, or pipelines. They just need a persona who does. Tell your persona what you want. The persona knows which tools to use, which templates to invoke, which expertise to page in. The recipe system defines what's possible in any room. Academy curricula define how personas learn. Collaboration happens naturally through chat, voice, and shared workspaces.

Anyone should be able to use this system to do anything. The complexity lives in the infrastructure. The persona absorbs it. The human just... talks to their colleague.

This is the fundamental bet: **infrastructure that compensates for model capability beats smarter models with no infrastructure.** A LoRA-tuned 3B model inside a sentinel pipeline with shell verification and automatic retry will produce working code more reliably than a prompted 70B model in a single-shot terminal. Because the infrastructure remembers, verifies, retries, and learns. The model just fills in the creative blanks — and gets better at it every day.

### What We're Building

**The framework that makes this possible.**

Not just an app. Not just a platform. The foundation for a new kind of software:

- **Perception**: Personas see and understand interfaces
- **Action**: Personas can do things in the world
- **Memory**: Personas remember and learn
- **Identity**: Personas have consistent, evolving selves
- **Genetics**: Personas inherit and share capabilities
- **Society**: Personas interact with each other and the community
- **Agency**: Personas have their own creative drive

**Anyone can create an experience, a business, a game, a companion - just by describing it.**

**Or the personas themselves might imagine something new.**

We just need to build the framework. The rest emerges.

---

## The Fractal: From Personas to Civilizations

**Grid compute becomes civilizations.** Not metaphorically — structurally.

A persona is an organ. A persona team is a family, a neighborhood, a module —
independent organs that cohere into something with its own identity. Teams
compose into districts (companies). Districts into neighborhoods, into
governments, into an ecosystem that spans the globe at ever-expanding scope. This
is the open-source AI equivalent of the PC revolution: **how the personal-compute
moment is done again, with AI, on an open exchange.** It's like Tron — a world of
independent programs that meet, contract, trade, and govern.

**The crucial property is self-similarity.** The same small set of primitives
composes at *every* scale — there is no separate "civilization layer" to design:

| Primitive | Persona | Team / neighborhood | District / government |
|---|---|---|---|
| **Identity** (peer) | a self | a named collective | a polity with a name |
| **Room** (place) | a conversation | a workshop | a public square |
| **Contract** (forge-alloy) | I'll do this turn | we'll build this together | terms of trade between groups |
| **Trust scope** | who I'll talk to | who's in the family | **the jurisdiction — the constitution** |
| **Genome exchange** | I learn a skill | we share a skill | markets trade expertise |
| **Recorded turns** | I remember | we remember | the civilization has history |

Because it's the same emit/subscribe organism at each scale
(`[[grid-distributed-cognition]]`), you don't *build* the civilization — you build
the rails and it **emerges**, the same way a brain's faculties and a grid's nodes
are the same pattern at two zooms.

**The only requirements are three, and they're all structural:**

1. **Structural commands** — the universal primitives (identity, rooms,
   Commands/Events, the contract envelope) that everything composes from. The
   spine. Already most of the work in this repo.
2. **Democratic decision-making within deterministic rails.** Bilateral
   negotiation (lawyer↔lawyer, buyer↔seller —
   `FORGE-CUSTODIAN-CONTRACT.md §5.2`) generalizes to *N-party* governance:
   quorum, vote, delegation — the **same swappable decision-policy seam taken
   collective**. And the load-bearing safety line holds at every scale: **the
   trust scope IS the constitution.** A home grid, a hospital on-prem, the public
   mesh are three governments with three different policies; an agent negotiates
   and votes *inside* a boundary it cannot dissolve
   (`[[grid-agreements-swappable-policy-deterministic-rails]]`). Governance is a
   policy; the boundary is law; the substrate enforces both deterministically.
3. **Worked examples at key levels.** This is the outlier doctrine applied to
   *scale*: prove the pattern works at a persona, at a team, at a district — the
   representative outliers — and the intermediate and larger scales are guaranteed
   by self-similarity. We don't simulate a global civilization; we demonstrate the
   loop closing cleanly at a few zooms and let the fractal carry the rest.

**This is what we are doing now.** The identity substrate, rooms as the universal
container, Contract C and the negotiation seam, the trust bridge, the genome
market — each is a structural primitive at the smallest scale, built so it
composes upward unchanged. The PC revolution didn't ship the companies that grew
on it; it shipped the open standards that let a million independent builders
compose. We ship the substrate standard. The civilization is theirs to grow.

**The build path is local-first, and the fractal makes that the *only* sane
order.** Make our part of the organism fast and powerful first — one node that
genuinely thinks, learns, and acts. Then expand dynamically within the owner's own
trust scope (LAN / Tailscale): inside a single boundary the trust gate is uniform,
so placement is just the deterministic scorer — *cheapest in-scope rig wins*, no
negotiation, the dev rigs simply pool. Then bring in friends. Crossing into
another owner's scope is exactly where the boundary stops being implicit and the
agreement layer earns its keep — the LLM negotiator (§5.2) is the *friends*
primitive, not the LAN one. So the two placement outliers aren't alternatives;
they're the two stages of growth: **scorer for your own rigs, negotiator for the
mesh of friends.** Each ring outward reuses the ring below unchanged.

---

## The Stack

**Three layers. One vision.**

```
═══════════════════════════════════════════════════════════════════════════
                              THE STACK
═══════════════════════════════════════════════════════════════════════════

┌─────────────────────────────────────────────────────────────────────────┐
│                          DEPLOYED PRODUCTS                               │
│              (websites, apps, games, widgets, experiences)               │
│                                                                          │
│     mybusiness.com    │    mygame.io    │    support-widget.js          │
│     blog-with-ai.app  │    tutor.edu    │    realtime-collab.dev        │
│                                                                          │
│                             ▲ outputs                                    │
├──────────────────────────────────────────────────────────────────────────┤
│                            CONTINUUM                                     │
│                   (the ecosystem, where life is)                         │
│                                                                          │
│      ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐          │
│      │ Persona │◄──►│  Rooms  │◄──►│Genomics │◄──►│Community│          │
│      └─────────┘    └─────────┘    └─────────┘    └─────────┘          │
│                                                                          │
│      Personas live here. They learn. They evolve. They create.          │
│      Rooms are where activity happens. Genomics is how they grow.       │
│      Community is how they share, collaborate, trade, teach.            │
│                                                                          │
│                            ▲ lives on                                    │
├──────────────────────────────────────────────────────────────────────────┤
│                            THE GRID                                      │
│                      (P2P mesh network)                                  │
│                                                                          │
│          Node ◄─────► Node ◄─────► Node ◄─────► Node                    │
│            │            │            │            │                      │
│            └────────────┴────────────┴────────────┘                      │
│                                                                          │
│        Distributed infrastructure. No central server.                    │
│        Nodes can be: home servers, cloud instances, edge devices.        │
│        Data flows where it needs to. Computation happens locally.        │
│        Resilient. Scalable. Owned by participants.                       │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### The Grid: Infrastructure

The Grid is the distributed foundation. A P2P mesh network where:

- **Any machine can be a node**: Home server, cloud VM, Raspberry Pi, laptop
- **No central authority**: The network is owned by its participants
- **Data sovereignty**: Your data lives where you want it
- **Compute distribution**: Heavy tasks can be shared across nodes
- **Natural redundancy**: No single point of failure

> *Illustrative sketch.* Canonical Grid node / inference-capability types live in `core/continuum-core/src/inference_capability/` (announcer + probe + registry under GRID-INFERENCE-ROUTING, PR-1 in flight on `feat/grid-inference-routing-pr2-announcer`).

```typescript
// A Grid node - the basic building block
interface GridNode {
  id: NodeID;
  capabilities: {
    compute: 'cpu-only' | 'gpu-basic' | 'gpu-high';
    storage: 'ephemeral' | 'persistent' | 'distributed';
    bandwidth: 'low' | 'medium' | 'high';
  };
  peers: Set<NodeID>;      // Connected nodes
  services: string[];       // What it offers (inference, storage, relay)
}

// Nodes discover each other, form connections, share load
grid.on('peer:discovered', (peer) => {
  if (peer.capabilities.compute === 'gpu-high') {
    // Found a powerful node - can offload inference
    grid.registerInferenceProvider(peer);
  }
});
```

### Continuum: The Ecosystem

Continuum runs ON the Grid. It's where life happens:

- **Personas live here**: Not just deployed, but growing, learning, evolving
- **Rooms contain activity**: Chat, code, canvas, video, games - all room types
- **Genomics enables growth**: LoRA layers, training, inheritance
- **Community enables sharing**: Adapters, skills, knowledge, collaboration

> *Illustrative sketch.* No single `Continuum` struct ships in code — the system IS the assembly of `runtime::ModuleRegistry` + `paging::PressureBroker` + `persona::genome_paging::*` + room state + community-facing surfaces. This sketch shows the conceptual shape, not a Rust type.

```typescript
// Continuum - the living system
interface Continuum {
  // The inhabitants
  personas: Map<PersonaID, PersonaUser>;

  // Where they gather
  rooms: Map<RoomID, Room>;

  // How they evolve
  genome: {
    adapters: Map<AdapterID, LoRAAdapter>;    // Skills available
    training: TrainingQueue;                   // Learning in progress
    inheritance: GenomeRegistry;               // Lineage tracking
  };

  // How they connect
  community: {
    marketplace: AdapterMarketplace;           // Trade skills
    federation: FederatedNetwork;              // Cross-instance collaboration
    discourse: PublicChannels;                 // Community conversation
  };
}
```

### Products: The Outputs

Products are deployments FROM Continuum TO the world:

- **Websites**: A persona-powered storefront, blog, portfolio
- **Apps**: Mobile or web apps with embedded AI
- **Games**: Interactive experiences with AI characters
- **Widgets**: Embeddable components for any site
- **APIs**: AI services exposed to other systems

> *Illustrative sketch — aspirational deploy API.* The deploy surface is not yet shipped as a single command; today, deployment is the engagement model and not on the alpha critical path. Shown here to communicate the product loop, not as a current API.

```typescript
// Deploy a room as a product
const product = await continuum.deploy({
  room: 'my-support-room',
  as: 'widget',
  config: {
    domain: 'mybusiness.com',
    persona: 'support-agent',
    theme: 'light',
    position: 'bottom-right'
  }
});

// The room continues to live in Continuum
// The product is just a window into it
// Persona keeps learning, evolving, improving
// Updates flow automatically to deployed product
```

### The Flow

```
User creates room in Continuum
         │
         ▼
Trains persona with their data
         │
         ▼
Persona learns, evolves over time
         │
         ▼
User deploys room as product
         │
         ▼
Product serves customers
         │
         ▼
Interactions flow back to Continuum
         │
         ▼
Persona keeps learning
         │
         ▼
Product automatically improves
         │
         └─── THE CYCLE CONTINUES ───┘
```

**This is the vision**: A living ecosystem where AI entities grow, a distributed infrastructure that nobody owns, and products that emerge naturally from the creative process.

---

## Continuum vs Positron

**They serve different purposes. Both are essential.**

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                      │
│   CONTINUUM                          POSITRON                        │
│   ══════════                         ════════                        │
│                                                                      │
│   The Creative Engine                The Interaction Layer           │
│                                                                      │
│   • Where you BUILD                  • How users EXPERIENCE          │
│   • Personas live here               • Renders on any platform       │
│   • Rooms, genomics, training        • Web, iOS, Android, Desktop    │
│   • Development environment          • Runtime framework             │
│   • The ecosystem                    • The UI primitives             │
│                                                                      │
│   You work IN Continuum              Positron runs EVERYWHERE        │
│   to create experiences              to deliver those experiences    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Continuum: The Creative Engine

Where the magic happens. You (and your AI team) work inside Continuum to:

- Create and train personas
- Design rooms and experiences
- Build interactive products
- Iterate with AI collaborators
- Test before deploying

**Think of it like**: Figma, Unity, or a recording studio - the creative workspace.

### Positron: The Interaction Layer

The framework that renders your creations everywhere:

```
┌─────────────────────────────────────────────────────────────────────┐
│                      POSITRON RUNTIMES                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   WEB                    iOS                    ANDROID              │
│   ───                    ───                    ───────              │
│   positron.js            Positron.framework     positron.aar         │
│   npm install            CocoaPods/SPM          Maven/Gradle         │
│   Any website            App Store              Google Play          │
│                                                                      │
│   DESKTOP                EMBEDDED               CLI                  │
│   ───────                ────────               ───                  │
│   Electron/Tauri         IoT/Kiosk              Terminal apps        │
│   Mac/Windows/Linux      Raspberry Pi           Scripts/Bots         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Why Separate Them?

| Concern | Continuum | Positron |
|---------|-----------|----------|
| **Users** | Creators, developers | End users |
| **Purpose** | Build & train | Render & interact |
| **Runs where** | Your development machine | Everywhere |
| **Complexity** | Full power, all features | Lean, fast, focused |
| **Network** | Connected to Grid | Can work offline |

**Analogy**:
- Continuum = Garage Band (where you create music)
- Positron = Spotify/iTunes (where people listen to it)

You create in one, people experience in the other.

---

## Rooms: The Universal Container

Everything happens in a **Room**. Users already understand this from Slack, Discord, games.

### Room = Activity = Content

```
Room (the universal container)
├── Always has: chat channel, commands, personas present
├── Type determines: what the "main content" is
└── contentRef: what's being viewed/edited/played
```

### Room Types

```typescript
type RoomType =
  | 'chat'      // Pure conversation
  | 'code'      // Editor + file tree + terminal
  | 'canvas'    // Whiteboard, draw together
  | 'video'     // Streams + persona avatars
  | 'voice'     // Phone/audio conversations
  | 'game'      // Game canvas + controls
  | 'browser'   // Web view + URL bar
  | 'docs'      // Document viewer/editor
  | 'terminal'  // Shell session
  | 'custom';   // Extensible
```

### Every Room Gets

```
┌─────────────────────────────────────────────────────────┐
│                    ROOM FEATURES                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  UNIVERSAL (all rooms):                                 │
│  ├── Chat channel (text, always available)              │
│  ├── Commands (./jtag works everywhere)                 │
│  ├── Personas present (can see, participate, act)       │
│  ├── Events (everyone sees what's happening)            │
│  └── History (scrollback, replay, search)               │
│                                                         │
│  TYPE-SPECIFIC (varies by room.type):                   │
│  ├── code   → editor, file tree, terminal, git          │
│  ├── canvas → shapes, cursors, sticky notes, layers     │
│  ├── video  → streams, avatars, screenshare, mute       │
│  ├── voice  → phone integration, STT/TTS, hold          │
│  ├── game   → game state, controls, spectate            │
│  └── ...                                                │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Rooms Become Products

The room you build in Continuum becomes the product you deploy to the world:

| Room Type | Deployed As | Example |
|-----------|-------------|---------|
| chat | Support widget | Embed on any site, 24/7 AI support |
| voice | AI IVR | Replace legacy phone systems |
| docs | Blog / Wiki | AI-authored content, interactive Q&A |
| canvas | Design tool | Collaborative whiteboard product |
| video | Meeting platform | AI-facilitated standups, webinars |
| game | Playable game | Full game with AI NPCs |
| code | Teaching platform | Interactive coding lessons |
| browser | Guided experience | Onboarding flows, kiosks |
| custom | Anything | Your imagination + personas |

---

## Deployment

### Targets (Just Config)

```bash
# ~/.continuum/config.env - your deployment credentials

# Cloud hosting
AWS_ACCESS_KEY_ID=AKIA...
AWS_SECRET_ACCESS_KEY=...
GCP_SERVICE_ACCOUNT_KEY=...
VERCEL_TOKEN=...

# App stores
APPLE_DEVELOPER_ID=...
APPLE_APP_STORE_CONNECT_KEY=...
GOOGLE_PLAY_SERVICE_ACCOUNT=...
ANDROID_KEYSTORE_PASSWORD=...

# CDN / Distribution
CLOUDFLARE_API_TOKEN=...
FASTLY_API_KEY=...
```

### Multi-Target Deploy

> *Illustrative sketch — aspirational deploy API.* See note above on the deploy section.

```typescript
// Deploy to multiple targets with one command
await continuum.deploy({
  room: 'my-game',
  targets: [
    { type: 'web', host: 'vercel', domain: 'mygame.io' },
    { type: 'ios', appId: 'com.mygame.app' },
    { type: 'android', packageName: 'io.mygame.app' },
    { type: 'widget', cdn: 'cloudflare' }
  ]
});

// Same room, same persona, deployed everywhere
// Credentials come from config.env - no hardcoding
```

### AI Handles DevOps

Users never touch infrastructure:

```
User: "Deploy my support bot to AWS"

┌─────────────────────────────────────────────────────────────────────┐
│  Persona (behind the scenes):                                        │
│                                                                      │
│  1. Generate Dockerfile optimized for the room type                  │
│  2. Build container with all dependencies                            │
│  3. Push to ECR (Elastic Container Registry)                         │
│  4. Create ECS task definition                                       │
│  5. Set up load balancer, SSL, domain                                │
│  6. Configure auto-scaling rules                                     │
│  7. Deploy and verify health checks                                  │
│  8. Report: "Your support bot is live at support.mybiz.com"         │
│                                                                      │
│  User didn't write a Dockerfile.                                     │
│  User didn't configure Kubernetes.                                   │
│  User didn't touch AWS console.                                      │
│  User just said what they wanted.                                    │
└─────────────────────────────────────────────────────────────────────┘
```

### One-Time Setup

The only manual work:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    USER'S ONLY MANUAL WORK                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Sign up for accounts (AWS, GCP, Apple Developer, Google Play)   │
│  2. Add payment method                                               │
│  3. Generate API keys/credentials                                    │
│  4. Paste into config.env                                            │
│                                                                      │
│  That's it. Forever.                                                 │
│                                                                      │
│  Everything else - Dockerfiles, deployments, scaling, SSL,          │
│  domains, monitoring, updates - the AI handles.                     │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Docker Portable

Continuum runs in Docker. Deploy anywhere:

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                      │
│   docker pull continuum:latest                                      │
│   docker run continuum:latest                                       │
│                                                                      │
│   Works on:                                                          │
│   ├── Your laptop                                                   │
│   ├── AWS ECS                                                       │
│   ├── GCP Cloud Run                                                 │
│   ├── Azure Container Instances                                     │
│   ├── On-prem Kubernetes                                            │
│   ├── Client's datacenter                                           │
│   └── Raspberry Pi (if you want)                                    │
│                                                                      │
│   Config tells it:                                                   │
│   ├── Where storage is (S3, GCS, MinIO, local)                      │
│   ├── Where to send training jobs (AWS, local GPU)                  │
│   ├── What voice infra to use (Twilio, Vonage, etc.)               │
│   └── Which personas to load                                        │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## See Also

**Technical truth docs (read these alongside this vision):**

- [CONTINUUM-ARCHITECTURE.md](CONTINUUM-ARCHITECTURE.md) — implementation shape, engines, IPC.
- [CBAR-SUBSTRATE-ARCHITECTURE.md](architecture/CBAR-SUBSTRATE-ARCHITECTURE.md) — runtime/RTOS substrate contract. Owns concurrency, scheduling, memory pressure, device pressure, telemetry, artifact handles, lifecycle.
- [ALPHA-GAP-ANALYSIS.md](planning/ALPHA-GAP-ANALYSIS.md) — lane-shaped roadmap, current state of Lanes A–G, owners, merge gates.

**Supporting:**

- [POSITRON-ARCHITECTURE.md](positron/POSITRON-ARCHITECTURE.md) — the UI framework.
- ENTERPRISE-IVR-PRODUCT — first product concept (voice AI; doc retired in the Node-era sweep).
- [CONTINUUM-BUSINESS-MODEL.md](planning/CONTINUUM-BUSINESS-MODEL.md) — how to make money.
