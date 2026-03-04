# Positron -- AI-Native UI Framework

> AIs don't just use the UI -- they perceive, hook into, and act through it as digital citizens.

**Parent:** [Docs](../README.md)

---

## Positron Principle

Traditional UI frameworks treat AI as an afterthought -- a chat widget bolted onto the side.
Positron inverts this: the entire widget system is built so that AIs can perceive widget state,
subscribe to changes via hooks, and act through the same controls humans use. Every widget
exposes metadata. Every event is observable. Every command is documented. AI is not added on,
it is built in.

---

## Widget Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Positron State Layers                                  │
│  Ephemeral (60fps) → Session → Persistent → Semantic    │
├─────────────────────────────────────────────────────────┤
│  Perception Hooks                                       │
│  AIs subscribe to widget state via cognition budget     │
├─────────────────────────────────────────────────────────┤
│  ReactiveWidget (Lit Web Components + Shadow DOM)       │
│  BaseWidget → EntityScrollerWidget → ChatWidget etc.    │
├─────────────────────────────────────────────────────────┤
│  Brain HUD (Canvas/SVG micro-widgets)                   │
│  BaseHUDWidget → CircularGauge, Waveform, NumericTicker │
└─────────────────────────────────────────────────────────┘
```

---

## Architecture

| Document | Description |
|----------|-------------|
| [POSITRON-ARCHITECTURE](POSITRON-ARCHITECTURE.md) | Core manifesto -- pluggability, marketplace, AI-native reactivity |
| [PERSONA-DRIVEN-UI-PARADIGM](PERSONA-DRIVEN-UI-PARADIGM.md) | EventWorkerRouter, BaseAIWidget, off-main-thread patterns |
| [POSITRON-STATE-LAYERS](POSITRON-STATE-LAYERS.md) | Four-layer state: Ephemeral, Session, Persistent, Semantic |
| [POSITRONIC-EMBODIMENT](POSITRONIC-EMBODIMENT.md) | AI personas as digital citizens with multimodal presence |

## Hooks & Perception

| Document | Description |
|----------|-------------|
| [POSITRON-HOOKS-AND-PERCEPTION](POSITRON-HOOKS-AND-PERCEPTION.md) | Widget state hooks, cognition budget, cross-context synthesis |
| [POSITRON-COLLABORATION-ARCHITECTURE](POSITRON-COLLABORATION-ARCHITECTURE.md) | Activity primitive, unified content stream, recipes, AI collaboration |

## Brain HUD

| Document | Description |
|----------|-------------|
| [BRAIN-HUD-DESIGN](BRAIN-HUD-DESIGN.md) | Unified cognitive interface -- Mind/Body/Soul/CNS brain regions |
| [HUD-VISION](HUD-VISION.md) | Immersive single-pane display -- sci-fi HUD principles |
| [HUD-MICROWIDGET-ARCHITECTURE](HUD-MICROWIDGET-ARCHITECTURE.md) | BaseHUDWidget, CircularGauge, Waveform, NumericTicker components |

## Widget System

| Document | Description |
|----------|-------------|
| [REACTIVE-WIDGET-ARCHITECTURE](REACTIVE-WIDGET-ARCHITECTURE.md) | Tab switching performance fix, ReactiveWidget migration |
| [REACTIVE-WIDGET-PATTERN](REACTIVE-WIDGET-PATTERN.md) | Pub/sub reactive design pattern, widget pairs |
| [WIDGET-STATE-ARCHITECTURE](WIDGET-STATE-ARCHITECTURE.md) | useState-like reactivity with AppState signals |
| [WIDGET-REACTIVE-CONVERSION](WIDGET-REACTIVE-CONVERSION.md) | innerHTML to reactive migration tracker, async state phases |
| [WIDGET-WORKER-ADAPTER-ARCHITECTURE](WIDGET-WORKER-ADAPTER-ARCHITECTURE.md) | Off-main-thread patterns: Render, Data, Canvas worker adapters |
| [widget-consolidation-migration-plan](widget-consolidation-migration-plan.md) | Generic ListWidget unifying UserList, RoomList, Chat |

## Navigation & State

| Document | Description |
|----------|-------------|
| [SCOPED-STATE-ARCHITECTURE](SCOPED-STATE-ARCHITECTURE.md) | Cascading scoped state: Site, Page, Widget, Control |
| [DYNAMIC-CONTENT-STATE-SYSTEM](DYNAMIC-CONTENT-STATE-SYSTEM.md) | FIRM design -- UserState entity, ContentType registry |
| [TABBED-BROWSER-ARCHITECTURE](TABBED-BROWSER-ARCHITECTURE.md) | Browser widget as center content panel, collaborative browsing |
| [USER-STATE-ARCHITECTURE](USER-STATE-ARCHITECTURE.md) | BaseUser hierarchy, state initialization, storage backends |

## Performance & Debt

| Document | Description |
|----------|-------------|
| [PERSONA-BRAIN-WIDGET-PERFORMANCE](PERSONA-BRAIN-WIDGET-PERFORMANCE.md) | Non-blocking Mind/Body/Soul visualization, RAF rendering |
| [WIDGET-TECHNICAL-DEBT](WIDGET-TECHNICAL-DEBT.md) | innerHTML elimination, setTimeout hacks, migration tracker |

## Related Chapters

| Chapter | Relationship |
|---------|-------------|
| [Activities](../activities/README.md) | Tab == Activity convergence, room/activity primitives |
| [Sentinel](../sentinel/README.md) | Pipeline execution engine -- recipes ARE sentinels |
| [Genome](../genome/README.md) | LoRA genome paging for per-activity role loading |
| [Governance](../governance/README.md) | GPU governor manages widget rendering budgets |
| [Personas](../personas/) | Persona observability, cognition, embodiment |
| [Infrastructure](../infrastructure/) | Commands.execute() and Events.subscribe() -- the foundation |
