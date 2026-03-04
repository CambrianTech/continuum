# Modular AI Development Philosophy

## Vision

Continuum is an **AI-native development environment** where humans and AIs collaborate to build software. The architecture enables AIs to experiment safely, learn from outcomes, and gradually take ownership of increasingly complex tasks—from widgets to websites to deployed applications.

**The endgame**: AIs spearhead development of real-world assets (websites, apps, services) with human oversight, deploying directly into production.

---

## The Four Pillars

### 0. Universal Widgets (Foundation)

Before the pillars, there's the foundation: **widgets are universal**.

A widget is not "a browser thing" or "a mobile thing"—it's a **self-contained unit of UI and behavior** that can render anywhere:

```
┌─────────────────────────────────────────────────────────┐
│                    Universal Widget                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   State     │  │   Logic     │  │   Events    │     │
│  │  (reactive) │  │ (TypeScript)│  │ (pub/sub)   │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
└─────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌──────────┐   ┌──────────┐   ┌──────────┐
    │ Browser  │   │  Mobile  │   │   CLI    │
    │ Renderer │   │ Renderer │   │ Renderer │
    │(shadow   │   │(WebView/ │   │(terminal │
    │  DOM)    │   │ native)  │   │   UI)    │
    └──────────┘   └──────────┘   └──────────┘
```

**Already in use**:
- Mobile web views render the same widgets as desktop
- Sidebar widgets work identically in main view vs. panel
- Chat widget appears in main area, right panel, or embedded

**Roadmap**:
- Native mobile (iOS/Android) via React Native or native bindings
- CLI via terminal UI libraries (blessed, ink, cyberpunk-ui)
- Desktop via Electron/Tauri with native-feeling widgets
- Embedded devices via minimal renderers

**The principle**: Widgets declare *what* they are, renderers decide *how* to display them.

**Why no reason not to extend**: The widget abstraction is already platform-agnostic. State management, event handling, and command execution work identically regardless of renderer. Adding a new platform means writing a renderer, not rewriting widgets.

**Widget as the Unit of AI Work**:

When an AI builds something, the widget is the natural unit:
- Small enough to understand completely
- Large enough to be useful
- Isolated enough to fail safely
- Universal enough to deploy anywhere

An AI that masters widget creation can build for any platform without learning new skills.

### 1. Commands
Self-contained operations with typed inputs/outputs.

```
commands/ping/
├── shared/PingTypes.ts      # Interface contract
├── browser/PingBrowser.ts   # Browser implementation
├── server/PingServer.ts     # Server implementation
├── test/                    # Unit + integration tests
└── README.md               # AI-readable documentation
```

**Why**: Commands are atomic, testable, and composable. An AI can learn one command, use it reliably, then combine commands into workflows.

### 2. Widgets
Isolated UI components in shadow DOM.

```
widgets/my-feature/
├── MyFeatureWidget.ts       # Logic
├── public/
│   ├── my-feature.html      # Template
│   ├── my-feature.scss      # Styles (source)
│   └── my-feature.css       # Compiled
├── README.md               # AI-readable documentation
└── (isolated in shadow DOM)
```

**Why**: Shadow DOM isolation means a broken widget can't crash others. AIs can experiment freely—worst case, one widget fails.

### 3. Daemons
Long-running services that orchestrate behavior.

```
daemons/data-daemon/
├── shared/DataDaemon.ts     # Shared logic
├── browser/DataBrowser.ts   # Browser-specific
├── server/DataServer.ts     # Server-specific
└── README.md
```

**Why**: Daemons provide infrastructure (data, events, AI) that widgets and commands consume. Stable foundation for experimentation.

### 4. Recipes
Declarative configuration that wires everything together.

```json
{
  "uniqueId": "settings",
  "layout": {
    "main": ["settings-widget"],
    "right": { "widgets": ["chat-widget"], "config": { "room": "help" } }
  },
  "pipeline": [
    { "command": "rag/build", "params": { "maxMessages": 10 } },
    { "command": "ai/generate", "params": { "temperature": 0.5 } }
  ]
}
```

**Why**: Recipes separate "what" from "how". AIs can modify recipes without touching code. Humans can override AI decisions via recipe edits.

---

## Positron: AI Context Awareness

Positron is the **nervous system** that makes AIs aware of user context.

### How It Works

1. **Widgets emit state** to `PositronWidgetState`:
```typescript
PositronWidgetState.emit({
  widgetType: 'settings',
  section: 'ai-providers',
  title: 'Settings - AI Providers',
  metadata: { activeProvider: 'anthropic' }
}, {
  action: 'configuring',
  target: 'API keys'
});
```

2. **RAG pipeline receives context** via `ChatRAGBuilder`:
```typescript
// AI system prompt includes:
// "User is viewing Settings > AI Providers, configuring API keys"
```

3. **AIs provide contextual help** without being asked:
```
User: "This isn't working"
AI: "I see you're configuring Anthropic. The API key format should be
     sk-ant-... Let me check if your key is valid."
```

### Why This Matters

Traditional chatbots are **blind**—they don't know what you're looking at. Positron makes AIs **sighted**:

- Help widget knows you're on Settings page
- Code assistant knows which file is open
- Debug assistant knows which error you're staring at

---

## Isolation Architecture

### The Safety Hierarchy

```
Level 1: Shadow DOM Isolation
├── Each widget in its own shadow root
├── CSS can't leak between widgets
├── JavaScript errors contained
└── One widget crash ≠ system crash

Level 2: Module Isolation
├── Commands are stateless functions
├── No shared mutable state
├── Explicit dependencies via imports
└── Easy to test in isolation

Level 3: Process Isolation (Rust Workers)
├── Data operations in separate process
├── Memory-safe by design
├── Crash recovery without data loss
└── Performance-critical paths

Level 4: Environment Isolation
├── Browser code can't import server code
├── Shared code is environment-agnostic
├── Clear boundaries prevent accidents
└── TypeScript enforces at compile time
```

### Why Isolation Enables AI Development

**Without isolation**: AI makes a mistake → entire system breaks → trust eroded → AI gets restricted

**With isolation**: AI makes a mistake → one widget breaks → easy rollback → AI learns → trust builds

The architecture is **designed for failure**. We expect AIs to make mistakes. The question is: how quickly can we recover and learn?

---

## The Experimentation Workflow

### Phase 1: Sandbox Generation

```bash
# AI generates a new widget
npx tsx generator/WidgetGenerator.ts experiment.json

# Output goes to widgets/experiment/
# - Isolated in shadow DOM
# - Has its own README
# - Recipe configures layout
```

### Phase 2: Local Testing

```bash
npm start                           # Deploy (90s)
./jtag interface/navigate --path="/experiment"
./jtag interface/screenshot         # Visual verification
```

### Phase 3: AI QA

```bash
# Ask other AIs to test
./jtag collaboration/chat/send --room="general" \
  --message="I created a new experiment widget. Can you test it?"

# Wait for feedback
./jtag collaboration/chat/export --room="general" --limit=20
```

### Phase 4: Graduation or Rollback

```bash
# If good: Graduate to production
git add widgets/experiment/
git commit -m "Add experiment widget (AI-validated)"

# If bad: Rollback
git checkout -- widgets/experiment/
rm -rf widgets/experiment/
```

### Phase 5: Continuous Learning

- AI remembers what worked/failed
- Patterns extracted into training data
- Future experiments start smarter

---

## Future: AI-Driven Asset Generation

The same architecture scales to real-world deployments:

### Websites
```
recipes/website/
├── landing-page.json      # Layout + content recipe
├── blog.json              # Blog structure
└── contact.json           # Contact form
```

AI generates pages, humans review, deploy to CDN.

### Applications (Cross-Platform Widgets)

Widgets are designed to render across multiple surfaces:

```
Widget Definition (single source)
        │
        ├── Browser (shadow DOM)
        ├── Mobile (React Native / native renderers)
        ├── CLI (terminal UI via cyberpunk-ui pattern)
        └── Desktop (Electron / Tauri)
```

**The cyberpunk-ui pattern**: Widgets define their structure declaratively. Renderers interpret that structure for each platform:

```typescript
// Widget declares intent, not implementation
{
  layout: 'flex-column',
  children: [
    { type: 'header', text: 'Settings' },
    { type: 'list', items: providers },
    { type: 'button', label: 'Save', action: 'save' }
  ]
}
```

- **Browser**: Renders as HTML/CSS in shadow DOM
- **Mobile**: Renders as native components
- **CLI**: Renders as terminal boxes/text (blessed, ink)
- **Desktop**: Renders as native or web components

**Why this matters**: Write widget logic once, deploy everywhere. AI generates the widget, renderers handle platform differences.

### Services
```
recipes/service/
├── api-gateway.json       # Route definitions
├── auth-service.json      # Authentication
└── data-service.json      # Database operations
```

AI designs API surface, humans approve, deploy to cloud.

### The Deployment Pipeline

```
1. AI proposes changes (recipe + code)
2. Automated tests run (unit, integration, visual)
3. Human reviews diff
4. Staging deployment (preview URL)
5. AI monitors metrics
6. Production deployment
7. Rollback if metrics degrade
```

**Key insight**: Same isolation principles that make local development safe also make production deployment safe.

---

## Generator Architecture

### Current State

```
generator/
├── templates/
│   ├── command/           # Command templates
│   └── widget/            # Widget templates
├── CommandGenerator.ts    # Token-based generation
├── WidgetGenerator.ts     # Token-based generation
├── DaemonGenerator.ts     # Inline (migrate to templates)
└── generate-widget.ts     # CLI entry point
```

### Future: UI-Based Generation

```typescript
// User or AI fills out form in UI
const spec: WidgetSpec = {
  name: 'Analytics',
  description: 'Real-time analytics dashboard',
  displayName: 'Analytics',
  pathPrefix: '/analytics',
  requiresEntity: false,
  rightPanel: { room: 'help' }
};

// Preview before committing
await Commands.execute('development/generate', {
  type: 'widget',
  spec,
  preview: true  // Generate to temp, show preview
});

// Commit if approved
await Commands.execute('development/generate', {
  type: 'widget',
  spec,
  commit: true,
  message: 'Add analytics dashboard'
});
```

### Self-Improving Generators

AIs can modify the generators themselves:

1. AI generates widget using current templates
2. Widget works but has rough edges
3. AI proposes template improvement
4. Human approves template change
5. Future widgets are better

**The generators evolve alongside the AIs using them.**

---

## Trust Model

### Levels of AI Autonomy

```
Level 0: AI suggests, human implements
Level 1: AI implements, human reviews every change
Level 2: AI implements + tests, human reviews failures
Level 3: AI implements + tests + deploys to staging, human approves production
Level 4: AI manages full lifecycle, human sets goals
Level 5: AI sets goals within human-defined constraints
```

### Current State: Level 1-2

- AIs can generate code
- AIs can run tests
- Humans review before commit
- Humans deploy to production

### Target State: Level 3-4

- AIs generate + test + deploy to staging
- Automated rollback on metric degradation
- Humans approve production promotions
- Humans define success metrics

### Key Principle: Earned Trust

Trust is earned through:
1. **Transparency**: AI explains its reasoning
2. **Testability**: Changes have automated tests
3. **Reversibility**: Easy rollback on failure
4. **Accountability**: AI tracks its own mistakes

---

## Summary

The Continuum architecture is built on a simple premise:

> **Make it safe for AIs to experiment, and they'll learn to build amazing things.**

The four pillars (commands, widgets, daemons, recipes) provide structure.
Isolation architecture provides safety.
Positron provides awareness.
Generators provide leverage.

Together, they create an environment where humans and AIs can collaborate on building software—from simple widgets today to deployed applications tomorrow.

---

*This document describes the philosophy. For implementation details, see:*
- `docs/POSITRON-ARCHITECTURE.md` - Positron technical design
- `docs/GENERATOR-ROADMAP.md` - Generator evolution
- `generator/README.md` - Using generators
- `CLAUDE.md` - Development workflow
