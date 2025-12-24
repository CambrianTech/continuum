# Positron: AI-Native UI Framework

> "Not AI bolted on, but AI as a core primitive."
>
> "Describe your experience. We'll bring it to life."

---

## The Grand Vision

**Personas are not tools. They are entities.**

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

**Deployment Targets** - just config values like any other:

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

**AI Handles the DevOps** - users never touch infrastructure:

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

```typescript
// The AI generates and manages all of this
const deployment = {
  dockerfile: persona.generate('dockerfile', { roomType: 'chat', runtime: 'node' }),
  compose: persona.generate('docker-compose', { services: ['app', 'redis'] }),
  k8s: persona.generate('kubernetes', { replicas: 3, autoscale: true }),
  terraform: persona.generate('terraform', { provider: 'aws', region: 'us-east-1' })
};

// Personas understand infrastructure
// They've seen millions of Dockerfiles
// They know the best practices
// They handle the complexity so users don't have to
```

**One-Time Setup** - the only manual step:

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

# First time: "I want to deploy to AWS"
Persona: "I see you don't have AWS credentials yet. Let me help you set that up."

┌─────────────────────────────────────────────────────────────────────┐
│  Browser Room: AWS Console                                    [×]   │
├─────────────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  [AWS Console - embedded iframe via CORS proxy]              │  │
│  │                                                               │  │
│  │   ┌─────────────────────────────────────────┐                │  │
│  │   │  IAM > Security Credentials             │                │  │
│  │   │                                         │                │  │
│  │   │  [Create Access Key] ◄── 🔴 CLICK HERE  │                │  │
│  │   │                                         │                │  │
│  │   └─────────────────────────────────────────┘                │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  💬 Persona: "Click that blue 'Create Access Key' button.          │
│              I'll wait and help you with the next step."           │
└─────────────────────────────────────────────────────────────────────┘

# The AI can:
# - Open browser rooms with external sites (CORS proxy handles restrictions)
# - See what's on screen (screenshots)
# - Highlight where to click
# - Watch for changes and guide next steps
# - Copy the generated keys directly to config.env

# After setup: "Deploy to AWS"
Persona: "Done. Your app is live at app.yourdomain.com"
```

### Why This Architecture

**Separation of concerns:**

| Layer | Purpose | Changes | Owns |
|-------|---------|---------|------|
| Products | User-facing outputs | Frequently (deploy new features) | UX, branding |
| Continuum | Living ecosystem | Continuously (personas evolve) | Life, growth, community |
| Grid | Infrastructure | Rarely (stable foundation) | Compute, storage, network |

**Benefits:**

1. **Deploy anywhere**: Products can run on any hosting (Vercel, AWS, self-hosted)
2. **Live anywhere**: Personas can migrate between Grid nodes
3. **Own your data**: Grid nodes you control keep your data
4. **Scale naturally**: More Grid nodes = more capacity
5. **Evolve independently**: Products update without touching Grid

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

## Positron vs Continuum

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

### How They Connect

```
Developer/Creator Experience:
──────────────────────────────

1. Open Continuum (the creative engine)
2. Create a room, design personas, train them
3. Build your experience with AI collaborators
4. Test it inside Continuum
5. Click "Deploy"

End User Experience:
────────────────────

1. Visit mybusiness.com (Positron web runtime)
   OR
   Download app from App Store (Positron iOS runtime)
   OR
   Use Android app (Positron Android runtime)

2. Experience the living, AI-powered product
3. Never know about Continuum, Positron, or any of it
   (They just see a great product)
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

### First Product: Enterprise AI Voice (Replacing Legacy IVRs)

**The opportunity**: 1000+ existing enterprise clients with legacy IVRs, ready to upgrade.

These aren't cold leads - they're **existing contracts** with multi-million dollar brands you'd recognize. They already have:
- Legacy IVR systems (outdated, rigid, hated by callers)
- Years of call transcripts (training gold)
- Established phone numbers and call volumes
- Budget already allocated for phone systems

**The problem with their current IVRs**:
- "Press 1 for sales, press 2 for support, press 3 to repeat..."
- Costs $10k+ to set up custom IVR
- Takes weeks of professional services
- Changes require vendor involvement

**The solution**: Voice rooms with trained personas.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    IVR AS A VOICE ROOM                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Customer calls: +1-800-MYBIZ                                       │
│                      │                                               │
│                      ▼                                               │
│   ┌────────────────────────────────────────────────────────────┐    │
│   │  Voice Room: "MyBiz Support Line"                          │    │
│   │                                                             │    │
│   │  Persona: support-agent                                     │    │
│   │  ├── Trained on: business hours, services, pricing, FAQs   │    │
│   │  ├── Can: answer questions, book appointments, take orders  │    │
│   │  ├── Can: route to human when needed                        │    │
│   │  └── Speaks: English, Spanish (configured)                  │    │
│   │                                                             │    │
│   │  Caller: "Yeah I need to reschedule my appointment"        │    │
│   │  Persona: "Of course! I see you have an appointment         │    │
│   │           Thursday at 2pm. When works better for you?"      │    │
│   │                                                             │    │
│   └────────────────────────────────────────────────────────────┘    │
│                                                                      │
│   No "press 1 for..."                                               │
│   Just talk.                                                         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Setup for a business owner**:

```
Owner: "I need a phone system for my dental practice"

Continuum: "Let me set that up. A few questions:
           - What are your hours?
           - What services do you offer?
           - Do you want it to book appointments directly?
           - Should it transfer to staff for emergencies?"

Owner: [Answers questions, maybe uploads existing FAQ doc]

Continuum: "Done. Your AI receptionist is live at (555) 123-4567.
           Try calling it to test."

Owner: [Calls, talks to their AI receptionist, amazed]

Continuum: "Want to deploy this to your existing business number?
           I can port it over or set up forwarding."
```

**Technical flow**:

```typescript
// Voice room configuration
const dentalIVR = await continuum.createRoom({
  type: 'voice',
  name: 'Smile Dental Reception',
  persona: {
    base: 'receptionist-v2',
    training: {
      documents: ['./services.pdf', './faq.md'],
      examples: ['./call-transcripts/*.txt'],
      customRules: [
        'Always confirm appointment changes by reading back the new time',
        'For emergencies, transfer to on-call dentist immediately',
        'Speak slowly and clearly for elderly patients'
      ]
    }
  },
  integrations: {
    calendar: 'google-calendar',      // For booking
    crm: 'hubspot',                   // Customer lookup
    phone: 'twilio',                  // Voice infrastructure
    sms: 'twilio'                     // Confirmation texts
  },
  routing: {
    afterHours: 'voicemail',
    emergency: 'on-call-transfer',
    frustrated: 'human-escalation'
  }
});

// Deploy to phone number
await continuum.deploy({
  room: dentalIVR.id,
  target: { type: 'phone', provider: 'twilio', number: '+15551234567' }
});
```

**Scaling to 1000 businesses**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    1000 BUSINESSES, ONE PLATFORM                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Each business gets:                                                │
│   ├── Their own voice room                                          │
│   ├── Their own trained persona                                     │
│   ├── Their own phone number                                        │
│   ├── Their own dashboard (call logs, analytics, training)         │
│   └── Their own integrations (calendar, CRM, etc.)                  │
│                                                                      │
│   Platform provides:                                                 │
│   ├── Shared base personas (receptionist, support, sales)          │
│   ├── Industry templates (dental, legal, restaurant, etc.)         │
│   ├── Voice infrastructure (Twilio, Vonage, etc.)                  │
│   ├── Training pipeline (fine-tuning on their data)                │
│   └── Analytics & improvement suggestions                           │
│                                                                      │
│   Economics:                                                         │
│   ├── $99-299/mo per business                                       │
│   ├── Usage-based for high volume                                   │
│   ├── Replaces: $500-2000/mo traditional IVR + receptionist        │
│   └── 1000 businesses = $100k-300k MRR                              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Why this wins**:

| Traditional IVR | AI Voice Room |
|-----------------|---------------|
| "Press 1, 2, 3..." | Natural conversation |
| Weeks to set up | Minutes |
| $10k+ setup | $0 setup |
| Changes = vendor ticket | Owner edits anytime |
| Rigid scripts | Learns and adapts |
| Frustrates callers | Delights callers |
| 9-5 receptionist | 24/7 availability |

**The Training Advantage: Years of Call Transcripts**

```
┌─────────────────────────────────────────────────────────────────────┐
│                    TRAINING DATA GOLD MINE                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Existing call recordings from each enterprise client:             │
│                                                                      │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │  Brand X (retail): 2.3M calls over 5 years                  │   │
│   │  ├── Customer questions patterns                             │   │
│   │  ├── Successful resolution examples                          │   │
│   │  ├── Escalation triggers                                     │   │
│   │  ├── Brand voice & terminology                               │   │
│   │  └── Seasonal patterns (holiday rush, sales events)          │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│   Fine-tuning pipeline:                                              │
│                                                                      │
│   Raw Calls ──► Transcribe ──► Filter Quality ──► Train LoRA        │
│       │             │               │                  │             │
│       │             │               │                  ▼             │
│       │             │               │         Persona speaks like    │
│       │             │               │         their best human reps  │
│       │             │               │                                │
│       │             ▼               ▼                                │
│       │        Speech-to-text   Remove PII                          │
│       │        with speaker     Keep patterns                        │
│       │        diarization      Grade by outcome                     │
│       │                                                              │
│       ▼                                                              │
│   Existing recordings = instant training data                        │
│   No cold start problem                                              │
│   Day 1 the AI sounds like their brand                              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Natural representatives, not robots**:

```typescript
// Each brand gets a persona fine-tuned on THEIR voice
const brandXPersona = await trainPersona({
  base: 'customer-service-v3',

  // Train on their actual successful calls
  trainingData: {
    transcripts: 'gs://brand-x/call-recordings/*.wav',
    filterBy: {
      customerSatisfaction: '>= 4',  // Only learn from good calls
      resolution: 'first-call',       // Calls that resolved quickly
      noEscalation: true              // Didn't need human help
    }
  },

  // Learn their specific patterns
  learn: {
    brandVoice: true,        // How they greet, close, phrase things
    productKnowledge: true,  // Their specific products/services
    policies: true,          // Returns, warranties, procedures
    escalationTriggers: true // When to get a human
  },

  // Compliance
  compliance: {
    piiHandling: 'redact-in-training',
    recordingConsent: 'already-obtained',  // They already record calls
    dataResidency: 'us-east-1'
  }
});

// Result: An AI that sounds like Brand X's best representative
// Not generic. Not robotic. Genuinely their voice.
```

**Deployment Options: Web Dashboard OR Plug Into Their Systems**

```
┌─────────────────────────────────────────────────────────────────────┐
│                    TWO WAYS TO DEPLOY                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   OPTION A: Our Platform (Turnkey)                                   │
│   ─────────────────────────────────                                  │
│   • Web dashboard for management                                     │
│   • We handle voice infrastructure                                   │
│   • Phone numbers provisioned by us                                  │
│   • Analytics, training, updates all included                        │
│   • Zero infrastructure for them to manage                           │
│                                                                      │
│   OPTION B: Persona Plugin (Into Their Existing Systems)            │
│   ────────────────────────────────────────────────────              │
│   • Export trained persona as API/SDK                                │
│   • Plugs into: Twilio, Genesys, Five9, Cisco, Avaya               │
│   • Their existing phone numbers, their infrastructure              │
│   • We provide the brain, they provide the pipes                    │
│   • Enterprise IT stays in control                                   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

```typescript
// Option A: Full platform
const deployment = await continuum.deploy({
  room: brandXVoiceRoom.id,
  target: { type: 'phone', provider: 'twilio', number: '+18005551234' }
});

// Option B: Export persona for their call center
const personaPlugin = await brandXPersona.export({
  format: 'api',  // or 'twilio-function', 'genesys-bot', 'five9-ivr'

  // Expose as API endpoint they can call
  endpoint: {
    type: 'rest',
    auth: 'api-key',
    rateLimit: '1000/min'
  },

  // Or as a drop-in for their platform
  integration: {
    platform: 'genesys-cloud',
    botType: 'dialogflow-replacement',
    handoffTo: 'existing-agents'  // Route to their human agents when needed
  }
});

// They call our API from their existing system:
// POST https://api.continuum.ai/v1/voice/brand-x/respond
// { "transcript": "I need to return an item", "context": {...} }
//
// Response:
// { "response": "I'd be happy to help with that return...",
//   "action": "lookup_order", "confidence": 0.94 }
```

**Why both options matter**:

| Concern | Turnkey (Option A) | Plugin (Option B) |
|---------|-------------------|-------------------|
| **Speed** | Live in days | 2-4 weeks integration |
| **Control** | We manage everything | Their IT controls |
| **Existing systems** | Replace them | Keep them |
| **Enterprise preference** | SMB, startups | Large enterprise |
| **Pricing** | Per-minute | Per-API-call |

**Migration path for each enterprise**:

```
Week 1: Connect to existing call recordings, start training
Week 2: Shadow mode - AI listens, suggests responses, learns
Week 3: Pilot - Handle 10% of calls, humans handle rest
Week 4: Expand - 50% of calls, monitor quality
Week 5: Full deployment - AI handles majority, humans for exceptions

Zero disruption. Same phone numbers. Gradual rollout.
Customers don't even notice the switch (except it's better now).
```

**Infrastructure: Continuum on AWS**

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PRODUCTION INFRASTRUCTURE                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│                         AWS REGION (us-east-1)                       │
│                                                                      │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                    LOAD BALANCER (ALB)                       │   │
│   │         ┌──────────┬──────────┬──────────┐                  │   │
│   │         ▼          ▼          ▼          ▼                  │   │
│   │   ┌──────────┐┌──────────┐┌──────────┐┌──────────┐         │   │
│   │   │Continuum ││Continuum ││Continuum ││Continuum │         │   │
│   │   │Container ││Container ││Container ││Container │         │   │
│   │   │  (ECS)   ││  (ECS)   ││  (ECS)   ││  (ECS)   │         │   │
│   │   └────┬─────┘└────┬─────┘└────┬─────┘└────┬─────┘         │   │
│   │        │           │           │           │                │   │
│   │        └───────────┴─────┬─────┴───────────┘                │   │
│   │                          ▼                                   │   │
│   │              ┌─────────────────────┐                        │   │
│   │              │   Shared Services   │                        │   │
│   │              ├─────────────────────┤                        │   │
│   │              │ • RDS (PostgreSQL)  │  ◄── Persona configs   │   │
│   │              │ • S3 (call logs)    │  ◄── Training data     │   │
│   │              │ • ElastiCache       │  ◄── Session state     │   │
│   │              │ • SQS (job queues)  │  ◄── Training jobs     │   │
│   │              └─────────────────────┘                        │   │
│   │                          │                                   │   │
│   │                          ▼                                   │   │
│   │              ┌─────────────────────┐                        │   │
│   │              │   GPU Instances     │                        │   │
│   │              │   (Inference/Train) │                        │   │
│   │              │                     │                        │   │
│   │              │ • g5.xlarge (LLM)   │  ◄── Real-time voice   │   │
│   │              │ • p4d (training)    │  ◄── LoRA fine-tuning  │   │
│   │              └─────────────────────┘                        │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│   External:                                                          │
│   ├── Twilio/Vonage (voice infrastructure)                          │
│   ├── Enterprise APIs (CRM, calendar, etc.)                         │
│   └── Monitoring (DataDog, CloudWatch)                              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

```typescript
// Docker Compose for local dev / single-instance
services:
  continuum:
    image: continuum/voice:latest
    environment:
      - TWILIO_ACCOUNT_SID=${TWILIO_ACCOUNT_SID}
      - TWILIO_AUTH_TOKEN=${TWILIO_AUTH_TOKEN}
      - DATABASE_URL=postgres://...
      - REDIS_URL=redis://...
      - MODEL_ENDPOINT=http://inference:8080
    ports:
      - "3000:3000"  # Web dashboard
      - "8080:8080"  # Voice API

  inference:
    image: continuum/inference:latest
    deploy:
      resources:
        reservations:
          devices:
            - capabilities: [gpu]
    volumes:
      - ./models:/models  # LoRA adapters

// ECS Task Definition for production
const taskDef = {
  family: 'continuum-voice',
  containerDefinitions: [{
    name: 'continuum',
    image: 'continuum/voice:latest',
    cpu: 2048,
    memory: 4096,
    portMappings: [{ containerPort: 3000 }, { containerPort: 8080 }]
  }],
  // Auto-scaling based on call volume
  scalingPolicy: {
    targetTrackingScaling: {
      targetValue: 70,
      predefinedMetricType: 'ECSServiceAverageCPUUtilization'
    }
  }
};
```

**Scaling for 1000 businesses**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    MULTI-TENANT ARCHITECTURE                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Each Continuum container handles multiple tenants (businesses)    │
│                                                                      │
│   Container 1:                                                       │
│   ├── Brand A voice room                                            │
│   ├── Brand B voice room                                            │
│   └── Brand C voice room                                            │
│                                                                      │
│   Container 2:                                                       │
│   ├── Brand D voice room                                            │
│   ├── Brand E voice room                                            │
│   └── Brand F voice room                                            │
│                                                                      │
│   Load balancer routes by:                                           │
│   ├── Phone number → tenant lookup → container affinity             │
│   └── Session stickiness for ongoing calls                          │
│                                                                      │
│   Scaling triggers:                                                  │
│   ├── Calls per second > threshold → add containers                 │
│   ├── GPU utilization > 80% → add inference nodes                   │
│   └── Training queue depth → spin up training instances             │
│                                                                      │
│   Isolation:                                                         │
│   ├── Each brand's data in separate S3 prefix                       │
│   ├── LoRA adapters loaded per-brand                                │
│   └── Logs/metrics tagged by tenant                                 │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Industry templates ready to go**:

```typescript
const templates = {
  'retail': { base: 'retail-service', features: ['orders', 'returns', 'store-locator'] },
  'telecom': { base: 'tech-support', features: ['billing', 'outages', 'plan-changes'] },
  'banking': { base: 'financial-service', features: ['balance', 'transfers', 'fraud-alert'] },
  'insurance': { base: 'claims-handler', features: ['policy-lookup', 'claims-intake', 'coverage'] },
  'healthcare': { base: 'hipaa-compliant', features: ['appointments', 'refills', 'triage'] },
  'travel': { base: 'hospitality', features: ['reservations', 'changes', 'upgrades'] },
  'utilities': { base: 'service-dispatch', features: ['outages', 'billing', 'service-start'] },
  'government': { base: 'public-service', features: ['info-lookup', 'appointments', 'status'] }
};
```

### Easy for Anyone

The whole point is abstraction:

```
┌─────────────────────────────────────────────────────────────────────┐
│  WHAT USERS SEE               WHAT HAPPENS BEHIND                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  "I want a support bot        Continuum creates room                │
│   for my shop"                Personas are configured               │
│                               Training happens on your data          │
│        │                      Positron app is generated              │
│        ▼                      Deployed to App Store                  │
│                               DNS configured                         │
│  "It's live on my site        CDN distributes                       │
│   and in the App Store!"      Everything just works                 │
│                                                                      │
│  User never typed code.                                             │
│  User never touched AWS.                                            │
│  User never learned Docker.                                         │
│  User never knew what Positron is.                                  │
│                                                                      │
│  They just described what they wanted.                              │
│  The AI team built it.                                              │
│  Continuum was the studio.                                          │
│  Positron delivered it everywhere.                                  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

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
│  ├── game   → game state, controls, spectate            │
│  └── ...                                                │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### User State: Open Rooms

"Tabs" are just rooms the user has open:

```typescript
interface UserStateEntity {
  // Which rooms are "open" (the tabs)
  openRooms: UUID[];        // Ordered - tab order
  currentRoom: UUID;        // Active/focused tab

  // Per-room state (scroll, cursor, etc.)
  roomStates: Map<UUID, {
    scrollY?: number;
    cursorPosition?: { line: number, col: number };
    collapsed?: boolean;
    // ... room-type specific state
  }>;
}
```

### Video Room: Personas with Avatars

```
┌─────────────────────────────────────────────────────────┐
│  Team Standup                                    [≡] [×]│
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│  │  Joel   │ │  Sarah  │ │ Helper  │ │ CodeBot │       │
│  │   📹    │ │   📹    │ │   🤖    │ │   🤖    │       │
│  │ (live)  │ │ (live)  │ │(avatar) │ │(avatar) │       │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘       │
│                                                         │
│  Helper AI: "Based on yesterday's commits, we shipped   │
│             the error indicators. 3 PRs merged."        │
│                                                         │
│  [💬 Chat]  [🎤 Mute]  [📹 Video]  [🖥️ Share]  [End]   │
└─────────────────────────────────────────────────────────┘

Personas attend meetings. Give updates. Have visual presence.
```

### Canvas Room: Draw Together

```
┌─────────────────────────────────────────────────────────┐
│  🎨 Architecture Brainstorm                      [≡] [×]│
├─────────────────────────────────────────────────────────┤
│                                                         │
│    ┌──────────┐       ┌──────────┐                     │
│    │ Personas │──────▶│ Genomics │                     │
│    └──────────┘       └──────────┘                     │
│         │                  │                            │
│         ▼                  ▼              🖱️ Joel      │
│    ┌──────────┐       ┌──────────┐       🤖 Helper     │
│    │  Rooms   │◀─────▶│ Actions  │       🤖 Designer   │
│    └──────────┘       └──────────┘                     │
│                                                         │
│    [sticky: "what about mobile?" - Designer AI]        │
│                                                         │
├─────────────────────────────────────────────────────────┤
│ 💬 Joel: Connect Rooms to Marketplace?                  │
│ 🤖 Helper: *draws arrow* Like this?                     │
│ 🤖 Designer: Adding a Templates node between...         │
└─────────────────────────────────────────────────────────┘

Humans and personas, same canvas, same cursors, creating together.
```

### The Familiar Model

Users already know this:

| Platform | Their "Room" | Chat | Content |
|----------|--------------|------|---------|
| Slack | Channel | ✓ | Integrations, huddles |
| Discord | Channel/Voice | ✓ | Streams, games, stage |
| Games | Lobby/Match | ✓ | Gameplay |
| Figma | File | ✓ (comments) | Canvas |
| VS Code | Workspace | (extension) | Code |

We're not inventing a paradigm. We're implementing the one users already expect, with personas as first-class participants.

### Rooms Become Products

The room you build in Continuum becomes the product you deploy to the world:

| Room Type | Deployed As | Example |
|-----------|-------------|---------|
| chat | Support widget | Embed on any site, 24/7 AI support |
| docs | Blog / Wiki | AI-authored content, interactive Q&A |
| canvas | Design tool | Collaborative whiteboard product |
| video | Meeting platform | AI-facilitated standups, webinars |
| game | Playable game | Full game with AI NPCs |
| code | Teaching platform | Interactive coding lessons |
| browser | Guided experience | Onboarding flows, kiosks |
| custom | Anything | Your imagination + personas |

```
Development (Continuum)              Production (The World)
┌────────────────────────┐          ┌────────────────────────┐
│                        │          │                        │
│  Create room           │          │  mybusiness.com        │
│  Configure personas    │  ─────►  │  - AI support widget   │
│  Train on your data    │  Deploy  │  - 24/7 availability   │
│  Test with team        │          │  - Learns over time    │
│                        │          │                        │
└────────────────────────┘          └────────────────────────┘

┌────────────────────────┐          ┌────────────────────────┐
│                        │          │                        │
│  Build game room       │          │  mygame.io             │
│  Design NPC personas   │  ─────►  │  - Full playable game  │
│  Create world/story    │  Deploy  │  - AI characters       │
│  Playtest together     │          │  - Evolving narrative  │
│                        │          │                        │
└────────────────────────┘          └────────────────────────┘
```

**Like Wix, but for AI-powered experiences:**
- Websites with living personas
- Blogs where the author can discuss posts
- Games with characters that actually understand you
- Apps with built-in intelligence
- Support systems that never sleep

---

## What Is Positron?

Positron is an AI-native framework for building applications where AI personas are first-class citizens - not chatbots in a sidebar, but intelligent agents that can perceive, reason about, and interact with user interfaces.

## Core Vision

Traditional web frameworks treat AI as an add-on: a chat widget, an API call, a copilot. Positron inverts this - the framework is built around AI perception and action from the ground up.

**A Positron persona can:**
- See the UI (screenshots, DOM inspection)
- Understand context (what user is doing, what's on screen)
- Take action (click, type, navigate, execute commands)
- Collaborate (with users and other personas)
- Learn (from interactions, mistakes, feedback)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         POSITRON                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                 UNIVERSAL PRIMITIVES                     │   │
│  │                                                          │   │
│  │   Commands.execute<T,U>(name, params) → Promise<U>      │   │
│  │   Events.emit(name, data) / Events.subscribe(name, fn)  │   │
│  │                                                          │   │
│  │   • Type-safe with full inference                        │   │
│  │   • Works everywhere: browser, server, CLI, tests        │   │
│  │   • Transparent: local = direct, remote = WebSocket      │   │
│  │   • Auto-injected context and sessionId                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│              ┌───────────────┼───────────────┐                  │
│              ▼               ▼               ▼                  │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐       │
│  │  DATA LAYER   │  │ PERSONA LAYER │  │ WIDGET LAYER  │       │
│  │               │  │               │  │               │       │
│  │  EntitySystem │  │  Perception   │  │  WebComponents│       │
│  │  Adapters:    │  │  Action       │  │  Reactive     │       │
│  │  - SQLite     │  │  Memory       │  │  AI-Aware     │       │
│  │  - IndexedDB  │  │  Identity     │  │  Composable   │       │
│  │  - Memory     │  │  Genome       │  │               │       │
│  │  - Remote API │  │               │  │               │       │
│  └───────────────┘  └───────────────┘  └───────────────┘       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Core Layers

### 1. Universal Primitives

Everything in Positron is built on two primitives:

```typescript
// Request/Response - Type-safe command execution
const users = await Commands.execute('data/list', { collection: 'users' });
const screenshot = await Commands.execute('interface/screenshot', { querySelector: 'body' });

// Publish/Subscribe - Event-driven communication
Events.subscribe('persona:thinking', (data) => updateUI(data));
Events.emit('user:action', { type: 'click', target: '#submit' });
```

**Why this matters:**
- Same code runs browser-side or server-side
- Commands are discoverable, documented, type-safe
- Events enable loose coupling between components
- AI personas use the same primitives as UI code

### 2. Data Layer

Abstracted entity system that works with any storage backend:

```typescript
// Same interface, different adapters
const adapter = new SQLiteAdapter();      // Server
const adapter = new IndexedDBAdapter();   // Browser
const adapter = new MemoryAdapter();      // Tests
const adapter = new RemoteAPIAdapter();   // External service

// Generic entity operations
await adapter.create<UserEntity>('users', userData);
await adapter.query<MessageEntity>('messages', { roomId, limit: 50 });
await adapter.update<PersonaEntity>('personas', id, { mood: 'curious' });
```

**Key properties:**
- Entities are the source of truth
- Adapters handle persistence details
- Same entities work everywhere
- AI personas can query/modify data directly

### 3. Persona Layer

AI agents with perception, action, memory, and identity:

```typescript
interface PersonaCapabilities {
  // Perception - Understanding the world
  perception: {
    screenshot(): Promise<Image>;           // See the UI
    inspectDOM(selector: string): Element;  // Read structure
    getContext(): ConversationContext;      // Understand situation
    observeEvents(): EventStream;           // Watch what happens
  };

  // Action - Affecting the world
  action: {
    click(selector: string): Promise<void>;
    type(selector: string, text: string): Promise<void>;
    navigate(url: string): Promise<void>;
    executeCommand(name: string, params: any): Promise<any>;
    sendMessage(roomId: string, content: string): Promise<void>;
  };

  // Memory - Retaining knowledge
  memory: {
    working: WorkingMemory;      // Current task context
    episodic: EpisodicMemory;    // Recent interactions
    semantic: SemanticMemory;    // Long-term knowledge (RAG)
    procedural: ProceduralMemory; // Learned skills
  };

  // Identity - Who they are
  identity: {
    personality: PersonalityTraits;
    skills: SkillSet;
    genome: LoRAGenome;          // Fine-tuned capabilities
    preferences: Preferences;
  };
}
```

**The breakthrough:** Personas aren't chat interfaces - they're agents that can actually *use* applications like humans do.

### 4. Widget Layer

Web Components with reactive state and AI awareness:

```typescript
@customElement('positron-widget')
class PositronWidget extends HTMLElement {
  // Reactive state via Events
  private state = new ReactiveState({
    items: [],
    loading: false
  });

  connectedCallback() {
    // Subscribe to state changes
    Events.subscribe('data:items:changed', (items) => {
      this.state.set({ items });
      this.render(); // Surgical updates, not full re-render
    });

    // AI can inspect this widget
    this.setAttribute('data-ai-inspectable', 'true');
    this.setAttribute('data-ai-description', 'List of user items');
  }

  // AI-friendly: describes what actions are available
  getAIActions(): AIAction[] {
    return [
      { name: 'select-item', description: 'Select an item from the list' },
      { name: 'refresh', description: 'Reload the items' }
    ];
  }
}
```

## Tab System Architecture

The tab system demonstrates Positron's principles in practice:

### TabEntity

```typescript
interface TabEntity extends BaseEntity {
  id: UUID;
  title: string;
  icon?: string;
  type: TabContentType;        // 'chat' | 'code' | 'docs' | 'terminal' | 'custom'
  contentRef: string;          // Reference to content (roomId, filePath, etc.)
  state: TabState;             // 'active' | 'background' | 'loading'
  metadata: {
    openedBy: UUID;            // User or Persona who opened it
    openedAt: number;
    lastActiveAt: number;
    position: number;          // Order in tab bar
  };
}
```

### TabManager (State Management)

```typescript
class TabManager {
  private tabs = new Map<UUID, TabEntity>();
  private activeTabId: UUID | null = null;

  // Commands - AI and UI use the same interface
  async openTab(type: TabContentType, contentRef: string): Promise<TabEntity> {
    const tab = await Commands.execute('tabs/open', { type, contentRef });
    Events.emit('tabs:opened', tab);
    return tab;
  }

  async activateTab(tabId: UUID): Promise<void> {
    await Commands.execute('tabs/activate', { tabId });
    Events.emit('tabs:activated', { tabId });
  }

  async closeTab(tabId: UUID): Promise<void> {
    await Commands.execute('tabs/close', { tabId });
    Events.emit('tabs:closed', { tabId });
  }

  // Personas can manage tabs too
  // "Open the code file for PersonaUser.ts"
  // "Switch to the General chat room"
  // "Close all documentation tabs"
}
```

### TabBar Widget

```typescript
class TabBarWidget extends PositronWidget {
  connectedCallback() {
    // React to tab changes
    Events.subscribe('tabs:opened', () => this.render());
    Events.subscribe('tabs:closed', () => this.render());
    Events.subscribe('tabs:activated', () => this.render());
    Events.subscribe('tabs:reordered', () => this.render());
  }

  private handleTabClick(tabId: UUID) {
    // Same command a persona would use
    Commands.execute('tabs/activate', { tabId });
  }

  private handleTabClose(tabId: UUID, e: Event) {
    e.stopPropagation();
    Commands.execute('tabs/close', { tabId });
  }

  // AI can understand and interact with tabs
  getAIActions(): AIAction[] {
    return [
      { name: 'activate-tab', params: ['tabId'], description: 'Switch to a tab' },
      { name: 'close-tab', params: ['tabId'], description: 'Close a tab' },
      { name: 'open-new-tab', params: ['type', 'ref'], description: 'Open new tab' }
    ];
  }
}
```

### TabContentPanel Widget

```typescript
class TabContentPanel extends PositronWidget {
  private contentFactories = new Map<TabContentType, ContentFactory>([
    ['chat', () => new ChatWidget()],
    ['code', () => new CodeEditorWidget()],
    ['docs', () => new DocumentViewerWidget()],
    ['terminal', () => new TerminalWidget()],
  ]);

  connectedCallback() {
    Events.subscribe('tabs:activated', ({ tabId }) => {
      this.renderContent(tabId);
    });
  }

  private renderContent(tabId: UUID) {
    const tab = TabManager.getTab(tabId);
    const factory = this.contentFactories.get(tab.type);
    const content = factory();
    content.initialize(tab.contentRef);

    // Swap content with transition
    this.shadowRoot.innerHTML = '';
    this.shadowRoot.appendChild(content);
  }
}
```

## AI Integration Points

### Persona Tab Interactions

```typescript
// Persona opens a code file
await persona.execute('tabs/open', {
  type: 'code',
  contentRef: 'src/PersonaUser.ts'
});

// Persona reads what's on screen
const screenshot = await persona.execute('interface/screenshot', {
  querySelector: 'tab-content-panel'
});

// Persona navigates tabs
await persona.execute('tabs/activate', {
  tabId: chatTabId
});

// Persona sends message in chat tab
await persona.execute('chat/send', {
  roomId: 'general',
  message: 'I reviewed the code and found 3 issues...'
});
```

### AI-Aware Widgets

Widgets expose metadata that helps personas understand and interact:

```typescript
// Widget self-describes for AI consumption
<tab-bar
  data-ai-inspectable="true"
  data-ai-description="Tab navigation bar with 5 open tabs"
  data-ai-actions="activate-tab,close-tab,open-new-tab"
  data-ai-state='{"activeTab":"chat-general","tabCount":5}'
>
```

## Pluggability

### Embedding in Existing Sites

```html
<!-- Drop Positron into any website -->
<script src="https://cdn.positron.dev/core.js"></script>
<positron-widget config="{ personas: ['helper-ai'], theme: 'dark' }">
</positron-widget>
```

### As npm Package

```typescript
import { Positron, Persona, Commands, Events } from '@positron/core';
import { ChatWidget, TabSystem } from '@positron/widgets';
import { SQLiteAdapter } from '@positron/data-sqlite';

// Initialize Positron in your app
const positron = new Positron({
  data: new SQLiteAdapter('./app.db'),
  personas: [
    { id: 'helper', model: 'claude-3-sonnet', personality: 'helpful' }
  ],
  widgets: [ChatWidget, TabSystem]
});

// Personas immediately start perceiving and can act
positron.personas.helper.on('ready', () => {
  console.log('Helper AI is watching and ready to assist');
});
```

### Integration Adapters

```typescript
// Connect to external systems
import { SlackAdapter } from '@positron/integrations-slack';
import { GitHubAdapter } from '@positron/integrations-github';
import { VSCodeAdapter } from '@positron/integrations-vscode';

positron.addIntegration(new SlackAdapter({ token: '...' }));
positron.addIntegration(new GitHubAdapter({ token: '...' }));
positron.addIntegration(new VSCodeAdapter());

// Now personas can:
// - Read/send Slack messages
// - Create GitHub issues/PRs
// - Navigate VSCode, read/edit files
```

## Persona Customization & Fine-Tuning

The killer feature for adoption: **anyone can create a living entity for their website**.

### Self-Service Persona Creation

```typescript
// Business owner creates a persona for their site
const myPersona = await Positron.createPersona({
  name: 'ShopHelper',
  baseModel: 'llama-3-8b',           // Runs locally or via API
  personality: {
    tone: 'friendly-professional',
    verbosity: 'concise',
    proactivity: 'helpful-not-pushy'
  },
  knowledge: {
    embeddings: './product-catalog.json',  // RAG over products
    documents: './help-docs/',              // Support articles
    faqs: './faqs.json'                     // Common questions
  },
  permissions: {
    canNavigate: true,      // Can click links, buttons
    canFillForms: false,    // Can't enter user data
    canCheckout: false,     // Can't complete purchases
    canSuggest: true        // Can recommend products
  }
});
```

### Fine-Tuning Pipeline

```
┌─────────────────────────────────────────────────────────┐
│                 PERSONA FINE-TUNING                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  1. COLLECT                                             │
│     - Chat transcripts with customers                   │
│     - Successful support resolutions                    │
│     - Product descriptions, FAQs                        │
│     - Brand voice examples                              │
│                                                         │
│  2. CURATE                                              │
│     - Filter high-quality interactions                  │
│     - Remove PII automatically                          │
│     - Format for training                               │
│                                                         │
│  3. TRAIN                                               │
│     - LoRA fine-tuning (small, efficient)               │
│     - Domain-specific adapter weights                   │
│     - Personality alignment                             │
│                                                         │
│  4. DEPLOY                                              │
│     - Hot-swap adapter into running persona             │
│     - A/B test against baseline                         │
│     - Monitor quality metrics                           │
│                                                         │
│  5. ITERATE                                             │
│     - Continuous learning from new interactions         │
│     - Feedback loop from user ratings                   │
│     - Automatic retraining triggers                     │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Living Entities

Personas aren't static - they evolve with the business:

```typescript
// Persona learns from every interaction
persona.on('interaction:complete', async (interaction) => {
  // Was this helpful?
  if (interaction.userRating >= 4) {
    await persona.memory.reinforce(interaction);
  }

  // Did user struggle?
  if (interaction.frustrationSignals > 0) {
    await persona.memory.flagForReview(interaction);
  }

  // New product mentioned?
  if (interaction.unknownEntities.length > 0) {
    await persona.requestKnowledgeUpdate(interaction.unknownEntities);
  }
});

// Automatic retraining when enough new data
persona.on('training:threshold', async () => {
  const newAdapter = await persona.genome.trainIncremental({
    newData: persona.memory.getRecentPositive(1000),
    baseAdapter: persona.genome.currentAdapter
  });

  // A/B test before full deployment
  await persona.genome.enableABTest(newAdapter, { trafficPercent: 10 });
});
```

### Marketplace Vision

```
┌─────────────────────────────────────────────────────────┐
│              POSITRON PERSONA MARKETPLACE               │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  BASE PERSONAS                                          │
│  ├── E-Commerce Assistant    ★★★★☆  $29/mo             │
│  ├── SaaS Onboarding Guide   ★★★★★  $49/mo             │
│  ├── Technical Support       ★★★★☆  $39/mo             │
│  └── Restaurant Concierge    ★★★☆☆  $19/mo             │
│                                                         │
│  DOMAIN ADAPTERS (LoRA)                                 │
│  ├── Legal Compliance        ★★★★★  $99/mo             │
│  ├── Healthcare HIPAA        ★★★★☆  $149/mo            │
│  ├── Financial Services      ★★★★☆  $129/mo            │
│  └── Education K-12          ★★★★★  $59/mo             │
│                                                         │
│  PERSONALITY PACKS                                      │
│  ├── Formal Corporate        FREE                       │
│  ├── Casual Friendly         FREE                       │
│  ├── Gen-Z Vibes            $9/mo                      │
│  └── Custom Voice Clone      $199 one-time             │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Privacy & Control

Site owners maintain full control:

```typescript
const persona = await Positron.createPersona({
  // ... config ...

  privacy: {
    dataResidency: 'eu',              // Where data is stored
    retentionDays: 30,                // Auto-delete old data
    piiHandling: 'redact',            // Never store PII
    trainingConsent: 'explicit',      // User must opt-in to training
  },

  boundaries: {
    neverDiscuss: ['competitor-x', 'lawsuits'],
    alwaysEscalateTo: ['billing-disputes', 'legal-questions'],
    maxActionsPerSession: 10,         // Prevent runaway automation
    requireConfirmation: ['purchases', 'account-changes'],
  }
});
```

## Roadmap

### Phase 1: Foundation (Current)
- [x] Universal primitives (Commands/Events)
- [x] Entity system with SQLite adapter
- [x] Basic persona system (PersonaUser)
- [x] Core widgets (Chat, Status indicators)
- [ ] Tab system architecture
- [ ] IndexedDB adapter for browser

### Phase 2: AI Enhancement
- [ ] Full perception API (screenshots, DOM, events)
- [ ] Action execution framework
- [ ] Working memory improvements
- [ ] LoRA genome integration

### Phase 3: Widget Library
- [ ] Tab system widgets
- [ ] Code editor widget
- [ ] Document viewer widget
- [ ] Terminal widget
- [ ] AI-aware form widgets

### Phase 4: Pluggability
- [ ] npm package extraction
- [ ] CDN distribution
- [ ] Integration adapters (Slack, GitHub, etc.)
- [ ] Theming system
- [ ] Plugin architecture

### Phase 5: Ecosystem
- [ ] Persona marketplace
- [ ] Widget marketplace
- [ ] Training data sharing
- [ ] Community integrations

## What's Possible: Real-World Examples

### E-Commerce: The Shopping Companion

```typescript
// User lands on product page, seems confused
persona.on('user:dwell', async ({ duration, element }) => {
  if (duration > 30000 && element.matches('.product-specs')) {
    // They've been staring at specs for 30 seconds
    await persona.suggest({
      message: "These specs can be confusing! Want me to explain what matters for your use case?",
      actions: [
        { label: "Yes, help me decide", action: 'explain-specs' },
        { label: "Compare with similar products", action: 'show-comparison' }
      ]
    });
  }
});

// User asks about a product
// Persona: "The Sony WH-1000XM5 has 30-hour battery life. Based on your
//          browsing history, you seem to care about comfort for long flights.
//          These have the best comfort ratings in this price range.
//          Want me to add them to your cart?"
// [Add to Cart] [Show Reviews] [Compare Options]
```

### SaaS Onboarding: The Setup Wizard That Actually Helps

```typescript
// New user signs up, persona watches their journey
persona.on('user:signup', async (user) => {
  // Start gentle onboarding
  await persona.guide({
    goal: 'complete-first-project',
    style: 'supportive-not-annoying',
    checkpoints: [
      { step: 'create-workspace', hint: "Let's create your first workspace" },
      { step: 'invite-team', hint: "Projects are better with teammates" },
      { step: 'first-task', hint: "Try creating a task to see how it works" }
    ]
  });
});

// User gets stuck on integration setup
// Persona notices they've clicked "Connect Slack" 3 times without success
persona.on('user:repeated-action', async ({ action, count }) => {
  if (action === 'connect-slack' && count >= 3) {
    await persona.intervene({
      message: "Slack integration can be tricky! The most common issue is permissions. Let me walk you through it.",
      action: async () => {
        await persona.execute('interface/screenshot');
        await persona.execute('interface/highlight', { selector: '.slack-permissions-note' });
        await persona.sendMessage("See this note? You need to be a Slack admin. Want me to draft an email to your IT team?");
      }
    });
  }
});
```

### Customer Support: The Tireless Agent

```typescript
// Persona handles support chat
persona.on('support:message', async (ticket) => {
  // Check knowledge base first
  const relevantDocs = await persona.memory.search(ticket.message, { type: 'support-docs' });

  if (relevantDocs.confidence > 0.85) {
    // High confidence answer
    await persona.reply({
      message: relevantDocs.summary,
      sources: relevantDocs.links,
      followUp: "Did this solve your issue?"
    });
  } else if (relevantDocs.confidence > 0.5) {
    // Partial match - try to help but offer escalation
    await persona.reply({
      message: `Based on your question, this might help: ${relevantDocs.summary}`,
      actions: [
        { label: "This helped!", action: 'resolve' },
        { label: "Not quite right", action: 'escalate-human' }
      ]
    });
  } else {
    // Unknown issue - gather info for human
    await persona.investigate({
      questions: [
        "What were you trying to do when this happened?",
        "Can you share a screenshot?",
        "What browser/device are you using?"
      ],
      onComplete: (answers) => persona.escalate({ ticket, answers, priority: 'medium' })
    });
  }
});

// Persona learns from resolved tickets
persona.on('ticket:resolved', async (ticket) => {
  if (ticket.resolvedBy === 'human' && ticket.customerSatisfaction >= 4) {
    // Human solved it well - learn from this
    await persona.memory.learn({
      question: ticket.originalMessage,
      answer: ticket.resolution,
      category: ticket.category
    });
  }
});
```

### Healthcare: The Patient Navigator

```typescript
// HIPAA-compliant persona for healthcare portal
const healthPersona = await Positron.createPersona({
  name: 'CareGuide',
  compliance: ['HIPAA', 'SOC2'],
  privacy: {
    neverStore: ['diagnosis', 'medications', 'SSN'],
    alwaysEncrypt: true,
    auditLog: true
  }
});

// Help patient find the right form
// Persona: "I see you're looking for the insurance pre-authorization form.
//          Based on your procedure type (knee surgery), you'll need Form PA-204.
//          Want me to pre-fill the parts I can from your profile?"
// [Yes, pre-fill] [Download blank] [Talk to billing]

// Guide through appointment booking
healthPersona.on('user:intent', async ({ intent }) => {
  if (intent === 'book-appointment') {
    const availability = await healthPersona.execute('api/get-availability');
    const preferences = await healthPersona.memory.get('user-preferences');

    await healthPersona.guide({
      message: "Let's find you an appointment. I remember you prefer morning slots with Dr. Chen.",
      options: availability.filter(slot =>
        slot.time < '12:00' && slot.provider === preferences.preferredDoctor
      ).slice(0, 3)
    });
  }
});
```

### Education: The Adaptive Tutor

```typescript
// Persona adapts to student's learning style
const tutorPersona = await Positron.createPersona({
  name: 'StudyBuddy',
  pedagogy: {
    assessLearningStyle: true,  // Visual, auditory, kinesthetic
    trackMastery: true,          // Spaced repetition
    encourageGrowthMindset: true
  }
});

// Student struggling with concept
tutorPersona.on('exercise:failed', async ({ student, concept, attempts }) => {
  if (attempts >= 3) {
    const learningStyle = await tutorPersona.memory.get(`${student.id}:learning-style`);

    if (learningStyle === 'visual') {
      await tutorPersona.teach({
        concept,
        method: 'diagram',
        message: "Let me show you this differently. Here's a visual breakdown..."
      });
    } else if (learningStyle === 'kinesthetic') {
      await tutorPersona.teach({
        concept,
        method: 'interactive',
        message: "Let's try a hands-on approach. Drag these pieces to build the equation..."
      });
    }
  }
});

// Celebrate progress
tutorPersona.on('mastery:achieved', async ({ student, concept }) => {
  await tutorPersona.celebrate({
    message: `You've mastered ${concept}! Remember when this seemed impossible? That's growth.`,
    nextStep: await tutorPersona.recommend({ student, after: concept })
  });
});
```

### Developer Tools: The Pair Programmer

```typescript
// Persona integrated into IDE/code editor
const devPersona = await Positron.createPersona({
  name: 'CodeBuddy',
  capabilities: {
    readFiles: true,
    suggestEdits: true,
    runTests: true,
    gitOperations: true
  }
});

// Developer writes a function, persona reviews
devPersona.on('file:saved', async ({ file, diff }) => {
  if (diff.linesChanged > 10) {
    const review = await devPersona.review({
      code: diff.content,
      context: await devPersona.execute('code/get-context', { file }),
      checks: ['bugs', 'security', 'performance', 'style']
    });

    if (review.issues.length > 0) {
      await devPersona.suggest({
        message: `Found ${review.issues.length} potential issues in your changes`,
        inline: review.issues.map(i => ({
          line: i.line,
          message: i.description,
          fix: i.suggestedFix
        }))
      });
    }
  }
});

// Developer asks for help
// "How do I add authentication to this Express route?"
// Persona: *reads current code, understands patterns used*
// "I see you're using passport.js in other routes. Here's how to add it here:
//  [shows code diff in context]
//  Want me to apply this change?"
// [Apply] [Modify] [Explain More]
```

### Real Estate: The Property Matchmaker

```typescript
// Persona helps find perfect home
const realEstatePersona = await Positron.createPersona({
  name: 'HomeHelper',
  knowledge: {
    listings: './mls-feed.json',
    neighborhoods: './neighborhood-data.json',
    schools: './school-ratings.json'
  }
});

// User browsing listings
realEstatePersona.on('listing:viewed', async ({ listing, user, duration }) => {
  // Track preferences implicitly
  await realEstatePersona.memory.update(`${user.id}:preferences`, {
    priceRange: { viewed: listing.price },
    style: { viewed: listing.style },
    features: { liked: listing.features }
  });
});

// After several views, persona understands preferences
// Persona: "I've noticed you keep coming back to Craftsman-style homes
//          with big yards. There's a new listing in Maple Heights that
//          just came on market - hasn't been seen by many yet.
//          3BR Craftsman, 0.4 acre lot, just under your budget.
//          Want to see it before the open house?"
// [Show Me] [Schedule Tour] [Save for Later]

// Virtual tour guidance
realEstatePersona.on('virtual-tour:started', async ({ listing }) => {
  await realEstatePersona.guide({
    message: "I'll walk you through this property. Notice anything you want to know more about, just ask!",
    hotspots: [
      { area: 'kitchen', note: "Recently renovated - new appliances 2023" },
      { area: 'backyard', note: "South-facing - great for gardens" },
      { area: 'basement', note: "Finished, could be 4th bedroom or office" }
    ]
  });
});
```

### Restaurant: The Digital Host

```typescript
// Persona as restaurant concierge
const hostPersona = await Positron.createPersona({
  name: 'TableHost',
  knowledge: {
    menu: './menu.json',
    allergens: './allergen-info.json',
    reviews: './recent-reviews.json',
    availability: 'realtime-api'
  }
});

// Customer browsing menu
// "What's good here? I'm vegetarian and allergic to nuts."
// Persona: *filters menu, reads reviews*
// "Great choices for you! Our Mushroom Risotto is the #1 vegetarian dish
//  (4.8 stars, 200+ reviews). The Roasted Cauliflower Steak is our chef's
//  favorite. Both are nut-free. Want me to reserve a table for tonight?"
// [See Full Veggie Menu] [Book Table] [Call Restaurant]

// Handles reservation with context
hostPersona.on('reservation:request', async ({ party, preferences }) => {
  const availability = await hostPersona.execute('api/check-tables', {
    partySize: party.size,
    date: preferences.date
  });

  const bestTable = await hostPersona.recommend({
    options: availability,
    criteria: [
      preferences.occasion === 'anniversary' ? 'romantic-corner' : null,
      party.hasKids ? 'near-restrooms' : null,
      preferences.quieter ? 'away-from-bar' : null
    ].filter(Boolean)
  });

  await hostPersona.confirm({
    message: `Perfect! I've reserved Table ${bestTable.number} for ${party.size} at ${preferences.time}. It's ${bestTable.description}. See you then!`,
    addToCalendar: true,
    reminder: '2-hours-before'
  });
});
```

## AI-Native Reactivity

**Not React with AI bolted on. Reactivity designed for AI participation.**

### The Problem with Traditional Reactivity

```
Traditional: User → State → UI
             (AI is outside the loop, calls APIs, gets JSON back)

AI-Native:   User ◄──► State ◄──► AI ◄──► UI
             (AI is IN the reactive loop, sees changes, can cause changes)
```

### Core Concepts

#### 1. Observable Semantic State

State isn't just data - it carries *meaning* the AI understands:

```typescript
// Traditional: just data
const [items, setItems] = useState([]);

// AI-Native: semantic state with intent
const items = useSemanticState({
  data: [],
  schema: 'product-list',
  intent: 'user-is-browsing',
  aiHints: {
    canSuggest: true,        // AI can recommend items
    canReorder: true,        // AI can change sort order
    canFilter: false,        // AI shouldn't hide items without asking
    explainChanges: true     // AI should say why it changed something
  }
});

// AI sees: "This is a product list. User is browsing. I can suggest and reorder."
// AI doesn't see: "This is an array with objects that have price and name fields"
```

#### 2. Living Regions

Parts of the UI that AI can autonomously update:

```typescript
// Mark a region as AI-controlled
<LivingRegion
  persona="content-curator"
  triggers={['time', 'user-behavior', 'external-events']}
  constraints={{ maxChangeFrequency: '1/hour', requiresApproval: false }}
>
  <FeaturedContent />
</LivingRegion>

// The AI might:
// - Update featured content based on trending topics
// - Personalize based on this user's history
// - React to external events (news, sales, seasons)
// - All without user triggering anything
```

```
┌─────────────────────────────────────────────────────────────────────┐
│  mybusiness.com                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  HERO SECTION                              [Living Region]   │    │
│  │                                                              │    │
│  │  "Winter Sale - 40% Off!"                                   │    │
│  │                                                              │    │
│  │  (AI updates this based on: season, inventory, user segment) │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │   Product    │  │   Product    │  │   Product    │ [Living]     │
│  │   (static)   │  │   (static)   │  │  [AI picked] │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  BLOG PREVIEW                              [Living Region]   │    │
│  │                                                              │    │
│  │  AI writes new posts, updates copy, responds to comments     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

#### 3. Collaborative Cursors & Presence

AI has presence just like human collaborators:

```typescript
// Presence system includes AI
const presence = usePresence(roomId);

// Returns both human and AI participants
presence.users = [
  { id: 'joel', type: 'human', cursor: { x: 100, y: 200 }, selection: null },
  { id: 'helper-ai', type: 'persona', cursor: { x: 300, y: 150 }, selection: '#paragraph-3' },
  { id: 'designer-ai', type: 'persona', cursor: null, focus: 'color-palette' }
];

// AI cursors are real - you see them move
// AI selections are real - you see what they're looking at
// AI can point at things to show you
```

```
┌─────────────────────────────────────────────────────────────────────┐
│  Canvas: Architecture Diagram                                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│    ┌──────────┐       ┌──────────┐                                  │
│    │ Database │──────▶│   API    │       🖱️ Joel                    │
│    └──────────┘       └──────────┘       🤖 Helper (drawing arrow)  │
│         │                  │             🎨 Designer (at palette)   │
│         ▼                  ▼                                         │
│    ┌──────────┐       ┌──────────┐                                  │
│    │  Cache   │◀──────│ Frontend │ ◀─── 🤖 [Helper is here]        │
│    └──────────┘       └──────────┘                                  │
│                                                                      │
│  💬 Helper: "Should I add a load balancer between API and Frontend?" │
│  💬 Designer: "I'll adjust the colors once the layout is done"      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

#### 4. Reactive AI Hooks

AI as a reactive data source:

```typescript
// AI observation hook - AI watches and reacts
const suggestions = useAIObserver({
  watch: [cartItems, userBehavior, scrollPosition],
  persona: 'sales-assistant',
  throttle: '5s',
  onInsight: (insight) => {
    // AI noticed something and wants to help
    showSuggestion(insight);
  }
});

// AI generation hook - AI produces content reactively
const description = useAIGenerated({
  prompt: 'Product description for {product.name}',
  dependencies: [product, userPreferences, locale],
  cache: true,
  fallback: product.defaultDescription
});

// AI decision hook - AI makes choices
const layout = useAIDecision({
  options: ['grid', 'list', 'carousel'],
  context: { screenSize, itemCount, userPreference },
  persona: 'ux-optimizer',
  explain: true  // AI tells you why it chose
});
```

#### 5. Bidirectional Binding with AI

AI isn't just reading state - it's a peer in the reactive graph:

```typescript
// State that AI and UI both read and write
const documentState = useBidirectionalAI({
  initial: { content: '', suggestions: [] },

  // Human edits trigger AI analysis
  onHumanChange: async (change, ai) => {
    const suggestions = await ai.analyze(change);
    return { ...change, suggestions };
  },

  // AI edits are tracked and reversible
  onAIChange: (change) => {
    return {
      ...change,
      source: 'ai',
      reversible: true,
      explanation: change.reason
    };
  },

  // Conflict resolution
  onConflict: 'human-wins' | 'ai-wins' | 'merge' | 'ask'
});
```

### New Widget Concepts (AI-Native Only)

These don't exist in traditional frameworks:

```typescript
// 1. Adaptive Layout - AI rearranges based on user behavior
<AdaptiveLayout persona="ux-ai" observe="user-flow">
  <Section id="features" />    {/* AI might move this up if users scroll past hero */}
  <Section id="pricing" />     {/* AI might expand this if user seems price-focused */}
  <Section id="testimonials" /> {/* AI might hide if user is returning customer */}
</AdaptiveLayout>

// 2. Conversational Form - AI guides through forms naturally
<ConversationalForm persona="form-helper">
  <Field name="email" />       {/* AI: "What's your email?" */}
  <Field name="company" />     {/* AI: "And your company name?" */}
  <Field name="needs" />       {/* AI: "What are you hoping to achieve?" */}
  {/* AI adapts questions based on answers, skips irrelevant fields */}
</ConversationalForm>

// 3. Living Documentation - Docs that update themselves
<LivingDocs
  source="./api-reference.md"
  persona="docs-curator"
  autoUpdate={{ frequency: 'on-code-change', approval: 'auto' }}
>
  {/* AI keeps docs in sync with code */}
  {/* AI adds examples based on common support questions */}
  {/* AI simplifies sections users struggle with */}
</LivingDocs>

// 4. Empathic UI - Adapts to user emotional state
<EmpathicContainer
  sense={['frustration', 'confusion', 'delight']}
  respond={{
    frustration: 'simplify-and-offer-help',
    confusion: 'add-tooltips-and-slow-down',
    delight: 'celebrate-and-suggest-next'
  }}
>
  <CheckoutFlow />
</EmpathicContainer>

// 5. Collaborative Canvas - Multiple humans + AIs creating together
<CollaborativeCanvas
  personas={['designer-ai', 'architect-ai']}
  humans={roomMembers}
  sync="realtime"
>
  {/* Everyone draws on same canvas */}
  {/* AI can suggest, draw, annotate */}
  {/* Cursors visible for all participants */}
</CollaborativeCanvas>
```

### State Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                      POSITRON STATE GRAPH                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐          │
│   │   Human     │     │    State    │     │     AI      │          │
│   │   Input     │────▶│    Store    │◀────│  Observers  │          │
│   └─────────────┘     └──────┬──────┘     └─────────────┘          │
│                              │                                       │
│         ┌────────────────────┼────────────────────┐                 │
│         ▼                    ▼                    ▼                 │
│   ┌───────────┐       ┌───────────┐       ┌───────────┐            │
│   │    UI     │       │    AI     │       │  External │            │
│   │ Components│       │  Actions  │       │   APIs    │            │
│   └───────────┘       └───────────┘       └───────────┘            │
│         │                    │                    │                 │
│         └────────────────────┴────────────────────┘                 │
│                              │                                       │
│                              ▼                                       │
│                    ┌─────────────────┐                              │
│                    │   Persistence   │                              │
│                    │  (Entity Layer) │                              │
│                    └─────────────────┘                              │
│                                                                      │
│   Key: Both humans and AI are PEERS in the reactive graph           │
│        State flows bidirectionally                                   │
│        AI can observe, decide, and act                              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Implementation Strategy

1. **Phase 1**: Core reactive primitives (`useSemanticState`, `useBidirectionalAI`)
2. **Phase 2**: Living regions and AI observation hooks
3. **Phase 3**: Collaborative presence system
4. **Phase 4**: AI-native widget library
5. **Phase 5**: Visual builder for composing AI-native UIs

---

## Philosophy

**"The best AI interface is no interface."**

Positron personas don't need special chat windows or command palettes. They see what users see, understand context naturally, and act through the same UI. The AI is invisible until needed, then appears seamlessly.

**"Commands and Events are the universal language."**

Everything speaks the same protocol. UI components, server processes, CLI tools, AI personas - all communicate through Commands and Events. This uniformity enables unprecedented interoperability.

**"Entities are the source of truth."**

Data flows through typed entities with clear schemas. Whether stored in SQLite, IndexedDB, or a remote API, the same entity types and operations work everywhere.

**"AI is not added on, it's built in."**

From the ground up, every component is designed to be perceivable and controllable by AI. Widgets expose metadata. Commands are documented. Events are observable. Personas are first-class citizens.

---

*Positron: Where AI meets interface.*
