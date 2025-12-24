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
