# Technical Debt Audit - Main Thread Issues

## Current State (Measured)

### 1. Type Safety Crisis
- **1,108 usages of `any`** across the codebase
- Violates strict typing principle from CLAUDE.md
- Causes runtime errors that TypeScript should catch

### 2. File Size Violations
**Top offenders** (>500 lines):
```
1652 lines - system/user/server/PersonaUser.ts
1647 lines - system/user/server/modules/PersonaResponseGenerator.ts
1402 lines - daemons/data-daemon/shared/DataDaemon.ts
1234 lines - daemons/data-daemon/server/RustWorkerStorageAdapter.ts
1225 lines - daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts
1214 lines - system/rag/builders/ChatRAGBuilder.ts
1210 lines - system/core/client/shared/JTAGClient.ts
```

**Rule**: Files should be <500 lines. These are 2-3x over limit.

### 3. Linting Broken
- ESLint configuration missing `@typescript-eslint/eslint-plugin`
- Can't catch type errors, unused imports, style violations
- Technical debt accumulates unchecked

### 4. Main Thread Bottlenecks (Suspected)

**Areas to audit:**
- Synchronous database operations (should be async)
- Large loops in Node (should be Rust workers)
- JSON parsing/serialization (move to Rust for large payloads)
- Blocking I/O (file reads, network calls)
- PersonaUser event processing (1652 lines of potential blocking)

### 5. Weak API Definitions

**Missing:**
- Strict request/response interfaces for Commands
- Protocol definitions for WebSocket messages
- Constant enums for magic strings
- Shared TypeScript types between client/server

---

## Action Plan (Priority Order)

### Phase 1: Fix Tooling (Unblock Development)

#### Task 1.1: Fix ESLint
```bash
npm install --save-dev @typescript-eslint/eslint-plugin @typescript-eslint/parser
npm run lint -- --fix  # Auto-fix what we can
```

#### Task 1.2: Enable Strict TypeScript
```json
// tsconfig.json
{
  "compilerOptions": {
    "strict": true,
    "noImplicitAny": true,
    "strictNullChecks": true,
    "strictFunctionTypes": true
  }
}
```

### Phase 2: Eliminate `any` (Type Safety)

#### Strategy:
1. **Find all `any` usages**: Already done (1,108 occurrences)
2. **Categorize by severity**:
   - Critical: In public APIs, command parameters, event payloads
   - High: In service layer, data access
   - Medium: In UI components, utilities
   - Low: In tests, mock data

3. **Replace systematically**:
   ```typescript
   // ❌ BEFORE
   function process(data: any) {
       return data.field;  // No type checking
   }

   // ✅ AFTER
   interface ProcessInput {
       field: string;
       metadata: Record<string, unknown>;
   }
   function process(data: ProcessInput): string {
       return data.field;  // Type-safe
   }
   ```

#### Tools:
```bash
# Find all any usages
grep -r ": any" --include="*.ts" system/ commands/ daemons/ > any-audit.txt

# Priority: Command types
grep -r ": any" --include="*Types.ts" commands/ > commands-any.txt

# Priority: Event types
grep -r ": any" --include="*Types.ts" system/core/shared/ > events-any.txt
```

### Phase 3: Split Oversized Files

#### Target: Get all files under 500 lines

**PersonaUser.ts (1652 lines) → Split into:**
```
PersonaUser.ts (300 lines) - Core state + lifecycle
PersonaInbox.ts (200 lines) - Message queue management
PersonaVoice.ts (200 lines) - Voice call handling
PersonaTools.ts (200 lines) - Tool execution
PersonaCognition.ts (200 lines) - Decision making
PersonaState.ts (200 lines) - Energy/mood tracking
PersonaGenome.ts (200 lines) - LoRA adapter management
```

**Principle**: One class per file, <500 lines per class

### Phase 4: Identify Main Thread Bottlenecks

#### Audit Checklist:

- [ ] **Synchronous operations in hot paths**
  ```bash
  grep -r "Sync(" system/ commands/ | grep -v "async" | grep -v node_modules
  ```

- [ ] **Large loops**
  ```bash
  grep -r "for.*length" system/ commands/ | wc -l
  ```

- [ ] **Blocking I/O**
  ```bash
  grep -r "readFileSync\|writeFileSync" system/ commands/
  ```

- [ ] **JSON.parse on large payloads**
  ```bash
  grep -r "JSON.parse" system/ commands/ | wc -l
  ```

- [ ] **Database operations not using workers**
  ```bash
  grep -r "sqlite3" system/ commands/ | grep -v "worker"
  ```

#### Move to Rust:
- Any loop over >100 items
- Any computation taking >10ms
- Any database query (use data-daemon worker)
- Any file I/O >1MB

### Phase 5: Define Strict Protocols

#### Create Protocol Definitions:

**File: `system/core/protocols/CommandProtocol.ts`**
```typescript
// All command requests must extend this
export interface CommandRequest {
    command: string;
    params: Record<string, unknown>;
    sessionId: UUID;
    context: ExecutionContext;
}

// All command responses must extend this
export interface CommandResponse<T = unknown> {
    success: boolean;
    data?: T;
    error?: string;
    timestamp: string;
}

// Type-safe command registry
export interface CommandRegistry {
    [K: string]: {
        request: CommandRequest;
        response: CommandResponse;
    };
}
```

**File: `system/core/protocols/EventProtocol.ts`**
```typescript
// All events must extend this
export interface BaseEvent<T = unknown> {
    type: string;
    payload: T;
    timestamp: number;
    sessionId?: UUID;
}

// Type-safe event registry
export type EventRegistry = {
    'data:created': BaseEvent<DataCreatedPayload>;
    'chat:message': BaseEvent<ChatMessagePayload>;
    'live:joined': BaseEvent<LiveJoinedPayload>;
    // ... all events
};
```

**File: `system/core/protocols/WebSocketProtocol.ts`**
```typescript
// All WebSocket messages must match this
export type WSMessage =
    | { type: 'join'; callId: string; userId: string; displayName: string }
    | { type: 'leave' }
    | { type: 'audio'; data: string }  // base64
    | { type: 'transcription'; text: string; confidence: number };

// Discriminated union ensures exhaustive matching
```

**File: `shared/constants/SystemConstants.ts`**
```typescript
// No more magic strings
export const COLLECTIONS = {
    USERS: 'users',
    ROOMS: 'rooms',
    MESSAGES: 'chat_messages',
    CALLS: 'calls',
} as const;

export const ROOM_UNIQUE_IDS = {
    GENERAL: 'general',
    ACADEMY: 'academy',
} as const;

export const DEFAULT_USER_UNIQUE_IDS = {
    PRIMARY_HUMAN: '@human',
    CLI_CLIENT: '@cli',
} as const;
```

---

## Measurement & Tracking

### Metrics to Track Weekly:

```bash
# Count any usages (target: 0)
grep -r ": any" --include="*.ts" system/ commands/ daemons/ | wc -l

# Count files >500 lines (target: 0)
find system/ commands/ daemons/ -name "*.ts" -exec wc -l {} + | awk '$1 > 500' | wc -l

# Count linting errors (target: 0)
npm run lint 2>&1 | grep "error" | wc -l

# Count TODO markers (track debt)
grep -r "TODO\|FIXME\|HACK" --include="*.ts" system/ commands/ | wc -l
```

### Success Criteria:

- [ ] ESLint runs successfully with 0 errors
- [ ] Zero usages of `any` in production code (tests ok)
- [ ] All files under 500 lines
- [ ] All commands use `Commands.execute<TReq, TRes>()` with strict types
- [ ] All events use `Events.emit<TPayload>()` with strict types
- [ ] No synchronous I/O in hot paths
- [ ] Main thread never blocks >16ms (60fps requirement)

---

## Implementation Strategy

**Week 1**: Fix tooling, measure baselines
**Week 2**: Eliminate top 100 `any` usages (commands, events)
**Week 3**: Split PersonaUser.ts, PersonaResponseGenerator.ts
**Week 4**: Define strict protocols, replace magic strings
**Week 5**: Identify and fix main thread bottlenecks
**Week 6**: Verify all metrics green, document patterns

**Principle**: Continuous improvement. Each PR must reduce technical debt metrics, never increase them.

---

## Prevention (Going Forward)

### Pre-commit Hooks:
```bash
# Reject commits with any
if git diff --cached | grep -q ": any"; then
    echo "ERROR: Commit contains 'any' type"
    exit 1
fi

# Reject commits with files >500 lines
# Reject commits that add synchronous I/O
# Reject commits without types on public APIs
```

### Code Review Checklist:
- [ ] No `any` types
- [ ] All APIs have strict TypeScript interfaces
- [ ] No files >500 lines
- [ ] No synchronous operations in hot paths
- [ ] No magic strings (use constants)
- [ ] Heavy computation in Rust workers

---

**The Goal**: Make the Node.js layer thin, type-safe, and reliable. Push all complexity to Rust workers where it belongs.
