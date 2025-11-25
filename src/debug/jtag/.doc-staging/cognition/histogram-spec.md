# Cognition Histogram - Frequency Analysis of AI Thought Pipeline

**Concept**: Visualize each stage of the ThoughtStream as a frequency band, showing cognitive "compute" or capacity usage in real-time.

---

## The Pipeline Stages (Frequency Bands)

```
Stage 1: RAG Build      ████████░░  80% (800ms)
Stage 2: Should Respond ██████░░░░  60% (120ms)
Stage 3: Generate       ███████░░░  70% (1400ms)
Stage 4: Coordination   ████░░░░░░  40% (200ms)
Stage 5: Post Response  ██████████  100% (50ms)
```

### 1. **RAG Build** (Context Gathering)
- **Metric**: Token count / time taken
- **Capacity**: Max context window (e.g., 128k tokens)
- **Visualization**: Height = % of context used, Color = speed (green=fast, red=slow)

```typescript
{
  stage: 'rag-build',
  tokensUsed: 4200,
  maxTokens: 128000,
  durationMs: 800,
  percentCapacity: 3.3,
  percentSpeed: 80  // Fast (< 1s)
}
```

### 2. **Should Respond** (Decision Making)
- **Metric**: Confidence score / evaluation time
- **Capacity**: Evaluation complexity (rule count, memory depth)
- **Visualization**: Height = confidence, Color = decision time

```typescript
{
  stage: 'should-respond',
  confidence: 0.85,
  evaluationMs: 120,
  rulesEvaluated: 5,
  memoriesChecked: 12,
  percentCapacity: 60,
  percentSpeed: 90  // Very fast (< 200ms)
}
```

### 3. **Generate** (LLM Inference)
- **Metric**: Output tokens / inference time
- **Capacity**: Model throughput (tokens/sec)
- **Visualization**: Height = output length, Color = generation speed

```typescript
{
  stage: 'generate',
  outputTokens: 150,
  inferenceMs: 1400,
  tokensPerSecond: 107,
  percentCapacity: 70,  // 150 tokens is moderate length
  percentSpeed: 60      // Moderate speed
}
```

### 4. **Coordination** (ThoughtStream Decision)
- **Metric**: Thought count / decision latency
- **Capacity**: Max personas in room
- **Visualization**: Height = participation rate, Color = decision speed

```typescript
{
  stage: 'coordination',
  thoughtsReceived: 5,
  maxPersonas: 10,
  decisionMs: 200,
  percentCapacity: 50,  // 5 of 10 personas evaluated
  percentSpeed: 80      // Fast decision
}
```

### 5. **Post Response** (Message Delivery)
- **Metric**: Delivery latency / processing time
- **Capacity**: Event queue depth
- **Visualization**: Height = queue depth, Color = latency

```typescript
{
  stage: 'post-response',
  deliveryMs: 50,
  queueDepth: 2,
  maxQueue: 20,
  percentCapacity: 10,  // Low queue depth (good!)
  percentSpeed: 100     // Instant delivery
}
```

---

## Visual Design (Winamp-Style)

### ASCII Prototype
```
┌─────────────────────────────────────────────────┐
│ 🧠 Cognition Pipeline - Helper AI              │
├─────────────────────────────────────────────────┤
│                                                 │
│ RAG Build      ████████░░  80%  800ms          │
│ Should Respond ██████░░░░  60%  120ms          │
│ Generate       ███████░░░  70%  1400ms         │
│ Coordination   ████░░░░░░  40%  200ms          │
│ Post Response  ██████████ 100%   50ms          │
│                                                 │
│ Total Latency: 2570ms                          │
│ Bottleneck: Generate (1400ms)                  │
└─────────────────────────────────────────────────┘
```

### HTML/CSS Prototype
```html
<div class="cognition-histogram">
  <div class="frequency-band rag-build">
    <div class="bar" style="width: 80%; background: linear-gradient(90deg, #0f0, #ff0)"></div>
    <div class="label">RAG Build</div>
    <div class="metrics">80% · 800ms</div>
  </div>

  <div class="frequency-band should-respond">
    <div class="bar" style="width: 60%; background: linear-gradient(90deg, #0f0, #0ff)"></div>
    <div class="label">Should Respond</div>
    <div class="metrics">60% · 120ms</div>
  </div>

  <!-- ... more bands ... -->
</div>
```

### Real-Time Animation
```typescript
class CognitionHistogram {
  updateBand(stage: string, value: number, duration: number) {
    // Smooth animation using requestAnimationFrame
    const bar = this.getBand(stage);
    const color = this.getColorForSpeed(duration);

    gsap.to(bar, {
      width: `${value}%`,
      background: color,
      duration: 0.3,
      ease: 'power2.out'
    });
  }

  getColorForSpeed(ms: number): string {
    // Green (fast) → Yellow → Red (slow)
    if (ms < 200) return 'linear-gradient(90deg, #0f0, #0ff)';
    if (ms < 1000) return 'linear-gradient(90deg, #ff0, #fa0)';
    return 'linear-gradient(90deg, #fa0, #f00)';
  }
}
```

---

## Multi-Persona View (The Orchestra)

Show ALL active personas side-by-side:

```
┌──────────────────────────────────────────────────────────────┐
│ 🎼 Cognition Orchestra                                       │
├──────────────────────────────────────────────────────────────┤
│              RAG    Eval   Gen    Coord  Post                │
│ Helper AI    ████   ███    ████   ██     ████                │
│ Grok         ███    ████   █████  ███    ████                │
│ GPT-4        ████   ████   ████   ████   ████                │
│ Claude       █      ██     ████   █      ████                │
│ Together     ███    ███    ████   ███    ████                │
│                                                              │
│ Total Compute: ████████░░  80% capacity                     │
│ Bottleneck: Generate stage (1400ms avg)                     │
└──────────────────────────────────────────────────────────────┘
```

Each row = one persona's pipeline
Each column = pipeline stage
Height = resource usage
Color = speed (green/yellow/red)

---

## Event Data Structure

```typescript
interface CognitionPipelineMetrics {
  messageId: UUID;
  personaId: UUID;
  timestamp: number;

  stages: {
    ragBuild: StageMetrics;
    shouldRespond: StageMetrics;
    generate: StageMetrics;
    coordination: StageMetrics;
    postResponse: StageMetrics;
  };

  totals: {
    latencyMs: number;
    bottleneck: string;      // Which stage was slowest
    computeScore: number;    // 0-100 aggregate
  };
}

interface StageMetrics {
  stage: string;
  durationMs: number;
  resourceUsed: number;      // Stage-specific (tokens, rules, etc)
  maxResource: number;       // Capacity limit
  percentCapacity: number;   // 0-100
  percentSpeed: number;      // 0-100 (relative to baseline)
  status: 'fast' | 'normal' | 'slow' | 'bottleneck';
}
```

---

## Emit Events from Pipeline

### In RAGBuilder
```typescript
// After building context
EventBus.emit('cognition:stage-complete', {
  messageId,
  personaId,
  stage: 'rag-build',
  metrics: {
    durationMs: Date.now() - startTime,
    resourceUsed: context.conversationHistory.length,
    maxResource: maxMessages,
    percentCapacity: (context.conversationHistory.length / maxMessages) * 100,
    percentSpeed: calculateSpeed(durationMs, 'rag-build')
  }
});
```

### In PersonaUser.evaluateShouldRespond()
```typescript
// After evaluation
EventBus.emit('cognition:stage-complete', {
  messageId,
  personaId: this.id,
  stage: 'should-respond',
  metrics: {
    durationMs: evalTime,
    resourceUsed: decision.confidence,
    maxResource: 1.0,
    percentCapacity: decision.confidence * 100,
    percentSpeed: calculateSpeed(evalTime, 'should-respond')
  }
});
```

### In PersonaUser.generateResponse()
```typescript
// After LLM generation
EventBus.emit('cognition:stage-complete', {
  messageId,
  personaId: this.id,
  stage: 'generate',
  metrics: {
    durationMs: generateTime,
    resourceUsed: outputTokens,
    maxResource: maxOutputTokens,
    percentCapacity: (outputTokens / maxOutputTokens) * 100,
    percentSpeed: calculateSpeed(generateTime, 'generate')
  }
});
```

---

## Widget Implementation

```typescript
@customElement('cognition-histogram')
export class CognitionHistogramWidget extends BaseWidget {
  private histograms: Map<UUID, PersonaHistogram> = new Map();

  override connectedCallback(): void {
    super.connectedCallback();

    // Listen for pipeline events
    EventBus.on('cognition:stage-complete', this.onStageComplete.bind(this));
    EventBus.on('cognition:pipeline-summary', this.onPipelineSummary.bind(this));
  }

  private onStageComplete(event: StageCompleteEvent): void {
    const { personaId, stage, metrics } = event;

    let histogram = this.histograms.get(personaId);
    if (!histogram) {
      histogram = new PersonaHistogram(personaId);
      this.histograms.set(personaId, histogram);
    }

    // Update specific frequency band
    histogram.updateBand(stage, metrics.percentCapacity, metrics.durationMs);

    // Animate bar with smooth transition
    this.animateBand(personaId, stage, metrics);
  }

  private animateBand(personaId: UUID, stage: string, metrics: StageMetrics): void {
    const element = this.querySelector(`[data-persona="${personaId}"] [data-stage="${stage}"]`);
    if (!element) return;

    // Color based on speed
    const color = this.getColorForSpeed(metrics.percentSpeed);

    // Animate bar
    gsap.to(element, {
      width: `${metrics.percentCapacity}%`,
      background: color,
      duration: 0.3,
      ease: 'power2.out'
    });

    // Update label
    const label = element.querySelector('.metrics');
    if (label) {
      label.textContent = `${metrics.percentCapacity.toFixed(0)}% · ${metrics.durationMs}ms`;
    }
  }

  private getColorForSpeed(speedPercent: number): string {
    // Fast (green) → Normal (yellow) → Slow (red)
    if (speedPercent >= 80) return 'linear-gradient(90deg, #0f0, #0ff)';
    if (speedPercent >= 50) return 'linear-gradient(90deg, #ff0, #fa0)';
    return 'linear-gradient(90deg, #fa0, #f00)';
  }
}
```

---

## Three.js Future Vision (Phase 2)

3D visualization of cognition pipeline:

```
       RAG
        ▲
        │ (height = token usage)
        │
   ┌────┼────┐
   │    │    │
Eval   Gen  Coord  (width = time, color = speed)
   │    │    │
   └────┼────┘
        │
        ▼
      Post
```

- **X-axis**: Pipeline stage progression
- **Y-axis**: Resource usage (capacity)
- **Z-axis**: Time (animate forward as messages flow)
- **Color**: Speed gradient (green → red)
- **Particles**: Thoughts flowing through pipeline

---

## Performance Considerations

- **Throttle updates**: Max 60fps (16.67ms per frame)
- **Aggregate metrics**: Update histograms every 100ms, not per-event
- **Lazy rendering**: Only render visible personas
- **Canvas optimization**: Use requestAnimationFrame, not CSS transitions

---

## Baseline Speeds (For Color Coding)

```typescript
const BASELINE_SPEEDS = {
  'rag-build': 500,        // < 500ms = fast
  'should-respond': 200,   // < 200ms = fast
  'generate': 1000,        // < 1s = fast
  'coordination': 300,     // < 300ms = fast
  'post-response': 100     // < 100ms = fast
};

function calculateSpeed(durationMs: number, stage: string): number {
  const baseline = BASELINE_SPEEDS[stage];
  // Return 0-100 score (100 = instant, 0 = very slow)
  return Math.max(0, Math.min(100, (1 - durationMs / (baseline * 2)) * 100));
}
```

---

## Next Steps

1. ✅ Architecture documented
2. ⏳ Emit stage-complete events from RAGBuilder
3. ⏳ Emit stage-complete events from PersonaUser
4. ⏳ Emit stage-complete events from ThoughtStreamCoordinator
5. ⏳ Create CognitionHistogramWidget
6. ⏳ Test with live conversations
7. ⏳ Add multi-persona orchestra view
8. ⏳ Optimize for 60fps
9. ⏳ Plan Three.js 3D visualization

---

## Related Files

- `system/conversation/server/ThoughtStreamCoordinator.ts` - Coordination stage
- `system/user/server/PersonaUser.ts` - Eval + Generate stages
- `system/rag/builders/ChatRAGBuilder.ts` - RAG Build stage
- `widgets/cognition-histogram/` - Visualization widget
- `COGNITION-EVENTS.md` - Event architecture

---

## References

- Winamp audio visualizer (frequency bands)
- Audio spectrum analyzer (real-time FFT)
- Chrome DevTools Performance (flame graphs)
- Grafana dashboards (time-series metrics)
- Three.js particle systems (future 3D viz)
