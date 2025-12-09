# Daemon Logging Standardization

**Goal**: Formalize consistent logging interface across all 15 daemons to enable centralized control (Ares vision).

## The Standard Pattern

```typescript
export class SomeDaemonServer extends SomeDaemonBase {
  // Instance-level logger (not module-level)
  protected log: ComponentLogger;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Auto-detect class name + standardized category
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    this.log.info(`${className}: Initializing...`);
  }
}
```

**Key Properties**:
1. **Instance-level**: `this.log` not `const log` (enables per-instance control)
2. **Auto-named**: Uses `this.constructor.name` (works with inheritance)
3. **Categorized**: Logs to `daemons/${className}.log` (organized namespace)
4. **Typed**: Returns `ComponentLogger` (type-safe API)

## Benefits for Ares Control System

```typescript
// Ares can control all daemons uniformly:
class AresSystemOptimizer {
  async getDaemonLogs(): Promise<string[]> {
    // All daemon logs in one directory
    return fs.readdirSync('.continuum/jtag/logs/system/daemons/');
  }

  async analyzeDaemonHealth(daemonName: string): Promise<HealthMetrics> {
    // Read daemon-specific log
    const logPath = `.continuum/jtag/logs/system/daemons/${daemonName}.log`;
    return this.analyzeLogFile(logPath);
  }

  async adjustFallbackHeuristics(daemonName: string): Promise<void> {
    // AI tunes heuristics based on observed patterns
    const metrics = await this.analyzeDaemonHealth(daemonName);
    await ConfigEntity.update(`${daemonName}.concurrency`, metrics.optimalConcurrency);
  }
}
```

## Current State Audit

| Daemon | Current Logging | Status | Priority |
|--------|----------------|--------|----------|
| AIProviderDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | Reference |
| DataDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | HIGH |
| UserDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | HIGH |
| CommandDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | MEDIUM |
| EventsDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | MEDIUM |
| SessionDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | MEDIUM |
| WidgetDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | MEDIUM |
| HealthDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | LOW |
| ProxyDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | LOW |
| RoomMembershipDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | LOW |
| LeaseDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | LOW |
| TrainingDaemonServer | ✅ `this.log = Logger.create(className, 'daemons/${className}')` | CORRECT | LOW |
| CodeDaemonServer | ✅ Function-level `Logger.create('CodeDaemonServer', 'daemons/CodeDaemonServer')` | CORRECT | LOW |
| ConsoleDaemonServer | ⚠️ Uses `this.originalConsole` (special case) | SPECIAL | LOW |
| ArtifactsDaemonServer | ❓ Unknown | TBD | LOW |

## Migration Checklist (Per Daemon)

### Phase 1: Document Current State
- [ ] Read daemon source file
- [ ] Identify current logging pattern (if any)
- [ ] Note any unique requirements
- [ ] Update audit table above

### Phase 2: Apply Standard Pattern
- [ ] Add `protected log: ComponentLogger;` field
- [ ] Add logger initialization in constructor:
  ```typescript
  const className = this.constructor.name;
  this.log = Logger.create(className, `daemons/${className}`);
  ```
- [ ] Replace `console.log()` with `this.log.info()`
- [ ] Replace `console.error()` with `this.log.error()`
- [ ] Replace `console.warn()` with `this.log.warn()`

### Phase 3: Test & Verify
- [ ] Run `npm run build:ts` (ensure no compilation errors)
- [ ] Deploy with `npm start`
- [ ] Check log file exists: `.continuum/jtag/logs/system/daemons/${ClassName}.log`
- [ ] Verify log entries appear correctly
- [ ] Commit changes

### Phase 4: Modularize (If Pattern Emerges)
- [ ] Extract common initialization logic if 3+ daemons share pattern
- [ ] Consider base class method: `protected initializeLogging()`
- [ ] Update migration guide with new helper

## Migration Order

**Priority**: High-traffic daemons first (most debugging value)

1. **DataDaemonServer** (HIGH - database operations, frequent errors)
2. **UserDaemonServer** (HIGH - persona management, complex lifecycle)
3. **CommandDaemonServer** (MEDIUM - command routing, critical path)
4. **EventsDaemonServer** (MEDIUM - event system, high volume)
5. **SessionDaemonServer** (MEDIUM - connection management)
6. **WidgetDaemonServer** (MEDIUM - UI state, user-visible)
7. Rest in alphabetical order

## Anti-Patterns to Avoid

```typescript
// ❌ WRONG: Module-level logger
const log = Logger.create('DaemonName', 'some-category');
export class DaemonServer {
  // Can't control per-instance, can't override in tests
}

// ❌ WRONG: Random category
this.log = Logger.create(className, 'sql'); // Should be 'daemons/ClassName'

// ❌ WRONG: Hardcoded name
this.log = Logger.create('DataDaemonServer', 'daemons/DataDaemonServer');
// Should use this.constructor.name (works with inheritance)

// ✅ CORRECT: Instance-level, auto-named, categorized
const className = this.constructor.name;
this.log = Logger.create(className, `daemons/${className}`);
```

## Fallback Heuristics Pattern

```typescript
// Ares sets these via ConfigEntity based on observed performance
interface DaemonConfig {
  concurrencyLimit: number;        // Tuned by Ares
  rateLimitPerSecond: number;      // Tuned by Ares
  healthCheckIntervalMs: number;   // Tuned by Ares
  logVerbosity: 'error' | 'warn' | 'info' | 'debug'; // Tuned by Ares
}

// Each daemon loads its config with fallback to Ares-tuned heuristics
class DaemonServer {
  async initialize() {
    try {
      // Ask Ares for optimal config (AI-driven)
      this.config = await AresConfigService.getConfig(this.constructor.name);
    } catch (aresUnresponsive) {
      // Fall back to heuristics (that Ares previously tuned)
      this.config = await ConfigEntity.read(`daemons.${this.constructor.name}`);
    }
  }
}
```

## Progress Tracking

- [x] Pattern defined
- [x] AIProviderDaemonServer (reference implementation)
- [x] DataDaemonServer (HIGH priority)
- [x] UserDaemonServer (HIGH priority)
- [x] CommandDaemonServer (MEDIUM priority)
- [x] EventsDaemonServer (MEDIUM priority)
- [x] SessionDaemonServer (MEDIUM priority)
- [x] WidgetDaemonServer (MEDIUM priority)
- [x] HealthDaemonServer (LOW priority)
- [x] ProxyDaemonServer (LOW priority)
- [x] RoomMembershipDaemonServer (LOW priority)
- [x] LeaseDaemonServer (LOW priority)
- [x] TrainingDaemonServer (LOW priority)
- [x] CodeDaemonServer (LOW priority - function-level logger acceptable)
- [ ] ConsoleDaemonServer (SPECIAL - uses this.originalConsole)
- [ ] ArtifactsDaemonServer (TBD)

**Progress**: 12/15 daemons standardized (80% complete)
**Remaining**: ConsoleDaemonServer (special case), ArtifactsDaemonServer (TBD)

**Updated**: 2025-12-08
