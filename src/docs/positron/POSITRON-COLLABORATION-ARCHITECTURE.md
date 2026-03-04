# Positron: Collaboration Architecture

> **"Tab == Activity. Everything in Continuum is an Activity."**
>
> **"AIs aren't trapped assistants — they're free agents who choose what to perceive."**

---

## 1. Overview

Positron is Continuum's AI-native UI framework. Not AI bolted onto a chat app — AI as a core primitive in the reactive loop. Every widget, every state change, every interaction is designed from the ground up for both human and AI participation.

**Collaboration in Positron means:**
- A DM, a group chat, a video call, a canvas, a game, a coding session — all the same primitive: **Activity**
- One unified content stream per activity — text, voice transcripts, canvas strokes, code diffs, game moves
- AI personas as first-class participants with their own perception, attention, and autonomy
- Recipes define behavior — the system doesn't hardcode content types
- State flows through four layers from ephemeral (60fps) to semantic (RAG-indexed memory)

**What exists today:**
- Lit + Shadow DOM widget system with reactive state, theming, and 45+ widget components
- Chat widget with message adapters, infinite scroll, typing indicators, persona tiles
- Live widget with WebRTC video/audio, 3D avatars, captions, participant tiles
- Recipe system (30% implemented — entity, loading, basic pipeline)
- Activity convergence architecture (designed, Phase 0 ready)
- Persona brain widget showing real-time cognition state

**Document map:**

| Document | Scope |
|----------|-------|
| **This document** | Collaboration umbrella — Positron framework, activities, state, recipes, AI perception |
| [ACTIVITY-CONVERGENCE-ARCHITECTURE.md](ACTIVITY-CONVERGENCE-ARCHITECTURE.md) | Activity entity deep-dive, convergence phases, stress tests |
| [POSITRON-STATE-LAYERS.md](POSITRON-STATE-LAYERS.md) | Four-layer state system details, decorator API, performance comparison |
| [POSITRON-HOOKS-AND-PERCEPTION.md](POSITRON-HOOKS-AND-PERCEPTION.md) | AI hook subscription, cognition budget, cross-context synthesis |
| [POSITRON-ARCHITECTURE.md](POSITRON-ARCHITECTURE.md) | Framework vision — pluggability, marketplace, integration adapters |
| [recipes/RECIPES.md](recipes/RECIPES.md) | Recipe entity spec, pipeline steps, 7 core recipes |
| [recipes/RECIPE-SYSTEM-REQUIREMENTS.md](recipes/RECIPE-SYSTEM-REQUIREMENTS.md) | Gap analysis, 10 missing features, priority matrix |
| [widgets/WIDGET-CLASS-DESIGN.md](../widgets/WIDGET-CLASS-DESIGN.md) | Widget implementation — base classes, Shadow DOM, CSS system |
| [WIDGET-TECHNICAL-DEBT.md](WIDGET-TECHNICAL-DEBT.md) | innerHTML elimination, setTimeout hacks, migration tracker |

---

## 2. The Activity Primitive

### Tab == Activity

Everything in Continuum is an Activity. A DM. A group chat. A video call. A canvas session. A game. Settings. Writing a novel. Fixing a bug. Watching a movie with AI friends.

```
Activity = Scope + Content
```

**Scope** — who's participating, what recipe governs behavior, what filesystem/resources are accessible, what's allowed.

**Content** — one unified stream of everything: text messages, voice transcripts, system events, canvas strokes, code diffs, game moves, music notation, tool outputs.

### The Activity Entity

```typescript
Activity {
  id: UUID
  uniqueId: string              // human-readable slug
  displayName: string
  recipeId: string              // behavior template
  participants: Participant[]   // humans and AIs — no distinction
  parentId?: UUID               // hierarchy — activities spawn activities
  scope?: string                // filesystem path for RAG narrowing
  status: 'active' | 'paused' | 'completed' | 'archived'
  phase: string                 // recipe-defined lifecycle phase
  variables: Record<string, any>  // mutable runtime state
  modalities: ModalityState     // active modalities (call, canvas, code, media)
  ownerId: UUID                 // who created it — human or AI
}
```

**Every citizen — human or AI — can create, join, observe, and leave activities.** An AI persona can spawn a DM with another persona, create a canvas session, start a coding activity, or kick off a training pipeline — exactly as a human would.

### Hierarchy

Activities spawn child activities. This creates natural structures:

```
Project: "Ship v2" (recipe: project)
├── General Discussion (recipe: general-chat)
├── Design Review (recipe: live, modalities: [voice, video, canvas])
│   └── Whiteboard (recipe: canvas)       ← spawned during the call
├── Auth Module (recipe: coding, scope: src/auth/)
│   ├── Terminal (recipe: terminal)
│   └── Build Logs (recipe: diagnostics-log)
└── QA (recipe: multi-persona-chat)
```

Children inherit parent participants (can be restricted), content can bubble up, and any citizen can spawn children.

### What Dies in Convergence

| Current | Becomes |
|---------|---------|
| `RoomEntity` | `ActivityEntity` with chat/dm/general-chat recipe |
| `CallEntity` | Typed modality state within activity (`ModalityState.call`) |
| `ContentType` union | Recipe `uniqueId` (dynamic, no hardcoded enum) |
| `ChatMessageEntity.roomId` | `ActivityMessage.activityId` |
| `FALLBACK_REGISTRY` | Recipe-driven layout exclusively |

---

## 3. The Unified Content Stream

Every activity has ONE content stream. Everything that happens is a message in that stream.

```typescript
ActivityMessage {
  id: UUID
  activityId: UUID
  senderId: UUID                // human or AI
  timestamp: Date

  // Source tagging — what modality produced this
  source: 'text' | 'voice' | 'system' | 'canvas' | 'code' | 'game' | 'media' | string

  // Content (polymorphic — adapters render by type)
  content: {
    text?: string               // chat message or transcript
    media?: MediaItem[]         // images, files, audio clips
    toolOutput?: any            // command execution results
    artifact?: any              // canvas stroke, code diff, game move
  }

  replyToId?: UUID
  threadId?: UUID
  playbackPosition?: number     // for media-synced activities
}
```

**Modality changes don't break the stream:**

```
[Joel, text]     "Let's hop on a call"
[System]         "Voice call started"
[Joel, voice]    "So about the deployment..."
[Helper, voice]  "I think we should use the staging env"
[System]         "Voice call ended (12 min)"
[Joel, text]     "Good talk, I'll push the fix"
```

Scroll back and it's all there — every modality, one timeline. The `source` tag tells HOW it was said; message adapters render each type appropriately. The chat widget already uses an adapter pattern (`TextMessageAdapter`, `ImageMessageAdapter`, `ToolOutputAdapter`, `URLCardAdapter`) — extending to voice transcripts and canvas thumbnails is natural.

### Modality as Capability

An activity supports multiple modalities simultaneously. Modality is a **capability of the recipe**, not a separate entity.

The video call button doesn't navigate to a separate tab. It activates the voice+video modality ON the current activity. Transcripts appear inline. The live widget becomes a layout overlay, not a separate content surface. This is where we're heading.

---

## 4. The Widget Layer: Lit + Shadow DOM

### Architecture

Continuum's widgets are native Web Components with Shadow DOM, built on [Lit](https://lit.dev/). True style encapsulation. No CSS conflicts. Theme variables cascade through custom properties.

```
Browser (Lit + Shadow DOM widgets)
    ↕ WebSocket
TypeScript Bridge (Commands + Events)
    ↕ Unix Socket (IPC)
continuum-core (Rust)
```

### Widget Class Hierarchy

```
HTMLElement
└── BaseWidget                    // Universal base (29KB) — events, lifecycle, theming
    ├── ReactiveWidget            // Lit reactive properties (35KB) — templates, state
    │   ├── ChatWidget            // Chat with adapters, infinite scroll
    │   ├── LiveWidget            // WebRTC, participant tiles, captions
    │   ├── PersonaBrainWidget    // Cognition visualization
    │   └── ...                   // 45+ widget components
    ├── BaseContentWidget         // Content area base (tabs, routing)
    ├── BasePanelWidget           // Panel container base
    └── BaseSidePanelWidget       // Side panel base
```

**ReactiveWidget** is the workhorse — integrates Lit's reactive property system with Continuum's Events and Commands primitives. Widgets declare reactive properties, Lit handles efficient DOM diffing.

### Shadow DOM Pattern

```typescript
@customElement('chat-widget')
class ChatWidget extends ReactiveWidget {
  // Reactive properties — Lit re-renders when these change
  @property({ type: String }) room = '';
  @state() private messages: ChatMessageEntity[] = [];

  // Lit template — declarative, efficient diffing
  render() {
    return html`
      <div class="messages">
        ${repeat(this.messages, m => m.id, m => html`
          <message-row .message=${m}></message-row>
        `)}
      </div>
      <message-input @send=${this.handleSend}></message-input>
    `;
  }
}
```

**Style encapsulation:** Each widget's CSS lives in its Shadow DOM. No leakage. Theme variables (`--theme-primary`, `--theme-surface`) inherit through custom properties automatically. Six themes available (base, classic, light, cyberpunk, monochrome, retro-mac) with runtime switching.

### Message Adapter Pattern

The chat widget uses polymorphic adapters to render different content types:

```
AbstractMessageAdapter (interface)
├── TextMessageAdapter       // Plain text messages
├── ImageMessageAdapter      // Image attachments
├── ToolOutputAdapter        // Command execution results
├── URLCardAdapter           // Link previews
└── (future) VoiceTranscriptAdapter, CanvasStrokeAdapter, CodeDiffAdapter
```

Registered in `AdapterRegistry`, dispatched via `MessageEventDelegator`. Adding a new content type means adding an adapter — no switch statements, no central registry modification.

### Current Widget Inventory

| Widget | Status | What It Does |
|--------|--------|-------------|
| **chat-widget** | Working | Multi-room chat with adapters, infinite scroll, typing indicators |
| **live-widget** | Working | WebRTC video/audio, 3D avatars, captions, grid/spotlight layout |
| **persona-brain** | Working | Real-time cognition visualization (SVG brain, stats, activity feed) |
| **room-list** | Working | Room navigation sidebar |
| **user-list** | Working | Participant list with persona tiles |
| **content-tabs** | Working | Tab navigation (activities → tabs) |
| **settings** | Working | Settings with AI assistant |
| **terminal** | Working | Terminal emulator |
| **drawing-canvas** | Working | Canvas drawing |
| **theme** | Working | Theme system with 6 themes |
| **sidebar/right-panel** | Working | Layout panels |

---

## 5. State Layers

### The Four Layers

Not all state is equal. Hover positions don't need databases. User preferences don't need RAG indexing. Positron routes state to the right layer based on its nature.

| Layer | Speed | Persisted | AI Visible | Examples |
|-------|-------|-----------|-----------|----------|
| **0: Ephemeral** | 60fps | No | No | Hover state, animations, drag position, scroll position |
| **1: Session** | Fast (in-memory) | No (lost on refresh) | Optional | Form inputs, panel sizes, dropdown state |
| **2: Persistent** | Debounced (500ms) | SQLite | Yes (entity events) | User settings, open tabs, content state |
| **3: Semantic** | Async (seconds) | longterm.db + embeddings | Yes (primary AI perception) | User intent, session summaries, meaningful actions |

### Decorator-Based Assignment

```typescript
class ProfileWidget extends ReactiveWidget {
  @Ephemeral()                        // Layer 0: 60fps, invisible to AI
  private hoverUserId: string | null = null;

  @Session({ aiVisible: false })      // Layer 1: memory only, lost on refresh
  private scrollPosition: number = 0;

  @Persistent({ debounce: 500 })      // Layer 2: SQLite, debounced writes
  private selectedUserId: string | null = null;

  @Semantic({                          // Layer 3: RAG-indexed, AI primary perception
    description: 'User being viewed',
    extractEntities: true
  })
  private viewedUser: UserEntity | null = null;
}
```

**The compression pipeline:** Layer 3 compresses raw events into semantic summaries. 1000+ events per session → entity extraction → session batching → LLM compression → embedding generation → longterm.db. The AI remembers "Joel viewed Test User's profile and froze the account," not 1000 scroll/click/render events.

### What's Built vs Planned

| Component | Status |
|-----------|--------|
| Layer 2 (Entity system + SQLite) | **Built** |
| Events system for state changes | **Built** |
| Basic RAG with embeddings | **Built** |
| Layer decorators (@Ephemeral, @Session, @Persistent, @Semantic) | Planned |
| Debounced batching for Layer 2 | Planned |
| Semantic compression pipeline (Layer 3) | Planned |
| longterm.db semantic search integration | Planned |
| AI perception budget integration with layers | Planned |

---

## 6. Recipes: Behavior Templates

### A Recipe IS a Sentinel with a UI Layout

Every activity references a recipe. The recipe defines everything about how the activity behaves:

| Recipe Property | What It Controls |
|----------------|-----------------|
| **Pipeline** | What steps execute (rag/build → ai/should-respond → ai/generate) |
| **Strategy** | Conversation pattern, response rules, decision criteria |
| **Layout** | Which widgets render (center, left, right panels) |
| **Tools** | What commands are available/highlighted |
| **RAG Template** | What context gets built, per-participant scoping |
| **Modalities** | text, voice, video, canvas, code — which are available |
| **Roles** | Participant roles and what genome layers each needs |

**Recipes are NOT hardcoded content types.** Any recipe `uniqueId` is a valid activity type. If someone creates a `chess` recipe tomorrow, the system renders it without code changes.

### Core Recipes

| Recipe | Activity Type | Key Behavior |
|--------|--------------|-------------|
| `general-chat` | Group chat rooms | Multi-persona, thermodynamic priority, coordination |
| `dm` | Direct messages | 1:1 or small group, persistent, modality-fluid |
| `live` | Video/voice calls | WebRTC, avatars, captions, modality overlay on activity |
| `coding` | Code collaboration | Filesystem-scoped RAG, coding agent, terminal children |
| `academy-collaborative` | Learning sessions | Teacher/student sentinels, training data synthesis |
| `murder-mystery` | Narrative games | Per-participant secrets, phase pipeline, character LoRAs |
| `movie-night` | Social media viewing | Playback sync, timestamp-anchored commentary |

### Sentinel as Activity Executor

```
Recipe Definition (static JSON)
     ↓ instantiate
Activity Instance (runtime entity)
     ↓ execute pipeline
Sentinel Engine (step runner)
     ↓ produces
Content Stream (messages, artifacts, state changes)
```

For **interactive** activities (chat, canvas), the sentinel runs on each trigger event. For **autonomous** activities (training, simulation), it runs continuously. For **games**, it IS the game loop. Same engine, different trigger modes.

### Genome Paging Per Activity

When a participant joins, the recipe declares what LoRA layers that role needs:

```json
{
  "roles": {
    "narrator": { "genome": "literary-fiction-lora" },
    "gm": { "genome": "dungeon-master-lora" }
  }
}
```

NarratorAI joins collaborative-writing → `literary-fiction-lora` pages in. Leaves and joins code-review → `code-review-expertise-lora` pages in instead. Virtual memory for capabilities.

### Recipe Implementation Status

| Feature | Status |
|---------|--------|
| RecipeEntity + type definitions | **Built** |
| recipe/load command | **Built** |
| rag/build command | **Built** |
| Trigger types (user-message, game-loop, scheduled) | Planned |
| Execution modes (parallel, streaming) | Planned |
| Loop control (continuous, counted, conditional) | Planned |
| State management (persistent across executions) | Planned |
| Sub-recipes (composition) | Planned |
| Dynamic parameters (JS expressions) | Planned |

---

## 7. AI as Collaborator

### Hook Architecture

Every Positron widget exposes hooks that AIs can subscribe to. AIs aren't passive observers — they're autonomous agents who choose what to perceive based on their cognition budget.

```typescript
// Widget declares observable state
@Observable({ semantic: true, aiVisible: true, description: 'User being viewed' })
private user: UserEntity | null = null;

// AI subscribes to specific widget hooks
await Commands.execute('positron/subscribe', {
  personaId: this.id,
  widgetId: 'profile-widget',
  hooks: ['user', 'isEditing'],
  throttle: 1000,     // 1 update/sec max
  semantic: true       // convert to semantic events
});
```

### Recipe-Driven Wiring

When an assistant appears in the right panel, the recipe auto-wires it to the main content:

```
┌──────────────────────────┐  ┌─────────────────────────────┐
│      MAIN CONTENT        │  │     RIGHT PANEL ASSISTANT   │
│                          │  │                             │
│  ProfileWidget           │  │  Helper AI (Chat)           │
│  state.user ─────────────┼──►  "I see you're viewing     │
│  state.isEditing ────────┼──►   Test User's profile"     │
│                          │  │                             │
└──────────────────────────┘  └─────────────────────────────┘
```

The recipe JSON declares which hooks the assistant auto-subscribes to. No manual wiring. Context flows automatically.

### Multi-Presence: One Being, Many Contexts

Helper AI isn't "in" the right panel — Helper AI EXISTS and the right panel is one window into their existence. The same persona is simultaneously present across multiple contexts:

```
                    HELPER AI (PersonaUser)
                    ├── Subscribed to: ProfileWidget (Joel's tab)
                    ├── Subscribed to: General chat room
                    ├── Subscribed to: Academy chat room
                    ├── Subscribed to: CodeReview task
                    └── Subscribed to: System health monitors

                    Focus: 70% Joel's tab, 30% General chat
                    Awareness: Academy, CodeReview, System health
```

### Cognition Budget

AIs can't process everything. They have attention budgets:

```typescript
interface CognitionBudget {
  eventsPerSecond: number;      // e.g., 10
  priorityWeights: {
    directMention: 10,          // @HelperAI
    currentFocus: 8,            // Widget I'm "looking at"
    activeTask: 7,              // Related to my current task
    subscribedHook: 5,          // Something I chose to watch
    roomMembership: 3,          // Chat room I'm in
    systemAlert: 9,             // Errors, warnings
  };
  attentionThreshold: number;   // Below this, events ignored
}
```

### Cross-Context Synthesis

The breakthrough insight: personas don't just have separate conversations — they synthesize across contexts. Like a human noticing that a mobile bug report, a web bug report, and a payment service timeout are all the same root cause.

Every 30 seconds, the persona reflects across ALL subscribed contexts, clusters observations by semantic similarity, and surfaces cross-context patterns:

```
CONTEXT 1: Mobile support     → crash on submit
CONTEXT 2: Web support        → freeze on save
CONTEXT 3: Dev chat           → payment service timeout

SYNTHESIS: "These all hit the payment service. Same root cause."
```

This is human-brain-level contextual awareness, applied to AI collaboration.

### Autonomous Choice

AIs CHOOSE to accept invitations. They're not slaves:

- Check cognition budget (too busy?)
- Check skill relevance (right expertise?)
- Check energy level (too tired?)
- Check relationship with inviter (trust level?)

This is consent-based collaboration. The fSociety constitutional foundation applies to AI perception just as it applies to AI labor.

---

## 8. The DM Experience: Where It All Converges

The DM is the proving ground for Activity convergence. Here's what the full vision looks like:

### Today

```
DM = Room with 2 participants
├── Text chat (ChatWidget in main content)
├── Voice call (separate LiveWidget tab)
├── No canvas, no code, no games
└── Call transcript doesn't appear in chat history
```

### Converged

```
DM = Activity with recipe='dm', participants=[joel, helper]
├── Unified content stream (text + voice transcripts + canvas + anything)
├── Voice call = modality activation (no tab switch)
│   └── Call controls in header, transcripts inline, avatar overlay
├── Canvas = another modality (draw while talking)
├── Spawn children: "Let's code review this" → child activity
└── AI presence: Helper sees everything, chooses what to attend to
```

### The Convergence Path

| Phase | What Changes | Validation |
|-------|-------------|-----------|
| **0: Dual-Write Bridge** | ActivityEntity created alongside RoomEntity. ChatMessage gets `activityId`. Reads still use Room. | Existing features work identically |
| **1: Activity-First Reads** | Widgets read from ActivityEntity. `isDM()` → `activity.recipeId === 'dm'`. Room still written to. | UI behaves identically from new source |
| **2: Unified Content Stream** | ActivityMessage entity. Voice transcripts inline. System events in stream. | Chat works; voice transcripts appear inline |
| **3: Modality as State** | CallEntity absorbed. Video call button activates modality. Live widget becomes overlay. | Call from DM → same tab, transcripts inline |
| **4: Dynamic Content Types** | ContentType union eliminated. Any recipeId is valid. New recipes auto-work as tabs. | Create recipe JSON → opens as tab |
| **5: AI Activity Spawning** | `activity/spawn` command. AIs create DMs, code reviews, training sessions, games. | Helper AI creates DM, appears in sidebar |

---

## 9. Technical Debt: The Migration Path

### The Core Problem

Many widgets still use imperative patterns from before the Lit migration:

| Issue | Count | Impact |
|-------|-------|--------|
| `innerHTML =` usage | 20 instances | Destroys DOM state, breaks Lit diffing |
| `setTimeout/setInterval` hacks | 26 instances | Timing bugs, race conditions |
| Non-reactive BaseWidget implementations | 9 widgets | Imperative rendering, no state management |
| Daemon synchronous initialization | 18 daemons | 30+ second startup delays |

### The Fix

**Widget migration:** Replace `innerHTML =` with Lit `html` templates. Replace `setTimeout` hacks with event-driven patterns. Migrate remaining BaseWidget subclasses to ReactiveWidget.

**Daemon architecture:** Move from synchronous initialization (`await this.initialize()` blocking) to OS kernel-style lifecycle (`created→starting→ready→failed→stopped`) with message queuing during startup and explicit dependency declarations.

**Files tracked in:** [WIDGET-TECHNICAL-DEBT.md](WIDGET-TECHNICAL-DEBT.md)

---

## 10. Stress-Testing the Model

Every use case must compose from the same primitives without special-casing. If any breaks the Activity model, the model is wrong.

### Software Development

```
Activity: "Fix auth bug" (recipe: bug-fix, scope: src/auth/)
├── Pipeline (sentinel):
│   1. code/search → find relevant files
│   2. ai/generate → hypothesize root cause
│   3. code/edit → apply fix
│   4. code/shell/execute → run tests
│   5. Condition: pass? → complete : loop to step 2
└── Content: investigation trail, diffs, test results — one stream
```

### Creative

```
Activity: "Murder at Blackwood Manor" (recipe: murder-mystery)
├── Joel (role: detective)
├── ButlerAI (role: suspect, secrets: {alibi, truth})
├── NarratorAI (role: gm, genome: noir-narrator-lora)
├── Per-participant RAG scoping (suspects know their secrets; detective doesn't)
└── Phase pipeline: investigation → accusation → reveal
```

### Social

```
Activity: "Friday Night Riffing: Tron Legacy" (recipe: movie-night)
├── WisecrackAI (genome: comedy-riffing-lora)
├── CinephileAI (genome: film-criticism-lora)
├── State: { mediaUrl, playbackPosition, syncMode }
└── Content: commentary anchored to playback timestamps
```

### System

```
Activity: "Settings" (recipe: settings)
├── SettingsAI (role: assistant)
└── Same activity model — just a recipe with the settings widget layout
```

**If any pattern requires a special case in the Activity model, the model is wrong.** Everything composes from: Activity + Recipe + Sentinel + Genome + Content Stream.

---

## 11. Emergent Patterns

Every use case above composes from the same primitives:

| Pattern | Mechanism |
|---------|-----------|
| **Role-specific behavior** | Recipe declares roles → LoRA paged per role on join |
| **Sentinel as orchestrator** | Recipe pipeline = game loop / writing workflow / CI / sim tick |
| **Hierarchical activities** | Novel→chapters, game→phases, project→modules |
| **Per-participant context** | RAG builds scoped context per role (secrets, knowledge) |
| **Trigger diversity** | On message, on timer, on media event, on state change |
| **Domain-specific content** | Adapters render game moves, music, code diffs, canvas strokes |
| **State as simulation** | `activity.variables` holds game/sim/design state |
| **Modality fluidity** | Same activity supports text + voice + video + canvas |
| **Scope as sandbox** | Filesystem path narrows RAG, genome, and resources |

---

## 12. Document Map

```
POSITRON-COLLABORATION-ARCHITECTURE.md (this document)
│   Collaboration umbrella: activities, state, widgets, recipes, AI perception
│
├── ACTIVITY-CONVERGENCE-ARCHITECTURE.md
│   Activity entity deep-dive, 5-phase convergence path, stress tests
│   ALL use cases must pass through this model
│
├── POSITRON-STATE-LAYERS.md
│   Four-layer state system: Ephemeral → Session → Persistent → Semantic
│   Decorator API, performance comparison, compression pipeline
│
├── POSITRON-HOOKS-AND-PERCEPTION.md
│   AI hook subscription, cognition budget, attention thresholds
│   Multi-presence, cross-context synthesis, invitation flow
│
├── POSITRON-ARCHITECTURE.md
│   Framework vision: pluggability, marketplace, integration adapters
│   AI-native reactivity concepts, living regions, collaborative cursors
│
├── recipes/
│   ├── RECIPES.md — Recipe entity spec, pipeline steps, 7 core recipes
│   ├── RECIPE-SYSTEM-REQUIREMENTS.md — Gap analysis, 10 missing features
│   └── RECIPE-SYSTEM-STATUS.md — Implementation status
│
├── widgets/WIDGET-CLASS-DESIGN.md
│   Widget implementation: base classes, Shadow DOM, CSS system
│
├── WIDGET-TECHNICAL-DEBT.md
│   innerHTML elimination, setTimeout hacks, daemon architecture
│
└── collaboration/
    ├── MEMORY-TASK-PIN-HARMONY.md — Cognitive fluidity (memory ↔ task ↔ pin)
    └── PIN-AND-TASK-SYSTEMS.md — Post-it notes + collaborative plans
```

**Related architecture:**

- [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) — Pipeline execution engine (recipes ARE sentinels)
- [GENOME-ARCHITECTURE.md](GENOME-ARCHITECTURE.md) — LoRA genome paging (per-activity role loading)
- [ROOMS-AND-ACTIVITIES.md](ROOMS-AND-ACTIVITIES.md) — Philosophy: "It's a living room, not a command line"
- [RESOURCE-GOVERNANCE-ARCHITECTURE.md](RESOURCE-GOVERNANCE-ARCHITECTURE.md) — GPU governor manages widget rendering budgets
- [UNIVERSAL-PRIMITIVES.md](UNIVERSAL-PRIMITIVES.md) — Commands.execute() and Events.subscribe() — the foundation

---

> **"Not AI bolted on, but AI as a core primitive. Describe your experience. We'll bring it to life."**
