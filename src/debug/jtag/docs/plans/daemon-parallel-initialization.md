# Daemon Parallel Initialization - Architecture Plan

## Problem: Sequential Bottleneck

**Current System:**
```typescript
// JTAGSystem.ts:176-189
const sortedDaemons = [...this.daemonEntries].sort((a, b) => a.priority - b.priority);

for (const daemonEntry of sortedDaemons) {
  await daemon.initializeDaemon(); // Sequential await!
}
```

**Issues:**
1. **Artificial ordering**: Priority forces total ordering (0 → 1 → 2 → 3...)
2. **Unnecessary waiting**: Daemon with priority 5 waits for ALL 0-4 daemons, even if it only needs DataDaemon
3. **Hard to maintain**: Adding new daemons requires careful priority assignment
4. **Circular dependency risk**: A → B and B → A can't be expressed
5. **No parallelism**: Daemons that don't depend on each other still initialize sequentially

**Real Example:**
```
DataDaemon (0) ────────────┐
SessionDaemon (1) ─────┐   │
EventsDaemon (2) ──┐   │   │
CommandDaemon (10) │   │   │  ← Waits for ALL above, even if only needs DataDaemon
ConsoleDaemon (10) │   │   │  ← Could run in parallel with CommandDaemon!
HealthDaemon (10)  │   │   │  ← Same here
                   └───┴───┘
                   Sequential bottleneck
```

## Solution: Dependency Graph with Parallel Execution

### Design Principles

1. **Declare dependencies explicitly**: Each daemon specifies what it needs
2. **Parallel by default**: All daemons start concurrently unless blocked by dependency
3. **Wait only on dependencies**: Daemon waits ONLY for its declared dependencies
4. **Detect circular dependencies**: Fail-fast on A → B → A cycles

### Architecture

#### 1. Daemon Dependency Declaration

```typescript
// daemons/command-daemon/server/CommandDaemonServer.ts
export class CommandDaemonServer extends DaemonBase {
  static readonly dependencies: string[] = ['data']; // Only needs DataDaemon
}

// daemons/session-daemon/server/SessionDaemonServer.ts
export class SessionDaemonServer extends SessionDaemon {
  static readonly dependencies: string[] = ['data']; // Only needs DataDaemon
}

// daemons/events-daemon/server/EventsDaemonServer.ts
export class EventsDaemonServer extends EventsDaemon {
  static readonly dependencies: string[] = ['data', 'session']; // Needs both
}

// daemons/data-daemon/server/DataDaemonServer.ts
export class DataDaemonServer extends DataDaemonBase {
  static readonly dependencies: string[] = []; // No dependencies - root
}
```

#### 2. Dependency Graph Builder

```typescript
// system/core/system/shared/DaemonDependencyGraph.ts
export class DaemonDependencyGraph {
  private adjacencyList: Map<string, string[]> = new Map();
  private completed: Set<string> = new Set();
  private pending: Map<string, Promise<void>> = new Map();

  constructor(daemonEntries: DaemonEntry[]) {
    // Build adjacency list from daemon declarations
    for (const entry of daemonEntries) {
      const deps = entry.daemonClass.dependencies || [];
      this.adjacencyList.set(entry.name, deps);
    }

    // Detect circular dependencies
    this.detectCycles();
  }

  /**
   * Get daemons ready to initialize (all dependencies completed)
   */
  getReadyDaemons(): string[] {
    const ready: string[] = [];

    for (const [daemon, dependencies] of this.adjacencyList.entries()) {
      // Skip if already completed or pending
      if (this.completed.has(daemon) || this.pending.has(daemon)) {
        continue;
      }

      // Check if all dependencies completed
      const allDepsComplete = dependencies.every(dep => this.completed.has(dep));
      if (allDepsComplete) {
        ready.push(daemon);
      }
    }

    return ready;
  }

  /**
   * Mark daemon as pending initialization
   */
  markPending(daemon: string, promise: Promise<void>): void {
    this.pending.set(daemon, promise);
  }

  /**
   * Mark daemon as completed
   */
  markCompleted(daemon: string): void {
    this.completed.add(daemon);
    this.pending.delete(daemon);
  }

  /**
   * Wait for specific daemon's dependencies
   */
  async waitForDependencies(daemon: string): Promise<void> {
    const dependencies = this.adjacencyList.get(daemon) || [];
    const promises = dependencies
      .map(dep => this.pending.get(dep))
      .filter(p => p !== undefined) as Promise<void>[];

    await Promise.all(promises);
  }

  /**
   * Detect circular dependencies using DFS
   */
  private detectCycles(): void {
    const visited = new Set<string>();
    const stack = new Set<string>();

    const dfs = (daemon: string, path: string[]): void => {
      if (stack.has(daemon)) {
        throw new Error(`Circular dependency detected: ${path.join(' → ')} → ${daemon}`);
      }

      if (visited.has(daemon)) {
        return;
      }

      visited.add(daemon);
      stack.add(daemon);

      const dependencies = this.adjacencyList.get(daemon) || [];
      for (const dep of dependencies) {
        dfs(dep, [...path, daemon]);
      }

      stack.delete(daemon);
    };

    for (const daemon of this.adjacencyList.keys()) {
      dfs(daemon, []);
    }
  }
}
```

#### 3. Parallel Initialization Engine

```typescript
// system/core/system/shared/JTAGSystem.ts
async setupDaemons(): Promise<void> {
  const graph = new DaemonDependencyGraph(this.daemonEntries);
  const daemonMap = new Map(this.daemonEntries.map(e => [e.name, e]));

  // Emit loading event
  this.router.eventManager.events.emit(SYSTEM_EVENTS.DAEMONS_LOADING, {
    context: this.context,
    timestamp: new Date().toISOString(),
    expectedDaemons: this.daemonEntries.map(d => d.name)
  });

  console.log(`🏗️ JTAG Server: Loading ${this.daemonEntries.length} daemons in parallel...`);

  // Initialize daemons wave by wave
  while (graph.getReadyDaemons().length > 0) {
    const readyDaemons = graph.getReadyDaemons();

    console.log(`🌊 Wave starting: ${readyDaemons.join(', ')}`);

    // Launch all ready daemons in parallel
    const promises = readyDaemons.map(async (daemonName) => {
      const entry = daemonMap.get(daemonName)!;

      try {
        // Wait for dependencies (should be instant since they're marked ready)
        await graph.waitForDependencies(daemonName);

        // Create and initialize daemon
        const daemon = this.createDaemon(entry, this.context, this.router);
        if (daemon) {
          await daemon.initializeDaemon();
          this.register(daemon);

          // Register CommandDaemon globally
          if (daemon.name === 'command-daemon' && typeof process !== 'undefined') {
            (globalThis as any).__JTAG_COMMAND_DAEMON__ = daemon;
          }

          console.log(`✅ ${daemonName} initialized`);
        }

        graph.markCompleted(daemonName);
      } catch (error) {
        console.error(`❌ Failed to initialize ${daemonName}:`, error);
        throw error;
      }
    });

    // Mark all as pending
    readyDaemons.forEach((name, idx) => {
      graph.markPending(name, promises[idx]);
    });

    // Wait for this wave to complete
    await Promise.all(promises);
  }

  // Emit loaded event
  this.router.eventManager.events.emit(SYSTEM_EVENTS.DAEMONS_LOADED, {
    context: this.context,
    timestamp: new Date().toISOString(),
    loadedDaemons: this.daemons.map(d => d.name)
  });

  console.log(`🔌 JTAG Server: All ${this.daemons.length} daemons initialized`);

  // Connect session (depends on all daemons ready)
  await this.connectSession();
}
```

### Initialization Flow Example

**Dependency Graph:**
```
DataDaemon (no deps) ─────┬─────┬─────────┬────────┬─────────┐
                          │     │         │        │         │
              SessionDaemon   CommandDaemon  ConsoleDaemon  HealthDaemon ...
                          │
                    EventsDaemon
```

**Execution Timeline:**
```
T=0ms:   DataDaemon starts (Wave 1)
T=100ms: DataDaemon completes
T=101ms: Wave 2 starts IN PARALLEL:
         - SessionDaemon
         - CommandDaemon
         - ConsoleDaemon
         - HealthDaemon
         - ProxyDaemon
         - etc (all that only depend on DataDaemon)
T=300ms: All Wave 2 daemons complete
T=301ms: Wave 3 starts:
         - EventsDaemon (depends on DataDaemon + SessionDaemon)
T=400ms: All daemons complete
```

**Before (Sequential):**
```
T=0ms:   DataDaemon (100ms)
T=100ms: SessionDaemon (200ms)    ← unnecessary wait
T=300ms: EventsDaemon (100ms)     ← waits for Session even though ready
T=400ms: CommandDaemon (50ms)     ← could have started at T=100ms!
T=450ms: ConsoleDaemon (50ms)     ← could have started at T=100ms!
T=500ms: HealthDaemon (50ms)      ← could have started at T=100ms!
Total: 600ms
```

**After (Parallel):**
```
T=0ms:   DataDaemon (100ms)
T=100ms: SessionDaemon (200ms) | CommandDaemon (50ms) | ConsoleDaemon (50ms) | HealthDaemon (50ms)
         All start in parallel!
T=300ms: EventsDaemon (100ms) ← waits only for SessionDaemon
T=400ms: All complete
Total: 400ms (1.5x faster!)
```

### Benefits

1. **True parallelism**: Daemons initialize as soon as dependencies ready
2. **Faster startup**: 1.5-3x faster depending on dependency structure
3. **Maintainable**: Just declare dependencies, graph handles ordering
4. **Safe**: Circular dependency detection prevents deadlocks
5. **Flexible**: Easy to add new daemons without priority gymnastics
6. **Observable**: Wave-based execution makes initialization visible

### Migration Path

**Phase 1: Add dependency declarations (1 day)**
- Add `static readonly dependencies: string[]` to all daemon classes
- Keep existing priority system working

**Phase 2: Implement graph infrastructure (1-2 days)**
- Create DaemonDependencyGraph class
- Add circular dependency detection
- Add tests for graph operations

**Phase 3: Integrate with JTAGSystem (1 day)**
- Replace priority-based sorting with graph-based waves
- Keep priority as fallback for compatibility
- Test with existing daemons

**Phase 4: Cleanup (1 day)**
- Remove priority declarations (obsolete)
- Update documentation
- Monitor startup times

### Dependency Reference

| Daemon | Dependencies | Reason |
|--------|--------------|--------|
| DataDaemon | [] | Root - no dependencies |
| SessionDaemon | [data] | Needs database for session storage |
| EventsDaemon | [data, session] | Emits events about sessions |
| CommandDaemon | [data] | Commands use database |
| ConsoleDaemon | [data] | Logs to database |
| HealthDaemon | [data] | Health metrics to database |
| ProxyDaemon | [data] | Proxy config from database |
| UserDaemon | [data] | User entities from database |
| WidgetDaemon | [data] | Widget state in database |
| AIProviderDaemon | [data] | Provider configs from database |
| ArtifactsDaemon | [data] | Artifacts stored in database |
| RoomMembershipDaemon | [data, session] | Room membership tracking |
| TrainingDaemon | [data] | Training data from database |
| CodeDaemon | [data] | Code operations use database |

### Testing Strategy

```typescript
// tests/unit/DaemonDependencyGraph.test.ts
describe('DaemonDependencyGraph', () => {
  it('detects circular dependencies', () => {
    const entries = [
      { name: 'a', daemonClass: { dependencies: ['b'] } },
      { name: 'b', daemonClass: { dependencies: ['a'] } } // Circular!
    ];

    expect(() => new DaemonDependencyGraph(entries as any))
      .toThrow('Circular dependency detected');
  });

  it('returns correct initialization waves', () => {
    const entries = [
      { name: 'data', daemonClass: { dependencies: [] } },
      { name: 'session', daemonClass: { dependencies: ['data'] } },
      { name: 'events', daemonClass: { dependencies: ['data', 'session'] } }
    ];

    const graph = new DaemonDependencyGraph(entries as any);

    // Wave 1: only data ready
    expect(graph.getReadyDaemons()).toEqual(['data']);

    graph.markCompleted('data');

    // Wave 2: session ready (data complete)
    expect(graph.getReadyDaemons()).toEqual(['session']);

    graph.markCompleted('session');

    // Wave 3: events ready (data + session complete)
    expect(graph.getReadyDaemons()).toEqual(['events']);
  });

  it('allows parallel initialization of independent daemons', () => {
    const entries = [
      { name: 'data', daemonClass: { dependencies: [] } },
      { name: 'command', daemonClass: { dependencies: ['data'] } },
      { name: 'console', daemonClass: { dependencies: ['data'] } },
      { name: 'health', daemonClass: { dependencies: ['data'] } }
    ];

    const graph = new DaemonDependencyGraph(entries as any);
    graph.markCompleted('data');

    // All three can start in parallel!
    const ready = graph.getReadyDaemons();
    expect(ready).toHaveLength(3);
    expect(ready).toContain('command');
    expect(ready).toContain('console');
    expect(ready).toContain('health');
  });
});
```

## Implementation Priority

**CRITICAL** - Fixes fundamental startup bottleneck

**Estimated Effort:** 3-4 days
- Day 1: Add dependency declarations to all daemons
- Day 2: Implement DaemonDependencyGraph with tests
- Day 3: Integrate with JTAGSystem, test end-to-end
- Day 4: Cleanup, documentation, monitor production

**Expected Results:**
- 1.5-3x faster daemon initialization
- Eliminates race conditions (each daemon waits for actual dependencies)
- Maintainable dependency management
- Safe circular dependency detection

## Appendix: Common Patterns

### Pattern 1: Database-Dependent Daemon
```typescript
export class MyDaemonServer extends DaemonBase {
  static readonly dependencies = ['data']; // Needs database

  async initialize() {
    // DataDaemon guaranteed ready
    const data = await DataDaemon.getData(...);
  }
}
```

### Pattern 2: Multi-Dependency Daemon
```typescript
export class EventsDaemonServer extends EventsDaemon {
  static readonly dependencies = ['data', 'session']; // Needs both

  async initialize() {
    // Both DataDaemon and SessionDaemon guaranteed ready
  }
}
```

### Pattern 3: Independent Daemon
```typescript
export class MyIndependentDaemon extends DaemonBase {
  static readonly dependencies = []; // No dependencies

  async initialize() {
    // Can start immediately (Wave 1 with DataDaemon)
  }
}
```

### Pattern 4: Chain Dependency
```typescript
// A depends on nothing
export class ADaemon extends DaemonBase {
  static readonly dependencies = [];
}

// B depends on A
export class BDaemon extends DaemonBase {
  static readonly dependencies = ['a'];
}

// C depends on B (transitively depends on A)
export class CDaemon extends DaemonBase {
  static readonly dependencies = ['b'];
}

// Execution: Wave 1: A, Wave 2: B, Wave 3: C
```
