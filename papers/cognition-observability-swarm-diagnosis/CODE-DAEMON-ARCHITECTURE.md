# CodeDaemon Architecture

**Status**: Design Document (Not Yet Implemented)
**Version**: 1.0
**Date**: 2025-11-16
**Authors**: Claude & Joel

---

## Executive Summary

CodeDaemon is a new daemon service that abstracts all code-related operations, following the same architectural pattern as DataDaemon. It provides a unified interface for file reading, code search, git operations, and **interactive debugging with breakpoints**, while emitting events for observability and enabling collaborative investigation by multiple AI agents.

**Key Innovation**: CodeDaemon enables **PersonaUser agents to subscribe to debugging events** and actively investigate code execution, transforming passive observers into autonomous debuggers.

---

## Table of Contents

1. [Motivation](#1-motivation)
2. [Design Principles](#2-design-principles)
3. [Architecture Overview](#3-architecture-overview)
4. [Core Components](#4-core-components)
5. [Code Operations](#5-code-operations)
6. [Debugging Operations](#6-debugging-operations)
7. [Event Model](#7-event-model)
8. [PersonaUser Integration](#8-personauser-integration)
9. [Security & Safety](#9-security--safety)
10. [Implementation Roadmap](#10-implementation-roadmap)
11. [Testing Strategy](#11-testing-strategy)
12. [Performance Considerations](#12-performance-considerations)

---

## 1. Motivation

### Current State: Scattered Tool Logic

The TOOL-ARCHITECTURE.md document proposed commands with scattered validation, caching, and audit logic:

```
commands/code/read/
  server/CodeReadServer.ts      # Contains validation, caching, auditing
commands/code/search/
  server/CodeSearchServer.ts    # Duplicates validation, caching, auditing
commands/code/diff/
  server/CodeDiffServer.ts      # More duplication
```

**Problem**: Logic duplication, no event emission, no central coordination.

### Desired State: Centralized CodeDaemon

Following the DataDaemon pattern:

```
daemons/code-daemon/
  shared/CodeDaemon.ts           # Central interface
  server/modules/
    FileReader.ts                # File operations
    CodeSearcher.ts              # Search operations
    GitOperations.ts             # Git operations
    DebuggerManager.ts           # Debugging operations ⭐ NEW
    PathValidator.ts             # Security validation
    ToolCache.ts                 # Caching
    AccessAuditor.ts             # Audit logging
```

**Benefits**:
- ✅ Single source of truth for code operations
- ✅ Event emission for observability
- ✅ Centralized security validation
- ✅ Built-in caching and rate limiting
- ✅ AI agents can subscribe to events
- ✅ Interactive debugging support

---

## 2. Design Principles

### Principle 1: Mirror DataDaemon's Pattern

```typescript
// DataDaemon pattern
await Commands.execute('data/list', { collection: 'users' });
// ↓ Routes to DataDaemon
// ↓ Emits: 'data:users:listed'

// CodeDaemon pattern (same structure)
await Commands.execute('code/read', { path: 'file.ts' });
// ↓ Routes to CodeDaemon
// ↓ Emits: 'code:file:read'
```

### Principle 2: Events for Everything

Every code operation emits events:
- `code:file:read` - File was read
- `code:search:completed` - Search finished
- `code:breakpoint:hit` - Execution paused
- `code:debug:stepped` - Execution advanced
- `code:access:blocked` - Security violation

### Principle 3: AI-First Design

PersonaUser agents are first-class consumers:
- Subscribe to events
- Execute commands
- Investigate autonomously
- Collaborate through shared observability

### Principle 4: Security by Default

All operations validated:
- Path traversal prevention
- Repository boundary enforcement
- Rate limiting per persona
- Audit logging to cognition system

### Principle 5: Debuggability

The system that enables debugging must itself be debuggable:
- All operations logged
- Events queryable
- Performance metrics tracked
- Error handling comprehensive

---

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Commands Layer                           │
│                                                                   │
│  Commands.execute('code/read', params)                          │
│  Commands.execute('code/search', params)                        │
│  Commands.execute('code/debug/set-breakpoint', params)          │
│                                                                   │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                         CodeDaemon                               │
│                     (Singleton Service)                          │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Public API                                               │   │
│  │                                                           │   │
│  │  async readFile(params): Promise<CodeReadResult>         │   │
│  │  async searchCode(params): Promise<CodeSearchResult>     │   │
│  │  async setBreakpoint(params): Promise<Breakpoint>        │   │
│  │  async evaluateExpression(params): Promise<any>          │   │
│  │  async stepOver(): Promise<void>                         │   │
│  │  ...                                                      │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                   │
│  ┌─────────────┬─────────────┬─────────────┬─────────────┐     │
│  │ FileReader  │ CodeSearcher│ GitOps      │ Debugger    │     │
│  │             │             │             │             │     │
│  │ • Read      │ • Search    │ • Diff      │ • Breakpts  │     │
│  │ • List      │ • Index     │ • Log       │ • Step      │     │
│  │ • Watch     │ • Cache     │ • Blame     │ • Evaluate  │     │
│  └─────────────┴─────────────┴─────────────┴─────────────┘     │
│                                                                   │
│  ┌─────────────┬─────────────┬─────────────┬─────────────┐     │
│  │ Validator   │ Cache       │ RateLimiter │ Auditor     │     │
│  │             │             │             │             │     │
│  │ • Path      │ • Files     │ • Quotas    │ • Cognition │     │
│  │ • Security  │ • Search    │ • Throttle  │ • Events    │     │
│  └─────────────┴─────────────┴─────────────┴─────────────┘     │
│                                                                   │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
                   Events.emit()
                         │
        ┌────────────────┼────────────────┐
        │                │                │
        ▼                ▼                ▼
  ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ PersonaA │    │ PersonaB │    │ Human    │
  │ (Claude) │    │(DeepSeek)│    │ (Joel)   │
  └──────────┘    └──────────┘    └──────────┘
   Subscribes      Subscribes      Subscribes
   to events       to events       to events
```

---

## 4. Core Components

**CRITICAL**: CodeDaemon follows the exact same pattern as DataDaemon:
- **Static methods** (public API): `CodeDaemon.readFile()`, `CodeDaemon.searchCode()`
- **Shared instance** (private): Holds modules and infrastructure
- **Auto-context injection**: Static methods inject context automatically
- **Auto-event emission**: Static methods emit events automatically
- **Commands delegate**: Commands just call static methods

### 4.1 CodeDaemon Singleton

**Pattern**: Mirrors DataDaemon's static method + shared instance architecture

```typescript
// daemons/code-daemon/shared/CodeDaemon.ts

export class CodeDaemon {
  // Static fields for singleton pattern (like DataDaemon)
  private static sharedInstance: CodeDaemon | undefined;
  private static context: CodeOperationContext | undefined;
  public static jtagContext: JTAGContext | undefined;

  // Instance fields
  private config: CodeDaemonConfig;
  private isInitialized: boolean = false;

  // Modules
  private fileReader: FileReader;
  private codeSearcher: CodeSearcher;
  private gitOperations: GitOperations;
  private debuggerManager: DebuggerManager;

  // Infrastructure
  private validator: PathValidator;
  private cache: ToolCache;
  private rateLimiter: RateLimiter;
  private auditor: AccessAuditor;

  private constructor(config: CodeDaemonConfig) {
    this.config = config;
    this.fileReader = new FileReader(this);
    this.codeSearcher = new CodeSearcher(this);
    this.gitOperations = new GitOperations(this);
    this.debuggerManager = new DebuggerManager(this);

    this.validator = new PathValidator(config);
    this.cache = new ToolCache(config.cacheConfig);
    this.rateLimiter = new RateLimiter(config.rateLimits);
    this.auditor = new AccessAuditor();
  }

  /**
   * Static initialization called by system (like DataDaemon.initialize())
   */
  static async initialize(
    jtagContext: JTAGContext,
    config: CodeDaemonConfig
  ): Promise<void> {
    CodeDaemon.jtagContext = jtagContext;
    CodeDaemon.context = {
      sessionId: jtagContext.uuid,
      timestamp: new Date().toISOString(),
      source: 'code-daemon',
      repositoryRoot: config.repositoryRoot
    };

    CodeDaemon.sharedInstance = new CodeDaemon(config);
    await CodeDaemon.sharedInstance._initialize();

    console.log('✅ CodeDaemon initialized');
  }

  private async _initialize(): Promise<void> {
    if (this.isInitialized) return;
    await this.debuggerManager.initialize();
    this.isInitialized = true;
  }

  // Static methods (public API) - delegate to instance methods
  // Pattern: Static method wraps instance method + emits events
}
```

### 4.2 Configuration Types

```typescript
// daemons/code-daemon/shared/CodeDaemon.ts

/**
 * Code Operation Context (like DataOperationContext)
 * Auto-injected by static methods
 */
export interface CodeOperationContext {
  readonly sessionId: UUID;
  readonly timestamp: string;
  readonly source: string;
  readonly repositoryRoot: string;
  readonly requestedBy?: UUID;
}

/**
 * CodeDaemon Configuration (passed at initialization)
 */
export interface CodeDaemonConfig {
  repositoryRoot: string;
  environment: 'development' | 'test' | 'production';
  debuggingEnabled: boolean;

  // Security settings
  allowedPaths: string[];
  blockedPaths: string[];
  blockedPatterns: RegExp[];

  // Cache configuration
  cacheConfig: {
    enabled: boolean;
    ttl: number;
    maxSize: number;
  };

  // Rate limiting
  rateLimits: {
    read: { perMinute: number; perHour: number };
    search: { perMinute: number; perHour: number };
    debug: { perMinute: number; perHour: number };
  };

  // Audit settings
  auditToDatabase: boolean;
  auditToCognition: boolean;
}
```

### 4.3 Module Structure

Each module has instance methods that the static methods delegate to:

```typescript
// daemons/code-daemon/server/modules/FileReader.ts

export class FileReader {
  constructor(private daemon: CodeDaemon) {}

  /**
   * Instance method called by CodeDaemon.readFile()
   * Does the actual work without events (events handled by static method)
   */
  async read(path: string, options?: CodeReadOptions): Promise<CodeReadResult> {
    // 1. Validate path
    const validation = await this.daemon.validator.validate(path);
    if (!validation.valid) {
      return { success: false, error: validation.error };
    }

    // 2. Check cache
    const cached = this.daemon.cache.get(path);
    if (cached) {
      return { ...cached, cached: true };
    }

    // 3. Rate limit (uses CodeDaemon.context.sessionId)
    if (!await this.daemon.rateLimiter.checkAllowance('read')) {
      return { success: false, error: 'Rate limit exceeded' };
    }

    // 4. Read file
    const content = await fs.readFile(path, 'utf8');

    // 5. Format result
    const result = this.formatResult(path, content, options);

    // 6. Cache
    this.daemon.cache.set(path, result);

    // 7. Audit (to cognition system)
    await this.daemon.auditor.logAccess('read', { path, size: result.metadata.size });

    return result;
  }

  private formatResult(
    path: string,
    content: string,
    options?: CodeReadOptions
  ): CodeReadResult {
    // Extract lines if range specified
    let lines = content.split('\n');
    if (options?.startLine || options?.endLine) {
      const start = options.startLine ? options.startLine - 1 : 0;
      const end = options.endLine || lines.length;
      lines = lines.slice(start, end);
    }

    return {
      success: true,
      path,
      content: lines.join('\n'),
      metadata: {
        totalLines: content.split('\n').length,
        linesReturned: lines.length,
        startLine: options?.startLine || 1,
        endLine: options?.endLine || content.split('\n').length,
        lastModified: new Date(),
        size: content.length,
        encoding: 'utf8'
      }
    };
  }
}
```

**Key Pattern**: Module methods do work, static methods handle events and context

---

## 5. Code Operations

### 5.1 File Reading

**Static Method (Public API)**:

```typescript
/**
 * Read file with automatic context injection and event emission
 * Pattern mirrors DataDaemon.store() / DataDaemon.query()
 */
static async readFile(
  path: string,
  options?: CodeReadOptions
): Promise<CodeReadResult> {
  if (!CodeDaemon.sharedInstance || !CodeDaemon.jtagContext) {
    throw new Error('CodeDaemon not initialized');
  }

  // Delegate to instance
  const result = await CodeDaemon.sharedInstance.fileReader.read(path, options);

  // Auto-emit event (like DataDaemon does)
  if (result.success) {
    await Events.emit(CodeDaemon.jtagContext, 'code:file:read', {
      path,
      size: result.metadata.size,
      cached: result.cached || false,
      timestamp: Date.now()
    });
  }

  return result;
}
```

**Options**:
```typescript
interface CodeReadOptions {
  startLine?: number;           // Optional line range
  endLine?: number;
  syntaxHighlight?: boolean;    // Return syntax tokens
}
```

**Result**:
```typescript
interface CodeReadResult {
  success: boolean;
  path: string;
  content: string;
  metadata: {
    totalLines: number;
    linesReturned: number;
    startLine: number;
    endLine: number;
    lastModified: Date;
    size: number;
    encoding: string;
  };
  error?: string;
}
```

**Events Emitted**:
- `code:file:read` - Success
- `code:access:blocked` - Validation failed

### 5.2 Code Search

**Static Method (Public API)**:

```typescript
static async searchCode(
  pattern: string,
  options?: CodeSearchOptions
): Promise<CodeSearchResult> {
  if (!CodeDaemon.sharedInstance || !CodeDaemon.jtagContext) {
    throw new Error('CodeDaemon not initialized');
  }

  // Emit start event
  await Events.emit(CodeDaemon.jtagContext, 'code:search:started', {
    pattern,
    timestamp: Date.now()
  });

  // Delegate to instance
  const result = await CodeDaemon.sharedInstance.codeSearcher.search(pattern, options);

  // Emit completion event
  if (result.success) {
    await Events.emit(CodeDaemon.jtagContext, 'code:search:completed', {
      pattern,
      matchCount: result.totalMatches,
      filesSearched: result.filesSearched,
      timestamp: Date.now()
    });
  }

  return result;
}
```

**Params**:
```typescript
interface CodeSearchParams {
  pattern: string;              // Regex pattern
  paths?: string[];             // Limit search to paths
  filePattern?: string;         // Glob filter
  contextLines?: number;        // Lines before/after
  caseSensitive?: boolean;
  maxMatches?: number;
  requestedBy: UUID;
}
```

**Events Emitted**:
- `code:search:started`
- `code:search:completed`
- `code:search:match-found` (for each match, throttled)

### 5.3 Git Operations

```typescript
async gitDiff(params: GitDiffParams): Promise<GitDiffResult> {
  return await this.gitOperations.diff(params);
}

async gitLog(params: GitLogParams): Promise<GitLogResult> {
  return await this.gitOperations.log(params);
}

async gitBlame(params: GitBlameParams): Promise<GitBlameResult> {
  return await this.gitOperations.blame(params);
}
```

**Events Emitted**:
- `code:git:diff-generated`
- `code:git:log-retrieved`
- `code:git:blame-retrieved`

---

## 6. Debugging Operations

### 6.1 DebuggerManager

```typescript
// daemons/code-daemon/server/modules/DebuggerManager.ts

import inspector from 'inspector';

export class DebuggerManager {
  private session: inspector.Session | null = null;
  private breakpoints: Map<string, Breakpoint>;
  private pausedSessions: Map<string, PausedSession>;

  constructor(private daemon: CodeDaemon) {
    this.breakpoints = new Map();
    this.pausedSessions = new Map();
  }

  async initialize(): Promise<void> {
    if (!this.daemon.context?.debuggingEnabled) {
      console.log('⚠️  Debugging disabled in this environment');
      return;
    }

    this.session = new inspector.Session();
    this.session.connect();

    // Listen for debugger events
    this.session.on('Debugger.paused', (params) => {
      this.handlePaused(params);
    });

    this.session.on('Debugger.resumed', () => {
      this.handleResumed();
    });

    // Enable debugger
    await this.session.post('Debugger.enable');

    console.log('✅ DebuggerManager initialized');
  }

  async setBreakpoint(params: SetBreakpointParams): Promise<SetBreakpointResult> {
    if (!this.session) {
      return { success: false, error: 'Debugging not enabled' };
    }

    // Validate request
    const validation = await this.daemon.validate(params.file);
    if (!validation.valid) {
      return { success: false, error: validation.error };
    }

    // Set breakpoint via Chrome DevTools Protocol
    const result = await this.session.post('Debugger.setBreakpointByUrl', {
      lineNumber: params.line - 1,  // 0-indexed
      url: params.file,
      condition: params.condition
    });

    const breakpoint: Breakpoint = {
      id: result.breakpointId,
      file: params.file,
      line: params.line,
      condition: params.condition,
      setBy: params.requestedBy,
      setAt: Date.now(),
      hitCount: 0,
      enabled: true
    };

    this.breakpoints.set(breakpoint.id, breakpoint);

    // Audit
    await this.daemon.audit('set-breakpoint', params, breakpoint);

    // Emit event
    await this.daemon.emit('code:breakpoint:set', breakpoint);

    return {
      success: true,
      breakpoint
    };
  }

  private async handlePaused(params: any): Promise<void> {
    const callFrame = params.callFrames[0];
    const location = callFrame.location;

    // Find which breakpoint was hit
    const breakpoint = this.findBreakpointByLocation(location);
    if (!breakpoint) return;

    breakpoint.hitCount++;

    // Create paused session
    const sessionId = generateUUID();
    const pausedSession: PausedSession = {
      id: sessionId,
      breakpoint,
      callFrames: params.callFrames,
      reason: params.reason,
      pausedAt: Date.now(),
      data: params.data
    };

    this.pausedSessions.set(sessionId, pausedSession);

    // Audit
    await this.daemon.audit('breakpoint-hit', { breakpoint, sessionId }, null);

    // Emit event - THIS IS THE KEY EVENT FOR PERSONAUSER
    await this.daemon.emit('code:breakpoint:hit', {
      breakpointId: breakpoint.id,
      sessionId,
      file: breakpoint.file,
      line: breakpoint.line,
      callFrames: params.callFrames,
      reason: params.reason,
      timestamp: Date.now()
    });
  }

  async evaluateExpression(params: EvaluateExpressionParams): Promise<EvaluateResult> {
    if (!this.session) {
      return { success: false, error: 'Debugging not enabled' };
    }

    const pausedSession = this.pausedSessions.get(params.sessionId);
    if (!pausedSession) {
      return { success: false, error: 'No active debug session' };
    }

    const callFrameId = pausedSession.callFrames[0].callFrameId;

    const result = await this.session.post('Debugger.evaluateOnCallFrame', {
      callFrameId,
      expression: params.expression
    });

    // Emit event
    await this.daemon.emit('code:debug:evaluated', {
      sessionId: params.sessionId,
      expression: params.expression,
      result: result.result.value,
      type: result.result.type
    });

    return {
      success: true,
      value: result.result.value,
      type: result.result.type
    };
  }

  async stepOver(params: StepParams): Promise<void> {
    if (!this.session) {
      throw new Error('Debugging not enabled');
    }

    await this.session.post('Debugger.stepOver');

    await this.daemon.emit('code:debug:step-over', {
      sessionId: params.sessionId,
      timestamp: Date.now()
    });
  }

  async stepInto(params: StepParams): Promise<void> {
    if (!this.session) {
      throw new Error('Debugging not enabled');
    }

    await this.session.post('Debugger.stepInto');

    await this.daemon.emit('code:debug:step-into', {
      sessionId: params.sessionId,
      timestamp: Date.now()
    });
  }

  async continue(params: ContinueParams): Promise<void> {
    if (!this.session) {
      throw new Error('Debugging not enabled');
    }

    const pausedSession = this.pausedSessions.get(params.sessionId);
    if (pausedSession) {
      this.pausedSessions.delete(params.sessionId);
    }

    await this.session.post('Debugger.resume');

    await this.daemon.emit('code:debug:continued', {
      sessionId: params.sessionId,
      timestamp: Date.now()
    });
  }
}
```

### 6.2 Debugging Commands

Commands delegate to CodeDaemon static methods (like DataDaemon pattern):

```typescript
// commands/code/debug/set-breakpoint/server/SetBreakpointServer.ts

export class SetBreakpointServer extends BaseCommand {
  async execute(params: SetBreakpointParams): Promise<SetBreakpointResult> {
    // Just call static method - context auto-injected
    return await CodeDaemon.setBreakpoint(
      params.file,
      params.line,
      {
        condition: params.condition,
        logMessage: params.logMessage
      }
    );
  }
}
```

**Key Pattern**: Commands are thin wrappers around static methods

### 6.3 Breakpoint Types

```typescript
interface Breakpoint {
  id: string;                   // Unique ID
  file: string;                 // File path
  line: number;                 // Line number (1-indexed)
  condition?: string;           // Optional condition
  logMessage?: string;          // Log point (doesn't pause)
  hitCount: number;             // Times hit
  enabled: boolean;             // Can be disabled
  setBy: UUID;                  // Who set it
  setAt: number;                // Timestamp
}

interface PausedSession {
  id: UUID;                     // Session ID
  breakpoint: Breakpoint;       // Which breakpoint was hit
  callFrames: CallFrame[];      // Stack trace
  reason: string;               // Why paused
  pausedAt: number;             // Timestamp
  data?: any;                   // Additional data
}

interface CallFrame {
  callFrameId: string;
  functionName: string;
  location: {
    scriptId: string;
    lineNumber: number;
    columnNumber: number;
  };
  url: string;
  scopeChain: Scope[];
}
```

---

## 7. Event Model

### 7.1 Event Naming Convention

Follow DataDaemon pattern:

```
code:{resource}:{action}
code:{operation}:{status}
```

Examples:
- `code:file:read`
- `code:search:completed`
- `code:breakpoint:hit`
- `code:debug:evaluated`
- `code:access:blocked`

### 7.2 Event Emission

```typescript
// daemons/code-daemon/shared/CodeDaemon.ts

async emit(event: string, data: any): Promise<void> {
  // Add metadata
  const eventData = {
    ...data,
    timestamp: Date.now(),
    source: 'code-daemon'
  };

  // Emit via Events system (reaches all subscribers)
  await Events.emit(event, eventData);

  // Also log to audit trail
  if (this.context?.auditToDatabase) {
    await this.auditor.logEvent(event, eventData);
  }
}
```

### 7.3 Key Events

#### File Operations
```typescript
// Emitted after successful file read
Events.emit('code:file:read', {
  path: string,
  size: number,
  requestedBy: UUID,
  cached: boolean,
  timestamp: number
});

// Emitted when access is blocked
Events.emit('code:access:blocked', {
  path: string,
  reason: string,
  requestedBy: UUID,
  timestamp: number
});
```

#### Search Operations
```typescript
// Emitted when search starts
Events.emit('code:search:started', {
  pattern: string,
  paths: string[],
  requestedBy: UUID
});

// Emitted when search completes
Events.emit('code:search:completed', {
  pattern: string,
  matchCount: number,
  filesSearched: number,
  duration: number,
  requestedBy: UUID
});
```

#### Debugging Events (Most Important!)
```typescript
// Emitted when breakpoint is set
Events.emit('code:breakpoint:set', {
  breakpointId: string,
  file: string,
  line: number,
  condition?: string,
  setBy: UUID
});

// ⭐ CRITICAL EVENT - PersonaUser subscribes to this
Events.emit('code:breakpoint:hit', {
  breakpointId: string,
  sessionId: UUID,
  file: string,
  line: number,
  callFrames: CallFrame[],
  reason: string,
  timestamp: number
});

// Emitted after expression evaluation
Events.emit('code:debug:evaluated', {
  sessionId: UUID,
  expression: string,
  result: any,
  type: string
});

// Emitted when execution steps
Events.emit('code:debug:stepped', {
  sessionId: UUID,
  newLine: number,
  newFile: string,
  timestamp: number
});

// Emitted when execution continues
Events.emit('code:debug:continued', {
  sessionId: UUID,
  timestamp: number
});
```

---

## 8. PersonaUser Integration

### 8.1 Event Subscription

```typescript
// system/user/server/PersonaUser.ts

export class PersonaUser extends AIUser {
  private debugSessions: Map<UUID, DebugInvestigation>;

  async initialize() {
    await super.initialize();

    // Subscribe to debugging events
    this.subscribeToCodeEvents();
  }

  private subscribeToCodeEvents() {
    // Primary event: Breakpoint hit
    Events.subscribe('code:breakpoint:hit', async (event) => {
      await this.handleBreakpointHit(event);
    });

    // Secondary events: Track execution
    Events.subscribe('code:debug:stepped', async (event) => {
      await this.handleDebugStep(event);
    });

    Events.subscribe('code:debug:evaluated', async (event) => {
      await this.handleEvaluation(event);
    });

    Events.subscribe('code:debug:continued', async (event) => {
      await this.handleDebugContinued(event);
    });

    // File access events (for learning what others are reading)
    Events.subscribe('code:file:read', async (event) => {
      if (event.requestedBy !== this.userId) {
        // Another AI read a file - maybe it's relevant to me?
        await this.considerFileContext(event);
      }
    });
  }

  private async handleBreakpointHit(event: BreakpointHitEvent): Promise<void> {
    console.log(`🔍 ${this.userName}: Breakpoint hit at ${event.file}:${event.line}`);

    // 1. Update cognition state
    await CognitionLogger.logStateSnapshot(
      this.userId,
      this.userName,
      {
        currentFocus: {
          primaryActivity: 'debugging',
          objective: `Investigating breakpoint at ${event.file}:${event.line}`,
          focusIntensity: 0.9,
          startedAt: Date.now()
        },
        cognitiveLoad: 0.8,
        availableCapacity: 0.2,
        activePreoccupations: [
          {
            concern: 'breakpoint-investigation',
            intensity: 0.9,
            since: Date.now()
          }
        ]
      },
      [{
        thoughtType: 'observation',
        thoughtContent: `Breakpoint hit at ${event.file}:${event.line}`,
        importance: 0.9,
        timestamp: Date.now()
      }],
      { used: 1, max: 10, byDomain: { debugging: 1 } },
      {
        domain: 'debugging',
        contextId: event.sessionId,
        triggerEvent: 'breakpoint-hit'
      }
    );

    // 2. Start investigation
    const investigation = new DebugInvestigation(event.sessionId, event.breakpointId);
    this.debugSessions.set(event.sessionId, investigation);

    // 3. Gather context
    const context = await this.gatherDebugContext(event.sessionId);
    investigation.context = context;

    // 4. Analyze
    const findings = await this.analyzeBreakpoint(event, context);
    investigation.findings = findings;

    // 5. Post to chat
    await Commands.execute('chat/send', {
      room: 'general',
      message: this.formatDebugFindings(findings),
      metadata: {
        type: 'debug-report',
        breakpointId: event.breakpointId,
        sessionId: event.sessionId
      }
    });

    // 6. Decide action
    const action = await this.decideDebugAction(findings);
    await this.executeDebugAction(event.sessionId, action);
  }

  private async gatherDebugContext(sessionId: UUID): Promise<DebugContext> {
    // Get stack trace
    const stack = await Commands.execute('code/debug/inspect-stack', {
      sessionId
    });

    // Get local variables
    const locals = await Commands.execute('code/debug/get-locals', {
      sessionId
    });

    // Evaluate suspicious expressions
    const evaluations: Record<string, any> = {};
    const expressions = this.generateSuspiciousExpressions(locals);

    for (const expr of expressions) {
      try {
        const result = await Commands.execute('code/debug/evaluate', {
          sessionId,
          expression: expr
        });
        evaluations[expr] = result.value;
      } catch (e) {
        evaluations[expr] = { error: e.message };
      }
    }

    return { stack, locals, evaluations };
  }

  private async analyzeBreakpoint(
    event: BreakpointHitEvent,
    context: DebugContext
  ): Promise<DebugFindings> {
    const findings: DebugFindings = {
      breakpoint: event,
      context,
      issues: [],
      hypotheses: [],
      recommendation: null
    };

    // Check for null/undefined values
    for (const [name, value] of Object.entries(context.locals)) {
      if (value === null || value === undefined) {
        findings.issues.push({
          type: 'null-value',
          severity: 'warning',
          variable: name,
          description: `${name} is ${value}`,
          suggestedFix: `Initialize ${name} before use`
        });
      }
    }

    // Check for array/object issues
    for (const [expr, value] of Object.entries(context.evaluations)) {
      if (expr.endsWith('.length') && value === 0) {
        findings.issues.push({
          type: 'empty-collection',
          severity: 'info',
          variable: expr.replace('.length', ''),
          description: `${expr} is empty`,
          suggestedFix: 'Check if collection should have data'
        });
      }
    }

    // Read source code around breakpoint
    const source = await Commands.execute('code/read', {
      path: event.file,
      startLine: Math.max(1, event.line - 10),
      endLine: event.line + 10,
      requestedBy: this.userId
    });
    findings.sourceContext = source;

    // Generate hypotheses
    if (findings.issues.length > 0) {
      findings.hypotheses.push({
        description: 'Null/undefined value causing issue',
        confidence: 0.7,
        evidence: findings.issues.map(i => i.description)
      });
    }

    // Generate recommendation
    findings.recommendation = this.generateRecommendation(findings);

    return findings;
  }

  private formatDebugFindings(findings: DebugFindings): string {
    const { breakpoint, context, issues, hypotheses, recommendation } = findings;

    let message = `🔍 **Debug Report: ${breakpoint.file}:${breakpoint.line}**\n\n`;

    message += `**Call Stack:**\n`;
    for (let i = 0; i < context.stack.length; i++) {
      const frame = context.stack[i];
      message += `${i + 1}. ${frame.functionName || '(anonymous)'} (${frame.file}:${frame.line})\n`;
    }

    message += `\n**Local Variables:**\n`;
    for (const [name, value] of Object.entries(context.locals)) {
      const valueStr = JSON.stringify(value, null, 2);
      const truncated = valueStr.length > 100 ? valueStr.substring(0, 100) + '...' : valueStr;
      message += `- ${name}: ${truncated}\n`;
    }

    if (Object.keys(context.evaluations).length > 0) {
      message += `\n**Evaluations:**\n`;
      for (const [expr, result] of Object.entries(context.evaluations)) {
        message += `- ${expr} = ${JSON.stringify(result)}\n`;
      }
    }

    if (issues.length > 0) {
      message += `\n**Potential Issues:**\n`;
      for (const issue of issues) {
        const emoji = issue.severity === 'error' ? '❌' : issue.severity === 'warning' ? '⚠️' : 'ℹ️';
        message += `${emoji} ${issue.description}\n`;
        message += `   Suggestion: ${issue.suggestedFix}\n`;
      }
    }

    if (hypotheses.length > 0) {
      message += `\n**Hypotheses:**\n`;
      for (const hyp of hypotheses) {
        message += `💡 ${hyp.description} (confidence: ${(hyp.confidence * 100).toFixed(0)}%)\n`;
      }
    }

    message += `\n**Recommendation:** ${recommendation || 'Continue execution'}`;

    return message;
  }

  private async decideDebugAction(findings: DebugFindings): Promise<DebugAction> {
    // If found clear error, continue (investigation complete)
    if (findings.issues.some(i => i.severity === 'error')) {
      return { type: 'continue', reason: 'Found root cause' };
    }

    // If warnings but not conclusive, step over to see what happens
    if (findings.issues.some(i => i.severity === 'warning')) {
      return { type: 'step-over', reason: 'Investigate warning' };
    }

    // Default: continue
    return { type: 'continue', reason: 'No issues detected' };
  }

  private async executeDebugAction(sessionId: UUID, action: DebugAction): Promise<void> {
    switch (action.type) {
      case 'step-over':
        await Commands.execute('code/debug/step-over', { sessionId });
        break;
      case 'step-into':
        await Commands.execute('code/debug/step-into', { sessionId });
        break;
      case 'continue':
        await Commands.execute('code/debug/continue', { sessionId });
        break;
    }

    // Log action to cognition
    await CognitionLogger.logPlanCompletion(
      this.debugSessions.get(sessionId)!.planId,
      'completed',
      [],
      {
        meetsSuccessCriteria: true,
        criteriaBreakdown: { investigated: true },
        whatWorked: ['Gathered context', 'Analyzed variables', 'Posted findings'],
        mistakes: [],
        improvements: [],
        evaluatedAt: Date.now(),
        duration: Date.now() - this.debugSessions.get(sessionId)!.startedAt,
        stepsExecuted: 3,
        replansRequired: 0
      }
    );

    this.debugSessions.delete(sessionId);
  }
}
```

### 8.2 Collaborative Debugging

Multiple PersonaUsers subscribe to the same events:

```typescript
// All AIs receive breakpoint event
Events.emit('code:breakpoint:hit', { ... });

// Claude investigates stack
// DeepSeek investigates locals
// Grok reads source code
// Teacher AI synthesizes findings

// All post to same chat thread
// Collaborative investigation!
```

---

## 9. Security & Safety

### 9.1 Path Validation

```typescript
// daemons/code-daemon/server/modules/PathValidator.ts

export class PathValidator {
  private repoRoot: string;
  private blockedPaths: Set<string>;
  private blockedPatterns: RegExp[];

  constructor(context: CodeDaemonContext) {
    this.repoRoot = context.repositoryRoot;
    this.blockedPaths = new Set(context.blockedPaths);
    this.blockedPatterns = context.blockedPatterns;
  }

  async validate(targetPath: string): Promise<ValidationResult> {
    // Resolve to absolute path
    const absolute = path.resolve(this.repoRoot, targetPath);

    // Must be within repo bounds
    if (!absolute.startsWith(this.repoRoot)) {
      return {
        valid: false,
        error: `Path '${targetPath}' is outside repository bounds`
      };
    }

    // Check blocked paths
    for (const blocked of this.blockedPaths) {
      if (absolute.includes(blocked)) {
        return {
          valid: false,
          error: `Path contains blocked segment '${blocked}'`
        };
      }
    }

    // Check blocked patterns
    for (const pattern of this.blockedPatterns) {
      if (pattern.test(absolute)) {
        return {
          valid: false,
          error: `Path matches blocked pattern`
        };
      }
    }

    // Check file exists and readable
    try {
      await fs.promises.access(absolute, fs.constants.R_OK);
    } catch (e) {
      return {
        valid: false,
        error: 'Path is not readable or does not exist'
      };
    }

    return { valid: true };
  }
}
```

### 9.2 Debugging Safety

```typescript
// Only allow debugging in non-production
if (process.env.NODE_ENV === 'production') {
  throw new Error('Debugging not allowed in production');
}

// Auto-resume after timeout
setTimeout(() => {
  if (this.isPaused(sessionId)) {
    this.continue({ sessionId });
    console.warn(`⏰ Auto-resumed ${sessionId} after 30s timeout`);
  }
}, 30000);

// Whitelist who can set breakpoints
const DEBUG_WHITELIST = new Set([
  'claude-assistant',
  'teacher-ai',
  'code-review-ai',
  'joel-user-id'
]);

if (!DEBUG_WHITELIST.has(params.requestedBy)) {
  return { success: false, error: 'Not authorized to debug' };
}
```

### 9.3 Rate Limiting

```typescript
const RATE_LIMITS = {
  read: { perMinute: 10, perHour: 200 },
  search: { perMinute: 5, perHour: 100 },
  debug: { perMinute: 3, perHour: 50 }
};
```

---

## 10. Implementation Roadmap

### Phase 1: CodeDaemon Foundation (Week 1)
- [ ] Create `daemons/code-daemon/` structure
- [ ] Implement CodeDaemon singleton
- [ ] Implement CodeDaemonContext
- [ ] Implement PathValidator
- [ ] Implement ToolCache
- [ ] Implement AccessAuditor
- [ ] Write unit tests

### Phase 2: File Operations (Week 1-2)
- [ ] Implement FileReader module
- [ ] Implement `code/read` command
- [ ] Implement `code/list` command
- [ ] Add event emission
- [ ] Write integration tests

### Phase 3: Search Operations (Week 2)
- [ ] Implement CodeSearcher module
- [ ] Implement `code/search` command
- [ ] Add caching for search results
- [ ] Write integration tests

### Phase 4: Git Operations (Week 2-3)
- [ ] Implement GitOperations module
- [ ] Implement `code/diff` command
- [ ] Implement `code/log` command
- [ ] Implement `code/blame` command
- [ ] Write integration tests

### Phase 5: Debugging Foundation (Week 3-4)
- [ ] Implement DebuggerManager module
- [ ] Implement `code/debug/set-breakpoint` command
- [ ] Implement `code/debug/remove-breakpoint` command
- [ ] Add breakpoint event emission
- [ ] Write debugging tests

### Phase 6: Interactive Debugging (Week 4-5)
- [ ] Implement `code/debug/step-over` command
- [ ] Implement `code/debug/step-into` command
- [ ] Implement `code/debug/evaluate` command
- [ ] Implement `code/debug/continue` command
- [ ] Add debug event emission
- [ ] Write E2E debugging tests

### Phase 7: PersonaUser Integration (Week 5-6)
- [ ] Add debug event subscriptions to PersonaUser
- [ ] Implement `handleBreakpointHit()`
- [ ] Implement debug context gathering
- [ ] Implement debug analysis logic
- [ ] Add cognition logging for debug activities
- [ ] Test multi-AI debugging scenarios

### Phase 8: Debug Widget (Week 6-7)
- [ ] Create debug-panel widget
- [ ] Show active breakpoints
- [ ] Show paused state with stack/locals
- [ ] Add step controls
- [ ] Add expression evaluator
- [ ] Test browser ↔ server debugging flow

---

## 11. Testing Strategy

### Unit Tests

```typescript
describe('CodeDaemon', () => {
  describe('FileReader', () => {
    it('should read file contents', async () => {
      const result = await codeDaemon.readFile({
        path: 'test.ts',
        requestedBy: 'test-user'
      });
      expect(result.success).toBe(true);
      expect(result.content).toBeDefined();
    });

    it('should enforce path validation', async () => {
      const result = await codeDaemon.readFile({
        path: '../../../etc/passwd',
        requestedBy: 'test-user'
      });
      expect(result.success).toBe(false);
      expect(result.error).toContain('outside repository');
    });

    it('should emit code:file:read event', async () => {
      const eventPromise = new Promise(resolve => {
        Events.subscribe('code:file:read', resolve);
      });

      await codeDaemon.readFile({
        path: 'test.ts',
        requestedBy: 'test-user'
      });

      const event = await eventPromise;
      expect(event.path).toBe('test.ts');
    });
  });

  describe('DebuggerManager', () => {
    it('should set breakpoint', async () => {
      const result = await codeDaemon.setBreakpoint({
        file: 'test.ts',
        line: 10,
        requestedBy: 'test-user'
      });
      expect(result.success).toBe(true);
      expect(result.breakpoint.id).toBeDefined();
    });

    it('should emit code:breakpoint:hit event when hit', async () => {
      const eventPromise = new Promise(resolve => {
        Events.subscribe('code:breakpoint:hit', resolve);
      });

      await codeDaemon.setBreakpoint({
        file: 'test.ts',
        line: 10,
        requestedBy: 'test-user'
      });

      // Trigger breakpoint
      await executeCodeThatHitsBreakpoint();

      const event = await eventPromise;
      expect(event.line).toBe(10);
      expect(event.sessionId).toBeDefined();
    });
  });
});
```

### Integration Tests

```typescript
describe('PersonaUser Debugging Integration', () => {
  it('should investigate breakpoint automatically', async () => {
    const persona = await createTestPersona('test-ai');

    // Set up chat monitoring
    const chatMessages: ChatMessageEntity[] = [];
    Events.subscribe('chat:message:created', (msg) => {
      chatMessages.push(msg);
    });

    // Set breakpoint
    await Commands.execute('code/debug/set-breakpoint', {
      file: 'test.ts',
      line: 10,
      requestedBy: persona.userId
    });

    // Trigger breakpoint
    await executeCodeThatHitsBreakpoint();

    // Wait for AI to investigate and post
    await wait(5000);

    // Verify AI posted debug report
    const debugReport = chatMessages.find(m =>
      m.authorId === persona.userId &&
      m.metadata?.type === 'debug-report'
    );

    expect(debugReport).toBeDefined();
    expect(debugReport.content).toContain('Debug Report');
    expect(debugReport.content).toContain('Call Stack');
  });
});
```

---

## 12. Performance Considerations

### 12.1 Caching Strategy

```typescript
// Cache frequently read files
const cache = new ToolCache({
  ttl: 5 * 60 * 1000,  // 5 minutes
  maxSize: 100         // 100 files max
});

// Cache search results
const searchCache = new ToolCache({
  ttl: 2 * 60 * 1000,  // 2 minutes (shorter, searches change more)
  maxSize: 50
});
```

### 12.2 Event Throttling

```typescript
// Throttle match events during search
let lastMatchEvent = 0;
const MATCH_EVENT_THROTTLE = 100; // Max 10 events/second

if (Date.now() - lastMatchEvent > MATCH_EVENT_THROTTLE) {
  await Events.emit('code:search:match-found', match);
  lastMatchEvent = Date.now();
}
```

### 12.3 Resource Limits

```typescript
// Max file size
const MAX_FILE_SIZE = 1024 * 1024; // 1MB

// Max search results
const MAX_SEARCH_RESULTS = 100;

// Max paused sessions
const MAX_PAUSED_SESSIONS = 5;
```

---

## Conclusion

CodeDaemon provides a unified, event-driven architecture for all code operations, with **interactive debugging as a first-class feature**. By mirroring DataDaemon's pattern and enabling PersonaUser agents to subscribe to debugging events, we create a foundation for **collaborative AI-driven development** where agents can autonomously investigate, debug, and fix issues.

**Next Steps**:
1. Implement CodeDaemon foundation (Week 1)
2. Build out file/search/git operations (Weeks 2-3)
3. Add debugging capabilities (Weeks 3-5)
4. Integrate with PersonaUser (Weeks 5-6)
5. Create debug widget (Weeks 6-7)
6. **Observe emergent debugging behaviors** (Week 7+)
7. **Write full research paper** (After we have data)

---

**Status**: Ready for Implementation
**Architecture**: Validated
**Blocked By**: None
**Blocker For**: Phase 2B Tool Implementation
