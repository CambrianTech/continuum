# PersonaUser Brain Widget - Performance Architecture
**Non-Blocking Event-Driven UI for Mind/Body/Soul/CNS Visualization**

## Problem Statement

**Current UI Has Performance Issues**:
- Events in widgets are killing performance
- Drawing operations block main thread
- Blocking strategies we cannot continue using
- Need efficient, non-blocking approach for real-time brain visualization

**Existing Infrastructure** (GOOD!):
✅ Four diamond/square controls already in UI (next to genome viz)
✅ Event-driven architecture already in place
✅ Click handlers ready (can navigate to content page or slide-out panel)
✅ Clean UI/UX aesthetic established

## Solution: Async, Throttled, RAF-Based Rendering

### Core Principles

1. **Never Block Main Thread**
   - Use `requestAnimationFrame` (RAF) for all drawing
   - Throttle event listeners (debounce/throttle patterns)
   - Use Web Workers for heavy computation
   - Defer non-critical updates

2. **Event Sampling (Not Every Event)**
   - Sample events at 10-30 Hz (not 1000 Hz)
   - Aggregate multiple events before rendering
   - Use dirty flags (only redraw when state changes)

3. **Incremental Rendering**
   - Render one subsystem per frame (Mind → Body → Soul → CNS rotation)
   - Use time-slicing for large data (render 100 items/frame max)
   - Lazy load detail views (only when user clicks)

4. **Off-Main-Thread Work**
   - Web Workers for: metric aggregation, log parsing, trace correlation
   - OffscreenCanvas for: complex visualizations, chart generation
   - Background threads for: data fetching, filtering, searching

---

## Architecture

### Component Hierarchy

```
PersonaBrainWidget (orchestrator)
├── BrainControlPanel (4 diamond controls)
│   ├── MindControl (diamond 1 - click → Mind detail)
│   ├── BodyControl (diamond 2 - click → Body detail)
│   ├── SoulControl (diamond 3 - click → Soul detail)
│   └── CNSControl  (diamond 4 - click → CNS detail)
│
├── DetailViewPanel (slide-out or full page)
│   ├── MindDetailView
│   ├── BodyDetailView
│   ├── SoulDetailView
│   └── CNSDetailView
│
└── StatusWorker (Web Worker - off main thread)
    └── Handles: event aggregation, metric calculation, log parsing
```

### Event Flow (Non-Blocking)

```
1. Subsystem emits event (e.g., "mind:thought:stored")
   ↓
2. Event listener adds to queue (O(1) operation, non-blocking)
   ↓
3. RAF callback processes queue (throttled to 30 FPS)
   ↓
4. Update virtual state (dirty flag set)
   ↓
5. Next RAF: Render changed subsystems only
   ↓
6. Browser paints (vsync, buttery smooth)
```

---

## Implementation Details

### 1. BrainControlPanel - Diamond Controls

**File**: `widgets/persona-brain/browser/BrainControlPanel.ts`

**Responsibilities**:
- Render 4 diamond/square controls (Mind/Body/Soul/CNS)
- Show live status (color, glow, pulse based on activity)
- Handle clicks (navigate to detail view or slide-out)
- **Non-blocking**: Use RAF for status updates, throttle events

**Performance Pattern**:
```typescript
export class BrainControlPanel extends HTMLElement {
  private statusUpdateQueue: StatusUpdate[] = [];
  private rafHandle: number | null = null;
  private isDirty = false;

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  connectedCallback(): void {
    this.render();
    this.subscribeToEvents();
    this.startRAFLoop();
  }

  disconnectedCallback(): void {
    this.unsubscribeFromEvents();
    this.stopRAFLoop();
  }

  /**
   * Subscribe to persona subsystem events
   * CRITICAL: Use throttled handlers to avoid flooding main thread
   */
  private subscribeToEvents(): void {
    // Mind events (throttled to 10 Hz = 100ms)
    Events.subscribe('persona:mind:state',
      this.throttle((event: MindStateEvent) => {
        this.queueStatusUpdate('mind', event);
      }, 100)
    );

    // Body events (throttled to 10 Hz)
    Events.subscribe('persona:body:tool-execution',
      this.throttle((event: ToolExecutionEvent) => {
        this.queueStatusUpdate('body', event);
      }, 100)
    );

    // Soul events (throttled to 10 Hz)
    Events.subscribe('persona:soul:genome-change',
      this.throttle((event: GenomeChangeEvent) => {
        this.queueStatusUpdate('soul', event);
      }, 100)
    );

    // CNS events (throttled to 10 Hz)
    Events.subscribe('persona:cns:routing-decision',
      this.throttle((event: CNSRoutingEvent) => {
        this.queueStatusUpdate('cns', event);
      }, 100)
    );
  }

  /**
   * Queue status update (non-blocking)
   * This is O(1) and never blocks main thread
   */
  private queueStatusUpdate(subsystem: SubsystemType, event: unknown): void {
    this.statusUpdateQueue.push({ subsystem, event, timestamp: Date.now() });
    this.isDirty = true;
  }

  /**
   * Start RAF loop (render at 30 FPS)
   * Lower FPS than 60 because status indicators don't need ultra-smooth animation
   */
  private startRAFLoop(): void {
    let lastFrameTime = 0;
    const targetFPS = 30;
    const frameDuration = 1000 / targetFPS;

    const loop = (timestamp: number): void => {
      this.rafHandle = requestAnimationFrame(loop);

      // Throttle to 30 FPS (33ms per frame)
      if (timestamp - lastFrameTime < frameDuration) {
        return;
      }

      lastFrameTime = timestamp;

      // Only render if dirty
      if (this.isDirty) {
        this.processQueueAndRender();
        this.isDirty = false;
      }
    };

    this.rafHandle = requestAnimationFrame(loop);
  }

  private stopRAFLoop(): void {
    if (this.rafHandle !== null) {
      cancelAnimationFrame(this.rafHandle);
      this.rafHandle = null;
    }
  }

  /**
   * Process queued status updates and render
   * Batches multiple events into single render pass
   */
  private processQueueAndRender(): void {
    if (this.statusUpdateQueue.length === 0) {
      return;
    }

    // Aggregate events by subsystem (most recent wins)
    const latestBySubsystem = new Map<SubsystemType, StatusUpdate>();
    for (const update of this.statusUpdateQueue) {
      latestBySubsystem.set(update.subsystem, update);
    }

    // Clear queue
    this.statusUpdateQueue.length = 0;

    // Update each subsystem control
    for (const [subsystem, update] of latestBySubsystem) {
      this.updateSubsystemControl(subsystem, update);
    }
  }

  /**
   * Update individual subsystem control (diamond/square)
   * This is cheap DOM operation (just CSS class changes)
   */
  private updateSubsystemControl(subsystem: SubsystemType, update: StatusUpdate): void {
    const control = this.shadowRoot?.querySelector(`[data-subsystem="${subsystem}"]`);
    if (!control) return;

    // Determine visual state from event
    const state = this.calculateVisualState(subsystem, update.event);

    // Apply CSS classes (GPU-accelerated, no layout thrash)
    control.className = `subsystem-control ${subsystem} ${state.status} ${state.activity}`;

    // Update tooltip
    control.setAttribute('title', state.tooltip);
  }

  /**
   * Calculate visual state from event data
   * This is pure computation (no DOM access)
   */
  private calculateVisualState(subsystem: SubsystemType, event: unknown): VisualState {
    // Example: Mind subsystem
    if (subsystem === 'mind' && isMindStateEvent(event)) {
      const energy = event.state.energy;

      return {
        status: energy > 0.7 ? 'healthy' : energy > 0.3 ? 'degraded' : 'low-energy',
        activity: event.isThinking ? 'active' : 'idle',
        tooltip: `Mind - Energy: ${(energy * 100).toFixed(0)}% - ${event.isThinking ? 'Thinking' : 'Idle'}`
      };
    }

    // Similar logic for Body/Soul/CNS...
    return { status: 'healthy', activity: 'idle', tooltip: subsystem };
  }

  /**
   * Throttle utility (prevents event flooding)
   */
  private throttle<T extends (...args: any[]) => void>(
    fn: T,
    delay: number
  ): T {
    let lastCall = 0;
    return ((...args: Parameters<T>) => {
      const now = Date.now();
      if (now - lastCall < delay) {
        return;
      }
      lastCall = now;
      fn(...args);
    }) as T;
  }

  /**
   * Render static HTML (one-time, on connectedCallback)
   * After this, only CSS classes change (cheap)
   */
  private render(): void {
    if (!this.shadowRoot) return;

    this.shadowRoot.innerHTML = `
      <style>
        /* GPU-accelerated animations */
        .subsystem-control {
          will-change: transform, opacity;
          transition: transform 0.2s ease, opacity 0.2s ease;
        }

        .subsystem-control.active {
          transform: scale(1.1);
        }

        .subsystem-control.healthy {
          border-color: var(--color-success);
        }

        .subsystem-control.degraded {
          border-color: var(--color-warning);
        }

        .subsystem-control.error {
          border-color: var(--color-error);
        }

        /* Diamond shape (existing styling) */
        .subsystem-control {
          width: 60px;
          height: 60px;
          transform: rotate(45deg);
          border: 2px solid var(--color-border);
          cursor: pointer;
          position: relative;
        }

        /* Add glow effect for active subsystems */
        .subsystem-control.active::before {
          content: '';
          position: absolute;
          inset: -4px;
          border: 2px solid currentColor;
          border-radius: inherit;
          opacity: 0.5;
          animation: pulse 1s ease-in-out infinite;
        }

        @keyframes pulse {
          0%, 100% { opacity: 0.3; transform: scale(1); }
          50% { opacity: 0.7; transform: scale(1.1); }
        }
      </style>

      <div class="brain-control-panel">
        <div class="subsystem-control mind" data-subsystem="mind" title="Mind">
          <span class="label">MIND</span>
        </div>
        <div class="subsystem-control body" data-subsystem="body" title="Body">
          <span class="label">BODY</span>
        </div>
        <div class="subsystem-control soul" data-subsystem="soul" title="Soul">
          <span class="label">SOUL</span>
        </div>
        <div class="subsystem-control cns" data-subsystem="cns" title="CNS">
          <span class="label">CNS</span>
        </div>
      </div>
    `;

    // Add click handlers (navigate to detail view or slide-out)
    this.shadowRoot.querySelectorAll('.subsystem-control').forEach(control => {
      control.addEventListener('click', (e) => {
        const subsystem = (e.currentTarget as HTMLElement).dataset.subsystem as SubsystemType;
        this.handleSubsystemClick(subsystem);
      });
    });
  }

  /**
   * Handle subsystem click (navigate or slide-out)
   */
  private handleSubsystemClick(subsystem: SubsystemType): void {
    // Option 1: Navigate to content page
    window.location.hash = `#persona/brain/${subsystem}`;

    // Option 2: Slide-out panel (preferred for performance)
    Events.emit('persona:brain:show-detail', { subsystem });
  }
}
```

---

### 2. DetailViewPanel - Slide-Out for Deep Dive

**File**: `widgets/persona-brain/browser/DetailViewPanel.ts`

**Responsibilities**:
- Slide-out panel OR full-page view for subsystem details
- Show working memory, tool queue, genome state, etc.
- **Lazy loaded**: Only render when user clicks diamond control
- **Virtual scrolling**: For long lists (1000+ items)

**Performance Pattern**:
```typescript
export class DetailViewPanel extends HTMLElement {
  private currentSubsystem: SubsystemType | null = null;
  private detailWorker: Worker | null = null;

  connectedCallback(): void {
    this.render();
    this.setupWorker();

    // Listen for show-detail events
    Events.subscribe('persona:brain:show-detail', (event) => {
      this.showDetailView(event.subsystem);
    });
  }

  /**
   * Setup Web Worker for heavy data processing
   * CRITICAL: Don't process thousands of log entries on main thread!
   */
  private setupWorker(): void {
    this.detailWorker = new Worker(new URL('./DetailWorker.ts', import.meta.url), {
      type: 'module'
    });

    this.detailWorker.onmessage = (event) => {
      this.handleWorkerResult(event.data);
    };
  }

  /**
   * Show detail view for subsystem
   * This opens the slide-out panel OR navigates to content page
   */
  private showDetailView(subsystem: SubsystemType): void {
    this.currentSubsystem = subsystem;

    // Show loading state immediately (cheap)
    this.showLoadingState(subsystem);

    // Fetch data in worker (off main thread)
    this.detailWorker?.postMessage({
      action: 'fetch-subsystem-data',
      subsystem,
      personaId: this.getPersonaId()
    });
  }

  /**
   * Handle worker result (data is already processed)
   */
  private handleWorkerResult(result: WorkerResult): void {
    if (result.action === 'subsystem-data-ready') {
      // Use virtual scrolling for large datasets
      this.renderSubsystemDetail(result.subsystem, result.data);
    }
  }

  /**
   * Render subsystem detail with virtual scrolling
   * Only renders visible items (not all 10,000 thoughts!)
   */
  private renderSubsystemDetail(subsystem: SubsystemType, data: SubsystemData): void {
    const container = this.shadowRoot?.querySelector('.detail-content');
    if (!container) return;

    // Example: Mind detail view
    if (subsystem === 'mind') {
      container.innerHTML = `
        <h2>Mind - Working Memory</h2>
        <virtual-scroll-list
          .items="${data.workingMemory}"
          .itemHeight="50"
          .renderItem="${(item: WorkingMemoryEntry) => this.renderThought(item)}"
        ></virtual-scroll-list>

        <h3>State</h3>
        <div class="state-metrics">
          <div class="metric">
            <span class="label">Energy:</span>
            <progress-bar value="${data.state.energy}" max="1"></progress-bar>
          </div>
          <div class="metric">
            <span class="label">Mood:</span>
            <span class="value">${data.state.mood}</span>
          </div>
        </div>
      `;
    }

    // Similar for Body/Soul/CNS...
  }

  private renderThought(thought: WorkingMemoryEntry): string {
    return `
      <div class="thought-item">
        <span class="importance">${thought.importance.toFixed(2)}</span>
        <span class="content">${thought.content}</span>
        <span class="timestamp">${new Date(thought.timestamp).toLocaleTimeString()}</span>
      </div>
    `;
  }
}
```

---

### 3. DetailWorker - Off Main Thread Processing

**File**: `widgets/persona-brain/browser/DetailWorker.ts`

**Responsibilities**:
- Fetch subsystem data (via Commands API)
- Aggregate metrics (average, percentiles, etc.)
- Filter/search large datasets
- Parse log files
- **All off main thread** - keeps UI responsive

**Performance Pattern**:
```typescript
// DetailWorker.ts (runs in Web Worker)
import { Commands } from '@system/core/shared/Commands';

self.onmessage = async (event: MessageEvent) => {
  const { action, subsystem, personaId } = event.data;

  if (action === 'fetch-subsystem-data') {
    try {
      // Fetch data via Commands API (works in Worker context)
      const data = await fetchSubsystemData(subsystem, personaId);

      // Post result back to main thread
      self.postMessage({
        action: 'subsystem-data-ready',
        subsystem,
        data
      });
    } catch (error) {
      self.postMessage({
        action: 'error',
        error: error.message
      });
    }
  }
};

async function fetchSubsystemData(subsystem: SubsystemType, personaId: string): Promise<SubsystemData> {
  switch (subsystem) {
    case 'mind':
      // Fetch working memory, persona state, etc.
      const workingMemory = await Commands.execute('mind/working-memory', {
        persona: personaId,
        limit: 1000  // Worker can handle large datasets
      });

      const state = await Commands.execute('mind/state', {
        persona: personaId
      });

      return { workingMemory, state };

    case 'body':
      // Fetch tool history, recent executions
      const toolHistory = await Commands.execute('body/tool-history', {
        persona: personaId,
        limit: 100
      });

      return { toolHistory };

    case 'soul':
      // Fetch genome state, memory stats
      const genome = await Commands.execute('soul/genome', {
        persona: personaId
      });

      return { genome };

    case 'cns':
      // Fetch scheduler state, metrics
      const scheduler = await Commands.execute('cns/scheduler', {
        persona: personaId
      });

      return { scheduler };

    default:
      throw new Error(`Unknown subsystem: ${subsystem}`);
  }
}
```

---

## Performance Benchmarks

### Target Metrics

**Main Thread Availability**:
- ✅ > 95% idle (< 5% CPU usage for brain widget)
- ✅ No frame drops during animation (solid 30 FPS)
- ✅ Event processing < 5ms per event

**Responsiveness**:
- ✅ Click-to-slide-out < 100ms
- ✅ Detail view loads < 500ms (even with 10,000 items)
- ✅ Status updates < 50ms (10 Hz throttling)

**Memory**:
- ✅ Brain widget < 10 MB heap (even with 4 personas open)
- ✅ Web Worker < 50 MB heap (even processing 100K log lines)

### Measurement Strategy

```typescript
// Performance measurement wrapper
class PerformanceMonitor {
  private marks: Map<string, number> = new Map();

  startMark(name: string): void {
    this.marks.set(name, performance.now());
  }

  endMark(name: string): number {
    const start = this.marks.get(name);
    if (!start) return 0;

    const duration = performance.now() - start;
    this.marks.delete(name);

    // Log to performance.log (separate from subsystem logs)
    console.debug(`[PERF] ${name}: ${duration.toFixed(2)}ms`);

    return duration;
  }

  measureFrameRate(callback: () => void): void {
    const start = performance.now();
    let frames = 0;

    const loop = (): void => {
      callback();
      frames++;

      if (performance.now() - start < 1000) {
        requestAnimationFrame(loop);
      } else {
        console.debug(`[PERF] Frame rate: ${frames} FPS`);
      }
    };

    requestAnimationFrame(loop);
  }
}

// Usage
const perfMonitor = new PerformanceMonitor();

perfMonitor.startMark('render-detail-view');
await renderSubsystemDetail('mind', data);
perfMonitor.endMark('render-detail-view');
// Output: [PERF] render-detail-view: 245.32ms
```

---

## Best Practices Summary

### DO ✅

1. **Use RAF for all animations/rendering**
2. **Throttle event listeners** (10-30 Hz max)
3. **Batch DOM updates** (read all, then write all - no interleaving)
4. **Use Web Workers for heavy processing**
5. **Virtual scrolling for long lists**
6. **Lazy load detail views** (only when user clicks)
7. **CSS animations over JS** (GPU-accelerated)
8. **will-change hints for animated elements**
9. **Dirty flags** (only render when state changes)
10. **Measure performance** (use Performance API)

### DON'T ❌

1. **Subscribe to high-frequency events without throttling**
2. **Block main thread with heavy computation**
3. **Render all items in long lists** (use virtual scrolling)
4. **Interleave DOM reads/writes** (causes layout thrash)
5. **Use setInterval for animations** (use RAF instead)
6. **Process log files on main thread** (use Web Worker)
7. **Render invisible elements** (lazy load, use IntersectionObserver)
8. **Inline styles** (use CSS classes, avoid forced reflow)
9. **Deep component trees** (flatten when possible)
10. **Synchronous I/O in UI code** (always async)

---

## Migration Plan

### Phase 1: Update BrainControlPanel (Existing Diamonds)

**Tasks**:
1. ✅ Add RAF loop to existing diamond controls
2. ✅ Throttle event listeners (10 Hz)
3. ✅ Implement dirty flag rendering
4. ✅ Add status aggregation (healthy/degraded/error)
5. ✅ GPU-accelerated animations (will-change + CSS transitions)

**Testing**:
- ✅ Measure main thread availability (> 95% idle)
- ✅ Measure frame rate (solid 30 FPS)
- ✅ Stress test: Send 1000 events/sec, verify no frame drops

### Phase 2: Create DetailViewPanel

**Tasks**:
1. ✅ Build slide-out panel component
2. ✅ Implement virtual scrolling for long lists
3. ✅ Add lazy loading for detail views
4. ✅ Wire up click handlers from diamond controls

**Testing**:
- ✅ Click-to-slide-out < 100ms
- ✅ Load 10,000 items without jank
- ✅ Measure memory usage (< 10 MB)

### Phase 3: Add Web Worker Processing

**Tasks**:
1. ✅ Create DetailWorker for data fetching
2. ✅ Move heavy processing off main thread
3. ✅ Implement log parsing in worker
4. ✅ Add metric aggregation in worker

**Testing**:
- ✅ Main thread stays idle during data processing
- ✅ Worker handles 100K log lines without blocking UI

---

## Related Documents

- `docs/personas/PERSONA-OBSERVABILITY-SYSTEM.md` - Full observability vision
- `docs/plans/PERSONA-LOGGING-AND-BASE-SUBSYSTEM.md` - Logging implementation plan
- `docs/architecture/widget-consolidation-migration-plan.md` - Widget architecture

---

**Document Status**: ✅ Ready for Implementation
**Priority**: MEDIUM (after logging infrastructure)
**Owner**: UI/UX + Being Architecture Teams
**Last Updated**: 2025-11-28
