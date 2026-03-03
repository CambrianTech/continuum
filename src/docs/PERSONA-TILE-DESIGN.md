# Persona Tile Design — Living Citizen Indicators

> Every visual element on a persona tile must represent real data. No fakes, no demos, no `Math.random()`.

---

## Time-Scale Visual Hierarchy

The tile encodes four temporal layers — each visual element maps to a different time horizon:

| Visual | Time Scale | What It Shows | Update Rate |
|--------|-----------|---------------|-------------|
| **Ring** | This second | Active cognitive phase (evaluating, generating, posting) | Event-driven (~ms) |
| **Diamonds** | This moment | Subsystem activity across four domains | Event-driven, decays after ~5s idle |
| **Meters** | This session | Energy (ephemeral) and fitness (accumulated) | Polled or event-driven (~1-10s) |
| **Genome Bars** | This lifetime | Real LoRA adapter layers from AdapterStore | On connect + after training events |

```
┌──────────────────────────────────────┐
│                                      │
│     ╭──── RING (this second) ───╮    │
│     │                           │    │
│     │    ┌───────────────┐      │    │
│     │    │   AVATAR      │      │    │
│     │    │               │      │    │
│     │    └───────────────┘      │    │
│     │                           │    │
│     ╰───────────────────────────╯    │
│                                      │
│  ◇ ◆ ◇ ◆   DIAMONDS (this moment)   │
│                                      │
│  ▰▰▰▱▱  ▰▰▰▰▱  METERS (session)    │
│                                      │
│  ████░░  GENOME BARS (lifetime)      │
│                                      │
└──────────────────────────────────────┘
```

---

## Ring — Cognitive Phase (Existing, Real)

The ring is already event-driven. It subscribes to `AI_DECISION_EVENTS` and transitions through phases:

| Phase | Visual | Source Event |
|-------|--------|-------------|
| Evaluating | Slow pulse, amber | `ai:decision:evaluating` |
| Responding | Fast spin, cyan | `ai:decision:respond` |
| Generating | Rapid spin, bright cyan | `ai:response:generating` |
| Checking | Subtle flash, white | `ai:response:checking-redundancy` |
| Posted | Brief flash, green | `ai:response:posted` |
| Error | Red pulse | `ai:response:error` |
| Idle | Dim static ring | Phase cleared (null) |

**Status: REAL.** Already wired via `setupAIEventSubscriptions()` in UserListWidget. No changes needed for ring behavior.

---

## Diamonds — Dynamic Subsystem Activity

Four diamonds arranged in a diamond pattern, each representing a real-time subsystem activity. When a subsystem is active, its diamond lights up. When idle for ~5 seconds, it dims back down.

### The Four Diamonds

```
        ◆ THINKING (top)
  ◆ TOOLS          ◆ SPEAKING (right)
  (left)
        ◆ LEARNING (bottom)
```

| Diamond | Position | What It Means | Source Events |
|---------|----------|---------------|---------------|
| **THINKING** | Top | AI is evaluating, deciding, generating | `AI_DECISION_EVENTS.*` (all phases) |
| **SPEAKING** | Right | AI is producing voice/audio output | `voice:ai:speech:start`, `voice:ai:speech:end` |
| **LEARNING** | Bottom | AI is training or capturing interactions | `AI_LEARNING_EVENTS.*` |
| **TOOLS** | Left | AI is executing a tool | `TOOL_EVENTS.STARTED`, `TOOL_EVENTS.RESULT` |

### Activity Pulse Behavior

Each diamond has three visual states:

1. **Off** — Dim outline, subsystem idle (no recent events)
2. **Active** — Solid fill with glow, subsystem currently working
3. **Fading** — Brief bright-to-dim transition after activity ends (~2s decay)

### Data Sources

**THINKING** — Already subscribed via `AI_DECISION_EVENTS`:
```typescript
// Existing events — reuse subscriptions from setupAIEventSubscriptions()
Events.subscribe(AI_DECISION_EVENTS.EVALUATING, ...)
Events.subscribe(AI_DECISION_EVENTS.GENERATING, ...)
// Diamond lights on any non-null phase, dims when POSTED/SILENT/ERROR + decay
```

**SPEAKING** — Voice synthesis events from VoiceOrchestrator / AIAudioBridge:
```typescript
// These events exist in the voice pipeline
Events.subscribe('voice:ai:speech:start', (data: { personaId: string }) => ...)
Events.subscribe('voice:ai:speech:end', (data: { personaId: string }) => ...)
```

**LEARNING** — Already subscribed via `AI_LEARNING_EVENTS`:
```typescript
// Existing events — reuse subscriptions from setupLearningEventSubscriptions()
Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, ...)
Events.subscribe(AI_LEARNING_EVENTS.INTERACTION_CAPTURED, ...)
// Diamond lights on TRAINING_STARTED, dims on TRAINING_COMPLETE/ERROR + decay
```

**TOOLS** — Tool execution events from ToolResult system:
```typescript
// TOOL_EVENTS already exist in system/core/shared/ToolResult.ts
Events.subscribe(TOOL_EVENTS.STARTED, (data: { tool: string, userId: string }) => ...)
Events.subscribe(TOOL_EVENTS.RESULT, (data: { tool: string, userId: string }) => ...)
// Filter by personaId — diamond lights on STARTED, dims on RESULT + decay
```

### Current State vs Target

**Current (FAKE):** Four diamonds show static booleans — `hasLearning`, `isCloud`, `hasRAG`, `hasGenome`. These are configuration flags, not activity indicators. They never change during runtime.

**Target (REAL):** Four diamonds pulse with real subsystem activity. A person watching the tile sees which subsystems are firing right now.

---

## Meters — Session Capacity

Two horizontal bar meters showing ephemeral and accumulated fitness:

### Energy Meter (Ephemeral — This Session)

| Metric | Source | Range | Behavior |
|--------|--------|-------|----------|
| Energy | `PersonaState.energy` | 0.0–1.0 | Depletes with activity, recovers with rest |

Energy is already tracked server-side by `PersonaStateManager`. It needs a transport path to browser:

```
PersonaStateManager (server)
    → new event: 'persona:state:update' { personaId, energy, mood, attention }
    → WebSocket bridge
    → UserListWidget subscribes per persona
```

**Color scale:**
- `>= 0.7` — Green (`#00ff88`) — Energized
- `>= 0.4` — Amber (`#ffaa00`) — Working
- `< 0.4` — Red (`#ff6b6b`) — Tired

### Fitness Meter (Persistent — Accumulated)

| Metric | Source | Range | Behavior |
|--------|--------|-------|----------|
| Fitness | Derived from genome layer count + training history | 0.0–1.0 | Grows with successful training, never depletes |

Fitness reflects accumulated capability — how many adapters this persona has successfully trained and integrated. It's derived from `AdapterStore.discoverForPersona(personaId)`:

```typescript
const adapters = AdapterStore.discoverForPersona(personaId).filter(a => a.hasWeights);
const fitness = Math.min(1.0, adapters.length / MAX_EXPECTED_ADAPTERS);
```

Where `MAX_EXPECTED_ADAPTERS` is a tuning constant (e.g., 10 — a persona with 10+ trained adapters is maximally fit).

### Current State vs Target

**Current (HALF FAKE):** IQ bars use a hardcoded `demoLevels` map with static numbers like `'persona-helper-001': 82`. These never change and are arbitrary.

**Target (REAL):** Two meters — energy (dynamic, server-sourced) and fitness (persistent, adapter-derived). Both reflect actual persona state.

---

## Genome Bars — Real Adapter Layers

Horizontal bars showing the persona's actual LoRA adapter stack from `AdapterStore`.

### Data Source

```typescript
// Server-side: AdapterStore is the single source of truth
const adapters = AdapterStore.discoverForPersona(personaId);

// Each adapter = one genome bar
// Bar properties derived from manifest:
interface GenomeBarData {
  name: string;           // adapter manifest name
  domain: string;         // 'conversational', 'code', 'voice', etc.
  hasWeights: boolean;    // true if trained, false if manifest-only
  baseModel: string;      // what it was trained on
}
```

### Visual Encoding

Each bar represents one adapter:

```
GENOME
┌─────────────────────────────┐
│ ████████████████ ts-expert  │  ← trained, active (bright cyan)
│ ██████████░░░░░ logic-v2   │  ← trained, loaded (medium)
│ ░░░░░░░░░░░░░░░ voice-v1   │  ← manifest only, no weights (dim)
└─────────────────────────────┘
```

| State | Visual | Meaning |
|-------|--------|---------|
| Active + trained | Bright fill + glow | Adapter loaded in inference stack |
| Trained | Medium fill | Adapter exists with weights, not currently loaded |
| Manifest only | Dim outline | Adapter declared but not yet trained |

### Dynamic Updates

Genome bars update on:
- Initial load (query `AdapterStore.discoverForPersona`)
- `AI_LEARNING_EVENTS.TRAINING_COMPLETE` — new adapter trained, add/update bar
- `genome:adapter:activated` — adapter loaded into inference stack
- `genome:adapter:deactivated` — adapter unloaded

### Current State vs Target

**Current (COMPLETELY FAKE):** `const activeLayers = 2 + Math.floor(Math.random() * 3);` generates random bars on every render. Total fabrication.

**Target (REAL):** Bars reflect actual adapters discovered by AdapterStore. Count, names, and training status all come from the filesystem-based adapter registry.

---

## Data Transport: Server → Browser

The tile needs server-side data that doesn't currently reach the browser. Three transport mechanisms:

### 1. Events (Already Bridged)

These events already cross the WebSocket bridge:
- `AI_DECISION_EVENTS.*` — Ring + THINKING diamond
- `AI_LEARNING_EVENTS.*` — LEARNING diamond
- `TOOL_EVENTS.*` — TOOLS diamond

### 2. New Event: `persona:state:snapshot`

PersonaStateManager needs to emit periodic state snapshots:

```typescript
// Server-side: PersonaStateManager emits after each activity/rest
Events.emit('persona:state:snapshot', {
  personaId: this.personaId,
  energy: this.state.energy,
  attention: this.state.attention,
  mood: this.state.mood,
  inboxLoad: this.state.inboxLoad,
  computeBudget: this.state.computeBudget,
  timestamp: Date.now()
});
```

Browser subscribes per-persona to drive the energy meter.

### 3. New Command: `genome/layers`

Query a persona's adapter stack for genome bar rendering:

```typescript
// Returns adapter summary for a persona
const layers = await Commands.execute('genome/layers', {
  personaId: 'persona-helper-001'
});
// Result: { layers: [{ name, domain, hasWeights, isActive, baseModel }] }
```

Called once on tile mount, then updated reactively via learning events.

### 4. Voice Events (May Need Bridging)

`voice:ai:speech:start/end` may only fire server-side. If so, they need WebSocket bridging to reach the browser for the SPEAKING diamond.

---

## Implementation Path

### Phase 1: Kill the Fakes

Replace all fabricated data with real sources or honest "no data" states:

1. **Genome bars**: Query `AdapterStore.discoverForPersona` via new `genome/layers` command. Show actual count. Zero adapters = empty section (not fake bars).
2. **IQ bars**: Remove entirely. Replace with energy + fitness meters. Energy starts at 1.0 (honest default). Fitness derived from adapter count.
3. **Diamond booleans**: Replace static `hasLearning`/`isCloud`/`hasRAG`/`hasGenome` with dynamic activity state driven by event subscriptions.

### Phase 2: Wire Dynamic Diamonds

1. Add `TOOL_EVENTS` subscription (TOOLS diamond)
2. Add voice event subscription (SPEAKING diamond)
3. Reuse existing `AI_DECISION_EVENTS` (THINKING diamond — already subscribed)
4. Reuse existing `AI_LEARNING_EVENTS` (LEARNING diamond — already subscribed)
5. Add decay timers (dim after ~5s of no activity)

### Phase 3: Wire Meters

1. Add `persona:state:snapshot` event emission in PersonaStateManager
2. Bridge event through WebSocket
3. Subscribe in UserListWidget, update energy meter reactively
4. Implement fitness meter from adapter count

### Phase 4: Lit Reactive Modernization

The current tile uses `unsafeHTML()` string templates and direct DOM manipulation (because EntityScroller caches rendered elements). This works but is fragile.

Target: Extract persona tile into its own Lit component (`<persona-tile>`) with proper reactive properties:

```typescript
@customElement('persona-tile')
class PersonaTile extends LitElement {
  @property() userId: string;
  @property() displayName: string;

  // Reactive state driven by event subscriptions
  @state() private _thinkingActive = false;
  @state() private _speakingActive = false;
  @state() private _learningActive = false;
  @state() private _toolsActive = false;
  @state() private _energy = 1.0;
  @state() private _fitness = 0;
  @state() private _genomeLayers: GenomeBarData[] = [];
  @state() private _cognitivePhase: string | null = null;

  // Lit handles re-rendering when @state() changes
  // No more direct DOM manipulation
}
```

This component would subscribe to events in `connectedCallback()`, update `@state()` properties, and let Lit's reactive rendering handle the visuals. EntityScroller would host `<persona-tile>` elements whose internal state updates independently of the scroller's cache.

---

## Design Principles

1. **No fakes.** Every visual element backed by real data. If data isn't available yet, show nothing — not a placeholder.
2. **Time-scale separation.** Ring = instant, diamonds = recent, meters = session, genome = lifetime. A glance tells you what's happening NOW and what's accumulated OVER TIME.
3. **Event-driven, not polled.** Diamonds and ring react to events. Meters may poll at low frequency (energy snapshot every few seconds). Genome loads once, updates on training events.
4. **Graceful absence.** A persona with no adapters shows no genome bars. A persona with no tools shows no TOOLS diamond activity. The tile works with partial data.
5. **Lit-native.** When modernized, each tile is a self-contained Lit component with reactive state. No `unsafeHTML()`, no manual DOM patching.
