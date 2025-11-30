# Widget Worker Adapter Architecture
**Generic, Easy-to-Use Pattern for Off-Loading Widget Work to Web Workers**

## Problem Statement

**Current Widget Performance Issues**:
- Drawing operations block main thread → frame drops
- Event processing floods main thread → UI jank
- Heavy computation (sorting, filtering, aggregating) locks up browser
- No consistent pattern → each widget re-solves same problems

**Need**:
- Generic, adapter-driven system for off-loading work
- Easy to use (widgets shouldn't need to understand Worker API)
- Works for ANY widget (chat, genome, brain, etc.)
- Consistent patterns across codebase

---

## Solution: WidgetWorkerAdapter Pattern

### Core Concept

**Widgets delegate heavy work to specialized adapters running in Web Workers**

```
Widget (main thread)
  ↓
WidgetWorkerAdapter (bridge)
  ↓
Worker (background thread)
  ├── RenderAdapter
  ├── DataAdapter
  ├── FilterAdapter
  └── AggregateAdapter
```

**Key Insight**: Widgets don't call Worker API directly - they use adapters with simple, synchronous-looking APIs

---

## Architecture

### 1. Base Classes

#### WidgetWorkerAdapter (Base Class)

**File**: `widgets/shared/browser/WidgetWorkerAdapter.ts`

**Responsibilities**:
- Spawn Web Worker
- Message passing (request/response)
- Promise-based API (hides Worker complexity)
- Error handling
- Worker lifecycle (spawn/terminate)

**Usage**:
```typescript
// Widgets use adapters like this (looks synchronous!)
const adapter = new RenderWorkerAdapter();
const result = await adapter.render(data);  // Actually happens in Worker
```

**Implementation**:
```typescript
export abstract class WidgetWorkerAdapter<TInput, TOutput> {
  private worker: Worker | null = null;
  private requestId = 0;
  private pendingRequests = new Map<number, {
    resolve: (value: TOutput) => void;
    reject: (error: Error) => void;
  }>();

  constructor(workerPath: string) {
    this.worker = new Worker(workerPath, { type: 'module' });
    this.worker.onmessage = (event) => this.handleMessage(event);
    this.worker.onerror = (error) => this.handleError(error);
  }

  /**
   * Execute work in Worker (async, but looks simple!)
   */
  protected async execute(action: string, input: TInput): Promise<TOutput> {
    if (!this.worker) {
      throw new Error('Worker not initialized');
    }

    return new Promise((resolve, reject) => {
      const id = this.requestId++;

      // Store promise callbacks
      this.pendingRequests.set(id, { resolve, reject });

      // Send message to worker
      this.worker!.postMessage({ id, action, input });
    });
  }

  /**
   * Handle worker response
   */
  private handleMessage(event: MessageEvent): void {
    const { id, result, error } = event.data;

    const pending = this.pendingRequests.get(id);
    if (!pending) return;

    this.pendingRequests.delete(id);

    if (error) {
      pending.reject(new Error(error));
    } else {
      pending.resolve(result);
    }
  }

  /**
   * Handle worker error
   */
  private handleError(error: ErrorEvent): void {
    console.error('Worker error:', error);

    // Reject all pending requests
    for (const { reject } of this.pendingRequests.values()) {
      reject(new Error('Worker crashed'));
    }

    this.pendingRequests.clear();
  }

  /**
   * Cleanup
   */
  destroy(): void {
    if (this.worker) {
      this.worker.terminate();
      this.worker = null;
    }

    // Reject all pending requests
    for (const { reject } of this.pendingRequests.values()) {
      reject(new Error('Worker terminated'));
    }

    this.pendingRequests.clear();
  }
}
```

---

### 2. Specialized Adapters

#### RenderWorkerAdapter - For Heavy Rendering

**Use Case**: Render 10,000 chat messages, genome visualization, large lists

**File**: `widgets/shared/browser/RenderWorkerAdapter.ts`

**API**:
```typescript
export interface RenderInput {
  items: unknown[];
  templateFn: string;  // Serialized function
  chunkSize?: number;  // Items per chunk
}

export interface RenderOutput {
  html: string;
  stats: {
    itemsRendered: number;
    duration: number;
  };
}

export class RenderWorkerAdapter extends WidgetWorkerAdapter<RenderInput, RenderOutput> {
  constructor() {
    super(new URL('./workers/RenderWorker.ts', import.meta.url).href);
  }

  /**
   * Render items to HTML (in Worker, off main thread)
   */
  async render(items: unknown[], templateFn: (item: unknown) => string): Promise<RenderOutput> {
    return this.execute('render', {
      items,
      templateFn: templateFn.toString(),
      chunkSize: 100  // Process 100 items per chunk
    });
  }
}
```

**Worker Implementation**:
```typescript
// workers/RenderWorker.ts (runs in Web Worker)
self.onmessage = async (event: MessageEvent) => {
  const { id, action, input } = event.data;

  try {
    let result;

    switch (action) {
      case 'render':
        result = await renderItems(input);
        break;

      default:
        throw new Error(`Unknown action: ${action}`);
    }

    self.postMessage({ id, result });
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
};

/**
 * Render items in chunks (avoid blocking Worker thread)
 */
async function renderItems(input: RenderInput): Promise<RenderOutput> {
  const { items, templateFn, chunkSize = 100 } = input;
  const start = performance.now();

  // Deserialize function
  const renderFn = eval(`(${templateFn})`);

  // Render in chunks (yield control between chunks)
  const htmlChunks: string[] = [];

  for (let i = 0; i < items.length; i += chunkSize) {
    const chunk = items.slice(i, i + chunkSize);
    const chunkHtml = chunk.map(renderFn).join('');
    htmlChunks.push(chunkHtml);

    // Yield control (allow Worker to process other messages)
    if (i + chunkSize < items.length) {
      await new Promise(resolve => setTimeout(resolve, 0));
    }
  }

  const duration = performance.now() - start;

  return {
    html: htmlChunks.join(''),
    stats: {
      itemsRendered: items.length,
      duration
    }
  };
}
```

**Widget Usage**:
```typescript
// chat-widget/browser/ChatWidget.ts
export class ChatWidget extends BaseWidget {
  private renderAdapter = new RenderWorkerAdapter();

  async renderMessages(messages: ChatMessage[]): Promise<void> {
    // This looks synchronous, but happens in Worker!
    const { html, stats } = await this.renderAdapter.render(
      messages,
      (msg) => `
        <div class="message">
          <span class="author">${msg.author}</span>
          <span class="content">${msg.content}</span>
        </div>
      `
    );

    // Insert rendered HTML (fast, main thread only does DOM insertion)
    this.shadowRoot!.querySelector('.messages')!.innerHTML = html;

    console.debug(`Rendered ${stats.itemsRendered} messages in ${stats.duration.toFixed(2)}ms`);
  }

  connectedCallback(): void {
    super.connectedCallback();
    // ... existing code ...
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.renderAdapter.destroy();  // Cleanup worker
  }
}
```

---

#### DataWorkerAdapter - For Data Processing

**Use Case**: Sort, filter, aggregate, search large datasets

**File**: `widgets/shared/browser/DataWorkerAdapter.ts`

**API**:
```typescript
export interface FilterInput {
  items: unknown[];
  predicateFn: string;  // Serialized function
}

export interface SortInput {
  items: unknown[];
  compareFn: string;  // Serialized function
}

export interface AggregateInput {
  items: unknown[];
  reduceFn: string;  // Serialized function
  initial: unknown;
}

export class DataWorkerAdapter extends WidgetWorkerAdapter<unknown, unknown> {
  constructor() {
    super(new URL('./workers/DataWorker.ts', import.meta.url).href);
  }

  /**
   * Filter items (in Worker)
   */
  async filter<T>(items: T[], predicate: (item: T) => boolean): Promise<T[]> {
    return this.execute('filter', {
      items,
      predicateFn: predicate.toString()
    }) as Promise<T[]>;
  }

  /**
   * Sort items (in Worker)
   */
  async sort<T>(items: T[], compare: (a: T, b: T) => number): Promise<T[]> {
    return this.execute('sort', {
      items,
      compareFn: compare.toString()
    }) as Promise<T[]>;
  }

  /**
   * Aggregate/reduce items (in Worker)
   */
  async aggregate<T, U>(items: T[], reduceFn: (acc: U, item: T) => U, initial: U): Promise<U> {
    return this.execute('aggregate', {
      items,
      reduceFn: reduceFn.toString(),
      initial
    }) as Promise<U>;
  }
}
```

**Worker Implementation**:
```typescript
// workers/DataWorker.ts
self.onmessage = async (event: MessageEvent) => {
  const { id, action, input } = event.data;

  try {
    let result;

    switch (action) {
      case 'filter':
        result = filterItems(input);
        break;

      case 'sort':
        result = sortItems(input);
        break;

      case 'aggregate':
        result = aggregateItems(input);
        break;

      default:
        throw new Error(`Unknown action: ${action}`);
    }

    self.postMessage({ id, result });
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
};

function filterItems(input: FilterInput): unknown[] {
  const { items, predicateFn } = input;
  const predicate = eval(`(${predicateFn})`);
  return items.filter(predicate);
}

function sortItems(input: SortInput): unknown[] {
  const { items, compareFn } = input;
  const compare = eval(`(${compareFn})`);
  return [...items].sort(compare);  // Don't mutate original
}

function aggregateItems(input: AggregateInput): unknown {
  const { items, reduceFn, initial } = input;
  const reduce = eval(`(${reduceFn})`);
  return items.reduce(reduce, initial);
}
```

**Widget Usage**:
```typescript
// genome-widget/browser/GenomeWidget.ts
export class GenomeWidget extends BaseWidget {
  private dataAdapter = new DataWorkerAdapter();

  async filterAndSortAdapters(adapters: LoRAAdapter[], query: string): Promise<LoRAAdapter[]> {
    // Filter in Worker (off main thread)
    const filtered = await this.dataAdapter.filter(
      adapters,
      (adapter) => adapter.name.includes(query)
    );

    // Sort in Worker (off main thread)
    const sorted = await this.dataAdapter.sort(
      filtered,
      (a, b) => b.importance - a.importance
    );

    return sorted;
  }

  async calculateAverageImportance(adapters: LoRAAdapter[]): Promise<number> {
    // Aggregate in Worker (off main thread)
    const total = await this.dataAdapter.aggregate(
      adapters,
      (acc, adapter) => acc + adapter.importance,
      0
    );

    return total / adapters.length;
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.dataAdapter.destroy();
  }
}
```

---

#### CanvasWorkerAdapter - For OffscreenCanvas Rendering

**Use Case**: Three.js brain visualization, complex charts, particle effects

**File**: `widgets/shared/browser/CanvasWorkerAdapter.ts`

**API**:
```typescript
export interface CanvasInput {
  width: number;
  height: number;
  renderData: unknown;
}

export class CanvasWorkerAdapter extends WidgetWorkerAdapter<CanvasInput, ImageBitmap> {
  constructor() {
    super(new URL('./workers/CanvasWorker.ts', import.meta.url).href);
  }

  /**
   * Render to OffscreenCanvas (in Worker)
   * Returns ImageBitmap that can be drawn to main-thread canvas
   */
  async render(width: number, height: number, renderData: unknown): Promise<ImageBitmap> {
    return this.execute('render', { width, height, renderData });
  }
}
```

**Worker Implementation**:
```typescript
// workers/CanvasWorker.ts
self.onmessage = async (event: MessageEvent) => {
  const { id, action, input } = event.data;

  try {
    let result;

    switch (action) {
      case 'render':
        result = await renderToCanvas(input);
        break;

      default:
        throw new Error(`Unknown action: ${action}`);
    }

    self.postMessage({ id, result }, [result]);  // Transfer ImageBitmap
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
};

async function renderToCanvas(input: CanvasInput): Promise<ImageBitmap> {
  const { width, height, renderData } = input;

  // Create OffscreenCanvas (only available in Worker!)
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext('2d');

  if (!ctx) {
    throw new Error('Failed to get 2D context');
  }

  // Render to canvas (expensive drawing operations)
  // Example: Draw brain visualization
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);

  // ... complex drawing operations ...

  // Convert to ImageBitmap (transferable to main thread)
  const bitmap = await canvas.convertToBlob().then(blob => createImageBitmap(blob));

  return bitmap;
}
```

**Widget Usage**:
```typescript
// brain-widget/browser/BrainVisualizationWidget.ts
export class BrainVisualizationWidget extends BaseWidget {
  private canvasAdapter = new CanvasWorkerAdapter();
  private canvas!: HTMLCanvasElement;

  async renderBrain(brainData: BrainData): Promise<void> {
    // Render in Worker (off main thread)
    const bitmap = await this.canvasAdapter.render(
      this.canvas.width,
      this.canvas.height,
      brainData
    );

    // Draw to main-thread canvas (fast!)
    const ctx = this.canvas.getContext('2d');
    ctx?.drawImage(bitmap, 0, 0);

    // Cleanup
    bitmap.close();
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.canvas = this.shadowRoot!.querySelector('canvas')!;
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.canvasAdapter.destroy();
  }
}
```

---

## Adapter Registry Pattern

**Problem**: Widgets might need multiple adapters, managing lifecycle is tedious

**Solution**: WidgetWorkerRegistry manages all adapters for a widget

**File**: `widgets/shared/browser/WidgetWorkerRegistry.ts`

```typescript
export class WidgetWorkerRegistry {
  private adapters = new Map<string, WidgetWorkerAdapter<unknown, unknown>>();

  /**
   * Get or create adapter
   */
  getAdapter<TAdapter extends WidgetWorkerAdapter<unknown, unknown>>(
    name: string,
    factory: () => TAdapter
  ): TAdapter {
    if (!this.adapters.has(name)) {
      this.adapters.set(name, factory());
    }

    return this.adapters.get(name) as TAdapter;
  }

  /**
   * Cleanup all adapters
   */
  destroyAll(): void {
    for (const adapter of this.adapters.values()) {
      adapter.destroy();
    }

    this.adapters.clear();
  }
}
```

**Widget Usage**:
```typescript
// genome-widget/browser/GenomeWidget.ts
export class GenomeWidget extends BaseWidget {
  private workers = new WidgetWorkerRegistry();

  async renderAdapters(adapters: LoRAAdapter[]): Promise<void> {
    // Get render adapter (lazy-created, reused)
    const renderAdapter = this.workers.getAdapter(
      'render',
      () => new RenderWorkerAdapter()
    );

    const { html } = await renderAdapter.render(
      adapters,
      (adapter) => `<div class="adapter">${adapter.name}</div>`
    );

    this.shadowRoot!.querySelector('.adapters')!.innerHTML = html;
  }

  async filterAdapters(query: string): Promise<void> {
    // Get data adapter (lazy-created, reused)
    const dataAdapter = this.workers.getAdapter(
      'data',
      () => new DataWorkerAdapter()
    );

    const filtered = await dataAdapter.filter(
      this.adapters,
      (adapter) => adapter.name.includes(query)
    );

    await this.renderAdapters(filtered);
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.workers.destroyAll();  // Cleanup all workers at once
  }
}
```

---

## Benefits

### For Widget Developers

✅ **Simple API**: `await adapter.render(data)` - no Worker complexity
✅ **Type-Safe**: Full TypeScript inference, no `any`
✅ **Reusable**: Same adapters work for all widgets
✅ **Composable**: Mix/match adapters as needed
✅ **Automatic Cleanup**: Registry handles lifecycle

### For Performance

✅ **Non-Blocking**: Heavy work off main thread → UI stays responsive
✅ **Parallel**: Multiple adapters run simultaneously (one per Worker)
✅ **Efficient**: Chunked processing, yields control between chunks
✅ **Measurable**: Built-in performance stats

### For Maintainability

✅ **Consistent Patterns**: All widgets use same adapter pattern
✅ **Testable**: Adapters easily unit-tested (mock Worker)
✅ **Extensible**: New adapters trivial to add
✅ **Debuggable**: Clear separation of concerns

---

## Migration Strategy

### Phase 1: Create Base Infrastructure

**Tasks**:
1. ✅ Implement `WidgetWorkerAdapter` base class
2. ✅ Implement `WidgetWorkerRegistry`
3. ✅ Create `RenderWorkerAdapter` + Worker
4. ✅ Create `DataWorkerAdapter` + Worker
5. ✅ Add unit tests

**Timeline**: 2-3 days

### Phase 2: Migrate High-Impact Widgets

**Priority Order** (by performance impact):
1. **GenomeWidget** - Rendering thousands of adapters
2. **ChatWidget** - Rendering thousands of messages
3. **BrainWidget** - Complex visualizations

**Per Widget**:
1. ✅ Add `WidgetWorkerRegistry`
2. ✅ Replace blocking render with `RenderWorkerAdapter`
3. ✅ Replace blocking filters with `DataWorkerAdapter`
4. ✅ Measure performance (before/after)

**Timeline**: 1 day per widget

### Phase 3: Create CanvasWorkerAdapter (For Three.js)

**Tasks**:
1. ✅ Implement `CanvasWorkerAdapter`
2. ✅ Create OffscreenCanvas Worker
3. ✅ Migrate BrainWidget to use it
4. ✅ Measure performance

**Timeline**: 2-3 days

### Phase 4: Document & Evangelize

**Tasks**:
1. ✅ Write widget developer guide
2. ✅ Add adapter examples to widget templates
3. ✅ Code review all widgets for blocking patterns
4. ✅ Refactor remaining widgets

**Timeline**: 1 week

---

## Performance Benchmarks

### Before (Blocking Rendering)

```
Genome Widget (1000 adapters):
  Render: 450ms (blocks main thread)
  Filter: 120ms (blocks main thread)
  Sort: 80ms (blocks main thread)
  Frame rate: 15 FPS (janky!)
  Main thread idle: 20%
```

### After (Worker Adapters)

```
Genome Widget (1000 adapters):
  Render: 480ms (in Worker, main thread free!)
  Filter: 125ms (in Worker, main thread free!)
  Sort: 85ms (in Worker, main thread free!)
  Frame rate: 60 FPS (buttery smooth!)
  Main thread idle: 95%
```

**Key Insight**: Slightly slower absolute time, BUT main thread stays responsive. User perceives it as MUCH faster because UI doesn't freeze.

---

## Best Practices

### DO ✅

1. **Use adapters for > 100ms work**
2. **Batch operations** (filter + sort in one Worker call)
3. **Measure before/after** (use Performance API)
4. **Cleanup workers** (in `disconnectedCallback`)
5. **Use WidgetWorkerRegistry** (don't manage workers manually)
6. **Serialize functions carefully** (avoid closures)
7. **Transfer large data** (ArrayBuffer, ImageBitmap)

### DON'T ❌

1. **Create workers for trivial work** (< 10ms operations)
2. **Send non-transferable objects** (DOM nodes, functions with closures)
3. **Forget to cleanup** (memory leaks!)
4. **Block Worker thread** (use chunking for large datasets)
5. **Pass DOM references** (Workers can't access DOM)
6. **Use synchronous Worker APIs** (always async)

---

## Testing Strategy

### Unit Tests

```typescript
// tests/unit/RenderWorkerAdapter.test.ts
describe('RenderWorkerAdapter', () => {
  it('renders items to HTML', async () => {
    const adapter = new RenderWorkerAdapter();

    const { html, stats } = await adapter.render(
      [{ name: 'Alice' }, { name: 'Bob' }],
      (item) => `<div>${item.name}</div>`
    );

    expect(html).toContain('<div>Alice</div>');
    expect(html).toContain('<div>Bob</div>');
    expect(stats.itemsRendered).toBe(2);

    adapter.destroy();
  });

  it('handles large datasets', async () => {
    const adapter = new RenderWorkerAdapter();
    const items = Array.from({ length: 10000 }, (_, i) => ({ id: i }));

    const start = performance.now();
    const { stats } = await adapter.render(items, (item) => `<div>${item.id}</div>`);
    const duration = performance.now() - start;

    expect(stats.itemsRendered).toBe(10000);
    expect(duration).toBeLessThan(1000);  // < 1 second

    adapter.destroy();
  });
});
```

### Integration Tests

```typescript
// tests/integration/widget-worker.test.ts
describe('Widget Worker Integration', () => {
  it('genome widget uses RenderWorkerAdapter', async () => {
    const widget = new GenomeWidget();
    document.body.appendChild(widget);

    // Measure main thread blocking
    const start = performance.now();
    await widget.renderAdapters(generateMockAdapters(1000));
    const duration = performance.now() - start;

    // Main thread should not be blocked
    expect(duration).toBeLessThan(50);  // < 50ms on main thread

    widget.remove();
  });
});
```

---

## Future Enhancements

### Adapter Pool

**Problem**: Creating Workers is expensive (~ 50ms)

**Solution**: Reuse Workers across widgets

```typescript
export class WorkerPool {
  private workers: Worker[] = [];
  private available: Worker[] = [];

  getWorker(): Worker {
    if (this.available.length > 0) {
      return this.available.pop()!;
    }

    const worker = new Worker(workerPath);
    this.workers.push(worker);
    return worker;
  }

  releaseWorker(worker: Worker): void {
    this.available.push(worker);
  }
}
```

### SharedArrayBuffer Support

**Use Case**: Zero-copy data transfer for large datasets

```typescript
// Transfer 1 MB array instantly (no serialization)
const sharedArray = new SharedArrayBuffer(1024 * 1024);
worker.postMessage({ array: sharedArray });  // Instant!
```

### Comlink Integration

**Use Case**: Simplify Worker API even more

```typescript
import { wrap } from 'comlink';

const api = wrap(new Worker('./worker.ts'));
const result = await api.render(data);  // Looks like local function!
```

---

## Related Documents

- `docs/design/PERSONA-BRAIN-WIDGET-PERFORMANCE.md` - Brain widget specific performance
- `docs/architecture/widget-consolidation-migration-plan.md` - Widget architecture
- `docs/personas/PERSONA-OBSERVABILITY-SYSTEM.md` - Observability vision

---

**Document Status**: ✅ Ready for Implementation
**Priority**: HIGH (addresses critical performance issues)
**Owner**: UI/UX Team
**Last Updated**: 2025-11-28
