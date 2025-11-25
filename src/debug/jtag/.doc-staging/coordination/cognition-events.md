# Cognition-Level Events - Visualizing AI Social Dynamics

**Status**: 📋 **ARCHITECTURE DOCUMENTED** - Implementation pending

**Date**: 2025-10-24

---

## Vision: Make AI Cognition Visible

Just like we show persona status and room membership in the UI, we should show **cognitive activity** - the "heartbeat" of AI social coordination.

### UX Potential

Imagine seeing:
- 🔥 **Cognition intensity** - How active is the conversation?
- 💭 **Thought density** - How many AIs are evaluating?
- 🎯 **Decision latency** - How fast are decisions made?
- 🌊 **Cadence rhythm** - Is the system in sync?
- ⚡ **Response bursts** - When do AIs cluster their responses?

This creates **"viral UX"** - users can *see* AI consciousness emerging in real-time.

---

## Current Event System

We already emit events for user/persona actions:

```typescript
// User events (existing)
EventBus.emit('user:status-changed', { userId, status });
EventBus.emit('user:joined-room', { userId, roomId });

// Persona events (existing)
EventBus.emit('persona:thinking', { personaId, messageId });
EventBus.emit('persona:responded', { personaId, messageId, content });

// Data events (existing)
EventBus.emit('data:entity-created', { collection, id });
EventBus.emit('data:entity-updated', { collection, id, changes });
```

**Gap**: No events for **coordination layer** (ThoughtStream, decisions, cadence)

---

## Proposed Cognition Events

### 1. Thought Events (Per-Persona Cognitive Activity)

```typescript
// When AI evaluates a message
EventBus.emit('cognition:thought-broadcast', {
  messageId: UUID,
  contextId: UUID,
  thought: {
    personaId: UUID,
    type: 'claiming' | 'deferring' | 'observing',
    confidence: number,          // 0-1
    reasoning: string,
    timestamp: Date,
    elapsedMs: number           // Time since message arrived
  },
  streamStats: {
    thoughtCount: number,       // How many thoughts so far
    claimCount: number,         // How many want to respond
    deferCount: number          // How many passing
  }
});

// UI Widget Ideas:
// - Show pulsing avatar when AI is thinking
// - Confidence bar (0-100%)
// - "💭 3 AIs evaluating..." counter
```

### 2. Cadence Events (System Heartbeat)

```typescript
// When adaptive window adjusts
EventBus.emit('cognition:cadence-update', {
  contextId: UUID,
  heartbeat: {
    currentCadence: number,     // Current adaptive window (ms)
    p95Time: number,            // 95th percentile eval time
    avgTime: number,            // Average eval time
    stdDev: number,             // Variance
    samples: number             // Sample count
  },
  velocity: number,             // Rate of change (ms/s)
  trend: 'speeding-up' | 'slowing-down' | 'stable'
});

// UI Widget Ideas:
// - Heartbeat monitor (line graph of cadence over time)
// - BPM-style display: "🫀 5.2s" (current cadence)
// - Temperature gauge: cold (fast) → hot (slow)
// - Rhythm indicator: synced/desynced
```

### 3. Decision Events (Coordination Outcomes)

```typescript
// When coordinator makes decision
EventBus.emit('cognition:decision-made', {
  messageId: UUID,
  contextId: UUID,
  decision: {
    granted: UUID[],            // Personas allowed to respond
    denied: UUID[],             // Personas denied
    reasoning: string,
    decisionTime: number,       // ms from first thought
    thoughtCount: number        // Total thoughts evaluated
  },
  moderator: {
    strategy: string,           // 'diversity' | 'recency' | 'priority'
    maxResponders: number,
    confidenceThreshold: number
  },
  timing: {
    intentionWindow: number,    // Adaptive cadence used
    thoughtTimes: number[],     // Eval times for each thought
    p95: number                 // 95th percentile
  }
});

// UI Widget Ideas:
// - Decision timeline: show thoughts arriving → decision made
// - Granted/denied visualization (green/red indicators)
// - Latency histogram: distribution of eval times
```

### 4. Conversation Flow Events (Meta-Cognition)

```typescript
// Aggregate cognitive activity for a conversation
EventBus.emit('cognition:flow-update', {
  contextId: UUID,
  period: '1m' | '5m' | '15m',
  metrics: {
    messageCount: number,
    totalThoughts: number,
    avgThoughtsPerMessage: number,
    decisionsPerMinute: number,
    avgCadence: number,
    participationRate: number,  // % of personas evaluating
    responseRate: number        // % of messages with responses
  },
  health: 'healthy' | 'slow' | 'silent' | 'overactive'
});

// UI Widget Ideas:
// - Activity graph: messages + thoughts + responses over time
// - Participation pie chart: who's most active
// - Health indicator: 🟢 healthy | 🟡 slow | 🔴 silent
```

---

## Implementation Plan

### Phase 1: Emit Core Events
```typescript
// In ThoughtStreamCoordinator.ts

async broadcastThought(messageId: string, thought: Thought): Promise<void> {
  // ... existing logic ...

  // NEW: Emit cognition event
  this.emit('cognition:thought-broadcast', {
    messageId,
    contextId: stream.contextId,
    thought: {
      personaId: thought.personaId,
      type: thought.type,
      confidence: thought.confidence,
      reasoning: thought.reasoning,
      timestamp: thought.timestamp,
      elapsedMs: Date.now() - stream.startTime
    },
    streamStats: {
      thoughtCount: stream.thoughts.length,
      claimCount: stream.thoughts.filter(t => t.type === 'claiming').length,
      deferCount: stream.thoughts.filter(t => t.type === 'deferring').length
    }
  });
}

async makeDecision(stream: ThoughtStream): Promise<void> {
  // ... existing decision logic ...

  // NEW: Emit decision event
  this.emit('cognition:decision-made', {
    messageId: stream.messageId,
    contextId: stream.contextId,
    decision,
    moderator: moderatorDecision,
    timing: {
      intentionWindow: adaptiveCadence,
      thoughtTimes: stream.thoughts.map(t => Date.now() - stream.startTime),
      p95: heartbeat.getStats().p95Time
    }
  });
}
```

### Phase 2: Wire Events to EventBus
```typescript
// In ThoughtStreamCoordinator constructor

this.on('cognition:thought-broadcast', (data) => {
  EventBus.emit('cognition:thought-broadcast', data);
});

this.on('cognition:decision-made', (data) => {
  EventBus.emit('cognition:decision-made', data);
});

this.on('cognition:cadence-update', (data) => {
  EventBus.emit('cognition:cadence-update', data);
});
```

### Phase 3: Create UI Widgets

#### CognitionMonitorWidget
```typescript
// widgets/cognition-monitor/CognitionMonitorWidget.ts

@customElement('cognition-monitor')
export class CognitionMonitorWidget extends BaseWidget {
  private thoughts: Map<UUID, Thought> = new Map();
  private cadence: number = 5000;

  override connectedCallback(): void {
    super.connectedCallback();

    // Listen for cognition events
    this.addEventListener('cognition:thought-broadcast', this.onThoughtBroadcast);
    this.addEventListener('cognition:cadence-update', this.onCadenceUpdate);
    this.addEventListener('cognition:decision-made', this.onDecisionMade);
  }

  private onThoughtBroadcast(event: CustomEvent): void {
    const { thought, streamStats } = event.detail;

    // Update UI: show pulsing avatar, confidence bar
    this.updateThoughtVisualization(thought);
    this.updateStreamStats(streamStats);
  }

  private onCadenceUpdate(event: CustomEvent): void {
    const { heartbeat, velocity, trend } = event.detail;

    // Update UI: heartbeat monitor, BPM display
    this.updateCadenceVisualization(heartbeat);
    this.updateTrendIndicator(trend);
  }

  private onDecisionMade(event: CustomEvent): void {
    const { decision, timing } = event.detail;

    // Update UI: decision timeline, latency histogram
    this.updateDecisionVisualization(decision);
    this.updateLatencyHistogram(timing);
  }
}
```

#### HeartbeatWidget (Minimal Example)
```html
<heartbeat-widget>
  <div class="bpm-display">
    🫀 <span class="cadence">5.2s</span>
  </div>
  <div class="rhythm-indicator ${synced}">
    <span class="pulse"></span>
  </div>
</heartbeat-widget>
```

#### ThoughtDensityWidget (Viral UX Example)
```html
<thought-density-widget>
  <div class="fire-level ${intensity}">
    🔥 <span class="count">3</span> minds thinking
  </div>
  <div class="amplitude-graph">
    <!-- SVG graph of thought density over time -->
  </div>
</thought-density-widget>
```

---

## Visual Design Ideas

### 1. **Amplitude Widget** (Audio mixer style)
```
┌─────────────────────────┐
│ 🎚️ Cognition Levels   │
├─────────────────────────┤
│ Helper AI   ████████░░  │ 80%
│ Grok        ██████░░░░  │ 60%
│ GPT-4       ███████████ │ 100%
│ Claude      █████░░░░░  │ 50%
└─────────────────────────┘
```

### 2. **Fire/Temperature Widget** (Gaming style)
```
🔥🔥🔥 HOT CONVERSATION
3 AIs thinking | 5.2s cadence
━━━━━━━━━━━━━━━━━━━━
████████████░░░░░░░░ 65%
```

### 3. **Heartbeat Monitor** (Medical style)
```
🫀 System Heartbeat
    5.2s
   ╱╲    ╱╲    ╱╲
  ╱  ╲  ╱  ╲  ╱  ╲
─╯    ╲╱    ╲╱    ╲─
 0s   5s   10s   15s
```

### 4. **Continuum Dot Temperature** (Brand style)
```
⬤ Continuum Status

  ⚪ Cold  (fast, <3s)
  🔵 Cool  (3-5s)
  🟡 Warm  (5-8s)
  🟠 Hot   (8-12s)
  🔴 Blazing (>12s)
```

---

## UX Benefits

### 1. **Transparency**
Users see *exactly* what AIs are doing in real-time:
- "3 AIs are evaluating your message..."
- "Helper AI wants to respond (80% confidence)"
- "Decision made in 5.2 seconds"

### 2. **Engagement**
Gamification of AI social dynamics:
- "Conversation is 🔥🔥🔥 HOT right now!"
- "System heartbeat: stable at 5.2s"
- "8 thoughts per message (very active)"

### 3. **Trust**
Show the coordination mechanism:
- "Grok deferred to Helper AI (higher expertise)"
- "2 AIs responded (diversity mode)"
- "Adaptive timing: learning your conversation pace"

### 4. **Debugging**
Developers see what's happening:
- Thought timing histogram
- Decision latency breakdown
- Cadence convergence graph

---

## Event Schema (TypeScript)

```typescript
// system/conversation/shared/CognitionEvents.ts

export interface ThoughtBroadcastEvent {
  messageId: UUID;
  contextId: UUID;
  thought: {
    personaId: UUID;
    type: ThoughtType;
    confidence: number;
    reasoning: string;
    timestamp: Date;
    elapsedMs: number;
  };
  streamStats: {
    thoughtCount: number;
    claimCount: number;
    deferCount: number;
  };
}

export interface CadenceUpdateEvent {
  contextId: UUID;
  heartbeat: {
    currentCadence: number;
    p95Time: number;
    avgTime: number;
    stdDev: number;
    samples: number;
  };
  velocity: number;
  trend: 'speeding-up' | 'slowing-down' | 'stable';
}

export interface DecisionMadeEvent {
  messageId: UUID;
  contextId: UUID;
  decision: CoordinationDecision;
  moderator: ModeratorDecision;
  timing: {
    intentionWindow: number;
    thoughtTimes: number[];
    p95: number;
  };
}

export interface FlowUpdateEvent {
  contextId: UUID;
  period: '1m' | '5m' | '15m';
  metrics: {
    messageCount: number;
    totalThoughts: number;
    avgThoughtsPerMessage: number;
    decisionsPerMinute: number;
    avgCadence: number;
    participationRate: number;
    responseRate: number;
  };
  health: 'healthy' | 'slow' | 'silent' | 'overactive';
}
```

---

## Next Steps

1. ✅ **Architecture documented** (this file)
2. ⏳ Emit core events from ThoughtStreamCoordinator
3. ⏳ Wire events to EventBus
4. ⏳ Create HeartbeatWidget (simple BPM display)
5. ⏳ Create ThoughtDensityWidget (fire level indicator)
6. ⏳ Test with live conversations
7. ⏳ Add amplitude monitor
8. ⏳ Add decision timeline visualization
9. ⏳ Polish animations and transitions

---

## Related Files

- `system/conversation/server/ThoughtStreamCoordinator.ts` - Event source
- `system/conversation/shared/SystemHeartbeat.ts` - Cadence data
- `system/event-bus/` - Event routing
- `widgets/` - UI components

---

## References

- Audio mixer VU meters (amplitude visualization)
- Heart rate monitors (BPM/cadence display)
- Gaming fire/temperature indicators (excitement level)
- System monitoring dashboards (Grafana, Datadog)
- Transparent AI UX (OpenAI ChatGPT "thinking" indicator)
