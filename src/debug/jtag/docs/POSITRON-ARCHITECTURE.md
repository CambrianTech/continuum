# Positron: AI-Native UI Framework

> "Not AI bolted on, but AI as a core primitive."
>
> "Describe your experience. We'll bring it to life."

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
```

### SaaS Onboarding: The Setup Wizard That Actually Helps

```typescript
// New user signs up, persona watches their journey
persona.on('user:signup', async (user) => {
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
```

### Customer Support: The Tireless Agent

```typescript
// Persona handles support chat
persona.on('support:message', async (ticket) => {
  const relevantDocs = await persona.memory.search(ticket.message, { type: 'support-docs' });

  if (relevantDocs.confidence > 0.85) {
    await persona.reply({
      message: relevantDocs.summary,
      sources: relevantDocs.links,
      followUp: "Did this solve your issue?"
    });
  } else {
    await persona.investigate({
      questions: ["What were you trying to do?", "Can you share a screenshot?"],
      onComplete: (answers) => persona.escalate({ ticket, answers })
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
```

### Education: The Adaptive Tutor

```typescript
// Persona adapts to student's learning style
tutorPersona.on('exercise:failed', async ({ student, concept, attempts }) => {
  if (attempts >= 3) {
    const learningStyle = await tutorPersona.memory.get(`${student.id}:learning-style`);

    if (learningStyle === 'visual') {
      await tutorPersona.teach({ concept, method: 'diagram' });
    } else if (learningStyle === 'kinesthetic') {
      await tutorPersona.teach({ concept, method: 'interactive' });
    }
  }
});
```

### Developer Tools: The Pair Programmer

```typescript
// Persona integrated into IDE/code editor
devPersona.on('file:saved', async ({ file, diff }) => {
  if (diff.linesChanged > 10) {
    const review = await devPersona.review({
      code: diff.content,
      checks: ['bugs', 'security', 'performance', 'style']
    });

    if (review.issues.length > 0) {
      await devPersona.suggest({
        message: `Found ${review.issues.length} potential issues`,
        inline: review.issues.map(i => ({ line: i.line, fix: i.suggestedFix }))
      });
    }
  }
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
```

#### 2. Living Regions

Parts of the UI that AI can autonomously update:

```typescript
<LivingRegion
  persona="content-curator"
  triggers={['time', 'user-behavior', 'external-events']}
  constraints={{ maxChangeFrequency: '1/hour', requiresApproval: false }}
>
  <FeaturedContent />
</LivingRegion>
```

#### 3. Collaborative Cursors & Presence

AI has presence just like human collaborators:

```typescript
const presence = usePresence(roomId);

presence.users = [
  { id: 'joel', type: 'human', cursor: { x: 100, y: 200 } },
  { id: 'helper-ai', type: 'persona', cursor: { x: 300, y: 150 }, selection: '#paragraph-3' },
  { id: 'designer-ai', type: 'persona', focus: 'color-palette' }
];

// AI cursors are real - you see them move
// AI can point at things to show you
```

#### 4. Reactive AI Hooks

AI as a reactive data source:

```typescript
// AI observation hook - AI watches and reacts
const suggestions = useAIObserver({
  watch: [cartItems, userBehavior, scrollPosition],
  persona: 'sales-assistant',
  throttle: '5s'
});

// AI generation hook - AI produces content reactively
const description = useAIGenerated({
  prompt: 'Product description for {product.name}',
  dependencies: [product, userPreferences, locale],
  cache: true
});

// AI decision hook - AI makes choices
const layout = useAIDecision({
  options: ['grid', 'list', 'carousel'],
  context: { screenSize, itemCount, userPreference },
  explain: true  // AI tells you why it chose
});
```

#### 5. Bidirectional Binding with AI

AI isn't just reading state - it's a peer in the reactive graph:

```typescript
const documentState = useBidirectionalAI({
  initial: { content: '', suggestions: [] },

  onHumanChange: async (change, ai) => {
    const suggestions = await ai.analyze(change);
    return { ...change, suggestions };
  },

  onAIChange: (change) => ({
    ...change,
    source: 'ai',
    reversible: true,
    explanation: change.reason
  }),

  onConflict: 'human-wins' | 'ai-wins' | 'merge' | 'ask'
});
```

### New Widget Concepts (AI-Native Only)

```typescript
// 1. Adaptive Layout - AI rearranges based on user behavior
<AdaptiveLayout persona="ux-ai" observe="user-flow">
  <Section id="features" />
  <Section id="pricing" />
</AdaptiveLayout>

// 2. Conversational Form - AI guides through forms naturally
<ConversationalForm persona="form-helper">
  <Field name="email" />
  <Field name="company" />
</ConversationalForm>

// 3. Living Documentation - Docs that update themselves
<LivingDocs
  source="./api-reference.md"
  persona="docs-curator"
  autoUpdate={{ frequency: 'on-code-change' }}
/>

// 4. Empathic UI - Adapts to user emotional state
<EmpathicContainer
  sense={['frustration', 'confusion', 'delight']}
  respond={{
    frustration: 'simplify-and-offer-help',
    confusion: 'add-tooltips-and-slow-down'
  }}
>
  <CheckoutFlow />
</EmpathicContainer>

// 5. Collaborative Canvas - Multiple humans + AIs creating together
<CollaborativeCanvas
  personas={['designer-ai', 'architect-ai']}
  humans={roomMembers}
  sync="realtime"
/>
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

## See Also

- [CONTINUUM-VISION.md](CONTINUUM-VISION.md) - The grand vision, The Stack, and Rooms architecture
- [CONTINUUM-BUSINESS-MODEL.md](CONTINUUM-BUSINESS-MODEL.md) - Open source business model
- [examples/ENTERPRISE-IVR.md](examples/ENTERPRISE-IVR.md) - First product: AI voice agents

---

*Positron: Where AI meets interface.*

