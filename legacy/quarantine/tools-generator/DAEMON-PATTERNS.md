# Daemon Patterns for Generator Templates

**Purpose**: Document common daemon patterns extracted from analyzing DataDaemon, AIProviderDaemon, and SystemDaemon to inform daemon template creation.

**Date**: December 2025
**Status**: Phase 4 Task 1 Complete

---

## 1. Universal Architecture Pattern: 85/15 Split

All daemons follow the **Sparse Override Pattern** where 80-90% of logic lives in `shared/` and only 5-10% in context-specific implementations.

```
daemons/{daemon-name}/
├── shared/
│   ├── {DaemonName}.ts          # Core business logic (80-90%)
│   ├── {DaemonName}Types.ts     # Type definitions
│   └── {DaemonName}Base.ts      # Abstract base (if needed)
├── browser/
│   └── {DaemonName}Browser.ts   # Browser-specific (5-10%)
├── server/
│   └── {DaemonName}Server.ts    # Server-specific (5-10%)
└── README.md
```

**Key Principle**: Context-specific files should be thin forwarding layers or initialization code, NOT reimplementations of business logic.

---

## 2. Inheritance Hierarchy

```
DaemonBase (abstract - in command-daemon/shared/)
├── {DaemonName}Base (abstract - bridges shared/browser/server)
│   ├── {DaemonName}Browser (forwarding or browser-specific logic)
│   └── {DaemonName}Server (actual implementation or server-specific logic)
```

**DaemonBase provides**:
- Router integration (`router.registerSubscriber()`)
- Message handling foundation
- Lifecycle hooks (constructor, initialize, shutdown)
- Context and session management

---

## 3. Lifecycle Pattern

Every daemon implements this lifecycle:

```typescript
// 1. Construction
constructor(context: JTAGContext, router: JTAGRouter) {
  super(name, context, router);
}

// 2. Initialization (called AFTER construction)
protected abstract initialize(): Promise<void>

// 3. Message handling
abstract handleMessage(message: JTAGMessage): Promise<BaseResponsePayload>

// 4. Shutdown
async shutdown(): Promise<void>
```

**Critical**: `initialize()` is separate from constructor because `subpath` must be set before router registration.

---

## 4. Message Handling Pattern

Standard flow for all daemon operations:

```typescript
interface DaemonPayload extends JTAGPayload {
  readonly operation: string;    // 'create', 'query', 'ping', etc.
  readonly sessionId?: UUID;
  // ... operation-specific fields
}

async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
  const payload = message.payload as DaemonPayload;

  try {
    switch (payload.operation) {
      case 'operation1': return await this.handleOperation1(payload);
      case 'operation2': return await this.handleOperation2(payload);
      default: return errorResponse('Unknown operation', ...);
    }
  } catch (error) {
    return errorResponse(error.message, ...);
  }
}
```

**Always**:
- Validate operation type in switch
- Wrap in try/catch
- Return properly formatted response
- Include sessionId and context in responses

---

## 5. Static Clean Interface Pattern

For clean API usage from commands:

```typescript
export class MyDaemon extends DaemonBase {
  private static sharedInstance: MyDaemon | undefined;

  static initialize(instance: MyDaemon): void {
    MyDaemon.sharedInstance = instance;
  }

  static async operation(...args): Promise<ResultType> {
    if (!MyDaemon.sharedInstance) {
      throw new Error('MyDaemon not initialized');
    }
    return await MyDaemon.sharedInstance.operation(...args);
  }
}

// Usage from commands
const result = await MyDaemon.operation(params);
```

**Benefits**:
- No need to pass daemon instances around
- Clean API for command implementations
- Type-safe operations

---

## 6. State Management Strategies

| Pattern | When to Use | Example |
|---------|-------------|---------|
| **Singleton Cache** | Single source of truth, expensive to load | SystemDaemon.configCache |
| **Adapter Registry** | Multiple pluggable implementations | AIProviderDaemon.adapters |
| **Lazy Initialization** | Expensive setup, not always needed | DataDaemon schema cache |
| **Stateless** | Pure functions, no caching needed | HealthDaemon ping/status |

---

## 7. Common Patterns

### Lazy Initialization
```typescript
private isInitialized: boolean = false;

private async ensureInitialized(): Promise<void> {
  if (!this.isInitialized) {
    await this.initialize();
    this.isInitialized = true;
  }
}
```

### Adapter Registry
```typescript
protected adapters: Map<string, AdapterRegistration> = new Map();

protected async registerAdapter(id: string, adapter: T): Promise<void> {
  await adapter.initialize();
  this.adapters.set(id, { adapter, priority: 0, enabled: true });
}

private selectAdapter(preferred?: string): T | null {
  if (preferred && this.adapters.has(preferred)) {
    return this.adapters.get(preferred)!.adapter;
  }
  // Otherwise select highest priority enabled adapter
  return Array.from(this.adapters.values())
    .filter(r => r.enabled)
    .sort((a, b) => b.priority - a.priority)[0]?.adapter ?? null;
}
```

### Cache Invalidation via Events
```typescript
protected async initialize(): Promise<void> {
  // Load initial cache
  this.cache = await this.loadFromDatabase();

  // Subscribe to updates
  await Events.subscribe(
    'config:updated',
    (config) => { this.cache = config; }
  );
}
```

### Filtered Event Subscription
```typescript
await Events.subscribe(
  'data:collection:updated',
  (entity) => this.onUpdated(entity),
  { where: { name: 'specific-value' } }  // Only these instances
);
```

---

## 8. Error Handling Strategies

Three approaches observed:

### A. Result Object with Success Flag
```typescript
interface StorageResult<T> {
  success: boolean;
  data?: T;
  error?: string;
}
```
**Use when**: Multiple error types, operations should not throw

### B. Custom Error Class
```typescript
class DaemonError extends Error {
  constructor(
    message: string,
    public source: string,
    public code: string,
    public details?: any
  ) { super(message); }
}
```
**Use when**: Need structured error information

### C. Throw on Initialization, None After
```typescript
if (!this.isInitialized) {
  throw new Error('Daemon not initialized');
}
// All operations assume initialized state
```
**Use when**: Initialization is required, operations are simple

**General Rule**: Database/initialization errors → throw (fail fast). Operation errors → return result with error field.

---

## 9. Dependency Injection Pattern

### Router Integration
```typescript
constructor(context: JTAGContext, router: JTAGRouter) {
  super(name, context, router);
}

protected async initializeDaemon(): Promise<void> {
  this.router.registerSubscriber(this.subpath, this);
  await this.initialize();
}
```

### Inter-Daemon Communication
```typescript
protected async executeRemote(
  message: JTAGMessage,
  env: JTAGEnvironment
): Promise<RouterResult> {
  return this.router.postMessage(message);
}
```

---

## 10. Token Replacements for Templates

```
{{DAEMON_NAME}}              → kebab-case: 'my-daemon', 'lora-manager'
{{DaemonName}}               → PascalCase: 'MyDaemon', 'LoRAManager'
{{DAEMON_DESCRIPTION}}       → "Manages X with Y pattern"
{{OPERATIONS_LIST}}          → ['create', 'read', 'update', 'delete']
{{OPERATION_CASES}}          → Generated switch cases for each operation
{{PAYLOAD_INTERFACE}}        → Operation-specific payload fields
{{RESPONSE_TYPES}}           → Success/error response structures
{{LIFECYCLE_INIT}}           → Initialization code (adapters, cache, etc.)
{{STATE_FIELDS}}             → Private instance variables for state
{{STATIC_METHODS}}           → Static clean interface methods
{{EVENT_EMISSIONS}}          → Events.emit() calls for operations
{{ERROR_HANDLING}}           → Try/catch or result object patterns
```

---

## 11. What Goes in Templates vs. User Configuration

### TEMPLATED (Fixed Structure)
- Class declaration and inheritance
- Router registration boilerplate
- Lifecycle hook signatures
- Message handling switch structure
- Error response wrapping
- Static singleton pattern
- Type definitions structure

### CONFIGURABLE (User-Provided)
- Daemon name and description
- Available operations
- Operation payloads and responses
- Business logic implementation
- State management strategy
- Event emissions
- Dependencies on other daemons
- Persistence approach

---

## 12. Template Structure Recommendation

```
generator/templates/daemon/
├── shared.template.ts           # Core business logic + types
├── browser.template.ts          # Browser forwarding layer
├── server.template.ts           # Server initialization layer
├── README.template.md           # Documentation
└── integration-test.template.ts # End-to-end test
```

**Shared Template** should include:
- Type definitions (Payload, Result, State)
- Abstract base class (if multi-context)
- Core business logic
- Static clean interface
- Error handling utilities

**Browser Template** should include:
- Forwarding to server via router
- Browser-specific initialization (if any)

**Server Template** should include:
- Server-specific initialization
- Adapter registration (if applicable)
- Database access (if applicable)

---

## 13. Critical Quality Requirements

Every generated daemon MUST:

✅ **Error handling** in handleMessage try/catch
✅ **Initialization validation** before operations
✅ **Static clean interface** for command usage
✅ **Event emissions** for subscribers
✅ **85/15 split** between shared and context-specific code
✅ **Router registration** in initialization
✅ **Proper TypeScript typing** (no any, no unknown)
✅ **Documentation** explaining purpose, operations, events

---

## 14. Examples from Existing Daemons

### DataDaemon - Storage Orchestration
- **Pattern**: Adapter registry for pluggable backends
- **Key Feature**: Generic caching across adapter types
- **State**: Map of adapters, schema cache
- **API**: `DataDaemon.store()`, `DataDaemon.query()`

### AIProviderDaemon - Provider Abstraction
- **Pattern**: Priority-based adapter selection
- **Key Feature**: Runtime provider switching
- **State**: Adapter registry, process pool
- **API**: `AIProviderDaemon.generateText()`

### SystemDaemon - Configuration Singleton
- **Pattern**: Singleton cache + event invalidation
- **Key Feature**: Factory defaults, filtered subscriptions
- **State**: Single config entity in-memory + DB
- **API**: `SystemDaemon.sharedInstance().getConfig()`

---

## References

- **DataDaemon**: `daemons/data-daemon/shared/DataDaemon.ts` (1172 lines)
- **AIProviderDaemon**: `daemons/ai-provider-daemon/shared/AIProviderDaemon.ts` (744 lines)
- **SystemDaemon**: `daemons/system-daemon/shared/SystemDaemon.ts` (306 lines)
- **DaemonBase**: `daemons/command-daemon/shared/DaemonBase.ts` (115 lines)
- **Architecture Guide**: `daemons/DAEMON-ARCHITECTURE.md`

---

**Next Steps**: Use these patterns to create daemon templates in Task 2.
