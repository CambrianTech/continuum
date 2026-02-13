# Sentinel Logging & Observability Plan

> **Note**: This document focuses on sentinel-specific logging. For the unified observability architecture covering system logs, cognition logs, and controls, see [OBSERVABILITY-ARCHITECTURE.md](OBSERVABILITY-ARCHITECTURE.md).

## Vision

Sentinels are autonomous agents that do real work (compile, test, deploy, fix). They need **complete observability** - every action, every output, every decision must be:

1. **Captured** - Full logs, not truncated
2. **Accessible** - Via command, event, or UI
3. **Contextual** - Tied to the specific sentinel run
4. **Controllable** - Can dial up/down verbosity

**The ideal**: When a sentinel runs `npm run build`, you can see EVERY line of compilation output in real-time, click on any error to jump to the file, and review the complete log later.

---

## Current State Analysis

### What Exists

| Component | Status | Gap |
|-----------|--------|-----|
| `SentinelExecutionLog` | Streaming events via `sentinel:{handle}:{type}` | Evidence output truncated to ~200 chars |
| `SentinelWorkspace` | Creates `.sentinel-workspaces/{handle}` dirs | No logging infrastructure in workspace |
| `BuildSentinel` | Captures stdout/stderr | Only stores `error.stdout + error.stderr` truncated |
| `Logger` | Per-component routing, Rust backend | No per-sentinel logging support |
| `LogLevelRegistry` | Runtime level control | No sentinel-aware controls |
| `logs/*` commands | list, read, search, config, stats | System logs only, not sentinel logs |

### The Core Problem

Sentinels run compilations that produce **massive output** (thousands of lines). Currently:

```typescript
// BuildSentinel.ts:201-213
try {
  output = execSync(this.config.command, { ... });
} catch (error: any) {
  output = error.stdout?.toString() || '';
  output += error.stderr?.toString() || '';  // This could be HUGE
}

// Later, in SentinelExecutionLog.ts:530
if (action.evidence.output && action.evidence.output.length < 200) {
  // Show inline
} else if (action.evidence.output) {
  const firstLines = action.evidence.output.split('\n').slice(0, 3)...
  // TRUNCATED - full output LOST
}
```

**The output is captured but then truncated.** We need to:
1. Save full output to files
2. Stream it in real-time
3. Reference it from execution logs

---

## Architecture Design

### Per-Sentinel Log Directory

Each sentinel gets a dedicated logging directory inside its workspace:

```
.sentinel-workspaces/
└── {handle}/                     # e.g., abc12345
    ├── .git/                     # Worktree checkout
    ├── logs/                     # NEW: Sentinel-specific logs
    │   ├── sentinel.log          # High-level sentinel actions
    │   ├── build-1.log           # Full output from build attempt 1
    │   ├── build-2.log           # Full output from build attempt 2
    │   ├── build-3.log           # Full output from build attempt 3
    │   ├── llm-requests.log      # LLM queries and responses
    │   └── evidence/             # Structured evidence files
    │       ├── error-parse-1.json
    │       └── fix-applied-2.json
    └── src/...                   # Working directory
```

**Key insight**: Logs live WITH the sentinel's workspace. When the workspace is cleaned up, logs can optionally be archived or deleted.

### Log Streaming Architecture

```
BuildSentinel
    │
    ├── spawn('npm run build')
    │       │
    │       ├── stdout.on('data') ──► SentinelLogWriter.write(handle, 'build-1', chunk)
    │       │                              │
    │       │                              ├── Append to logs/build-1.log
    │       │                              │
    │       │                              └── Events.emit('sentinel:{handle}:stdout', chunk)
    │       │                                      │
    │       │                                      └── UI subscribes for real-time display
    │       │
    │       └── stderr.on('data') ──► (same path)
    │
    └── on('close') ──► SentinelLogWriter.finalize(handle, 'build-1', exitCode)
```

### New Components

#### 1. `SentinelLogWriter` - Per-Sentinel Logging

```typescript
// system/sentinel/SentinelLogWriter.ts

export class SentinelLogWriter {
  private handle: string;
  private workspaceDir: string;
  private logDir: string;
  private streams: Map<string, fs.WriteStream>;
  private eventEmitter?: SentinelEventEmitter;

  constructor(handle: string, workspaceDir: string, eventEmitter?: SentinelEventEmitter) {
    this.handle = handle;
    this.workspaceDir = workspaceDir;
    this.logDir = path.join(workspaceDir, 'logs');
    fs.mkdirSync(this.logDir, { recursive: true });
    this.streams = new Map();
    this.eventEmitter = eventEmitter;
  }

  /**
   * Write a chunk to a named log stream (e.g., 'build-1', 'llm-requests')
   * Streams to file AND emits event for real-time UI
   */
  write(streamName: string, chunk: string | Buffer): void {
    // Get or create stream
    let stream = this.streams.get(streamName);
    if (!stream) {
      const logPath = path.join(this.logDir, `${streamName}.log`);
      stream = fs.createWriteStream(logPath, { flags: 'a' });
      this.streams.set(streamName, stream);
    }

    // Write to file
    stream.write(chunk);

    // Emit for real-time streaming
    if (this.eventEmitter) {
      this.eventEmitter({
        type: 'log',
        handle: this.handle,
        timestamp: new Date().toISOString(),
        payload: {
          stream: streamName,
          chunk: chunk.toString(),
          cumulative: false  // This is a delta, not full content
        }
      });
    }
  }

  /**
   * Get full content of a log stream
   */
  async read(streamName: string): Promise<string | null> {
    const logPath = path.join(this.logDir, `${streamName}.log`);
    if (!fs.existsSync(logPath)) return null;
    return fs.readFileSync(logPath, 'utf-8');
  }

  /**
   * List all log streams for this sentinel
   */
  list(): string[] {
    if (!fs.existsSync(this.logDir)) return [];
    return fs.readdirSync(this.logDir)
      .filter(f => f.endsWith('.log'))
      .map(f => f.replace('.log', ''));
  }

  /**
   * Get log directory path for external access
   */
  get logsPath(): string {
    return this.logDir;
  }

  /**
   * Close all streams
   */
  close(): void {
    for (const stream of this.streams.values()) {
      stream.end();
    }
    this.streams.clear();
  }
}
```

#### 2. Enhanced `SentinelExecutionLog` - Evidence File References

```typescript
// Enhance SentinelAction.evidence to reference full logs
export interface SentinelAction {
  // ... existing fields ...
  evidence?: {
    output?: string;           // Short preview (~200 chars)
    outputFile?: string;       // NEW: Full path to complete output file
    outputStream?: string;     // NEW: Stream name for log retrieval
    // ... other fields ...
  };
}
```

#### 3. New Events for Log Streaming

```typescript
// sentinel:{handle}:log - Real-time log chunks
Events.subscribe('sentinel:abc123:log', (event) => {
  // event.payload = { stream: 'build-1', chunk: '...', cumulative: false }
  appendToUI(event.payload.chunk);
});

// sentinel:{handle}:log-complete - Stream finished
Events.subscribe('sentinel:abc123:log-complete', (event) => {
  // event.payload = { stream: 'build-1', exitCode: 0, totalLines: 1523 }
  showCompletionStatus(event.payload);
});
```

#### 4. New Commands

| Command | Purpose |
|---------|---------|
| `sentinel/logs/list` | List log streams for a sentinel handle |
| `sentinel/logs/read` | Read full content of a log stream |
| `sentinel/logs/tail` | Stream real-time output (subscribe to events) |
| `sentinel/logs/search` | Search within sentinel logs |

---

## Implementation Phases

### Phase 1: Per-Sentinel Log Directory (Foundation)

**Goal**: Every sentinel run creates a dedicated log directory with full output capture.

**Files to modify**:
- `system/sentinel/SentinelWorkspace.ts` - Create logs/ directory on workspace init
- `system/sentinel/BuildSentinel.ts` - Use `spawn()` instead of `execSync()` for streaming
- `system/sentinel/SentinelExecutionLog.ts` - Add `outputFile` to evidence

**Changes**:

```typescript
// SentinelWorkspace.ts - Add logs directory creation
private async initialize(): Promise<void> {
  // ... existing code ...

  // Create logs directory
  const logsDir = path.join(this.info.workingDir, 'logs');
  fs.mkdirSync(logsDir, { recursive: true });
  this.info.logsDir = logsDir;  // NEW field
}
```

```typescript
// BuildSentinel.ts - Use spawn for streaming
private async build(attemptNumber: number): Promise<BuildAttempt> {
  const startTime = Date.now();
  const logWriter = new SentinelLogWriter(this.handle, this.workingDir);
  const streamName = `build-${attemptNumber}`;

  return new Promise((resolve) => {
    const proc = spawn('sh', ['-c', this.config.command], {
      cwd: this.config.workingDir,
      stdio: ['pipe', 'pipe', 'pipe']
    });

    proc.stdout.on('data', (chunk) => {
      logWriter.write(streamName, chunk);
    });

    proc.stderr.on('data', (chunk) => {
      logWriter.write(streamName, chunk);
    });

    proc.on('close', (exitCode) => {
      const fullOutput = await logWriter.read(streamName);
      const errors = exitCode === 0 ? [] : this.parseErrors(fullOutput || '');

      resolve({
        attemptNumber,
        command: this.config.command,
        success: exitCode === 0,
        errors,
        durationMs: Date.now() - startTime,
        // NEW: Reference to full output
        outputFile: path.join(logWriter.logsPath, `${streamName}.log`)
      });
    });
  });
}
```

### Phase 2: Real-Time Event Streaming

**Goal**: UI can subscribe to sentinel log events and display real-time compilation output.

**Files to create/modify**:
- `system/sentinel/SentinelLogWriter.ts` - New class
- `system/sentinel/SentinelExecutionLog.ts` - Add 'log' event type
- `commands/sentinel/run/server/SentinelRunServerCommand.ts` - Wire up events

**New events**:
```
sentinel:{handle}:log          - Real-time log chunks
sentinel:{handle}:log-complete - Stream finished
```

### Phase 3: Log Commands

**Goal**: CLI access to sentinel logs.

**New commands**:
- `commands/sentinel/logs/list/` - List log streams
- `commands/sentinel/logs/read/` - Read full log content
- `commands/sentinel/logs/tail/` - Stream real-time (returns subscription handle)

**Examples**:
```bash
# List logs for a sentinel
./jtag sentinel/logs/list --handle=abc123
# Returns: ["build-1", "build-2", "llm-requests", "sentinel"]

# Read full build output
./jtag sentinel/logs/read --handle=abc123 --stream=build-1
# Returns: Full compilation output (could be thousands of lines)

# Tail in real-time (for running sentinels)
./jtag sentinel/logs/tail --handle=abc123 --stream=build-1
# Streams output until sentinel completes
```

### Phase 4: Chat Integration (Inline Clickable Logs)

**Goal**: When a sentinel is invoked from chat, show inline log references that expand/click to full logs.

**Concept**:
```
┌─────────────────────────────────────────────────────────────┐
│ Helper AI                                              2:34pm │
├─────────────────────────────────────────────────────────────┤
│ Running build sentinel to fix the type errors...            │
│                                                             │
│ ┌─ sentinel:abc123 ─────────────────────────────────────┐  │
│ │ ▸ Build attempt 1/3 [FAILED]     📋 view logs          │  │
│ │   error TS2345: Argument of type 'string' is not...   │  │
│ │ ▸ Applying fix...                                      │  │
│ │ ▸ Build attempt 2/3 [SUCCESS]    📋 view logs          │  │
│ └────────────────────────────────────────────────────────┘  │
│                                                             │
│ Fixed! The build now passes. Ready for review.              │
└─────────────────────────────────────────────────────────────┘
```

**Implementation**:
- Tool calls from PersonaUser emit `tool:result` events
- If result contains `sentinelHandle`, render as expandable log widget
- Clicking "view logs" opens log-viewer widget with sentinel logs

### Phase 5: Verbosity Controls

**Goal**: Ability to dramatically reduce logging workload when not needed.

**Approaches**:

1. **Global sentinel log level**:
```bash
./jtag sentinel/config --logLevel=minimal  # Only errors and summaries
./jtag sentinel/config --logLevel=normal   # Default (streaming, but not persisted in full)
./jtag sentinel/config --logLevel=verbose  # Full capture + persistence
./jtag sentinel/config --logLevel=off      # No logging (fastest)
```

2. **Per-sentinel override**:
```bash
./jtag sentinel/run --type=build --logLevel=verbose  # Override for this run
```

3. **Retention controls**:
```bash
./jtag sentinel/logs/cleanup --olderThan=7d         # Delete logs older than 7 days
./jtag sentinel/logs/cleanup --handle=abc123        # Delete logs for specific run
./jtag sentinel/config --autoCleanup=24h            # Auto-delete after 24 hours
```

4. **Output filtering**:
```bash
./jtag sentinel/run --type=build --logFilter=errors  # Only capture error lines
```

---

## Data Structures

### SentinelLogConfig (New)

```typescript
export interface SentinelLogConfig {
  /** Global log level for sentinel execution */
  level: 'off' | 'minimal' | 'normal' | 'verbose';

  /** How long to retain logs (ms, 0 = forever) */
  retentionMs: number;

  /** Max log file size before rotation (bytes, 0 = unlimited) */
  maxFileSizeBytes: number;

  /** Stream logs via events (for real-time UI) */
  streamEvents: boolean;

  /** Persist full output to files */
  persistToFiles: boolean;

  /** Filter patterns (only log lines matching these) */
  includePatterns?: RegExp[];

  /** Filter patterns (exclude lines matching these) */
  excludePatterns?: RegExp[];
}

// Defaults
const DEFAULT_LOG_CONFIG: SentinelLogConfig = {
  level: 'normal',
  retentionMs: 7 * 24 * 60 * 60 * 1000,  // 7 days
  maxFileSizeBytes: 10 * 1024 * 1024,     // 10MB
  streamEvents: true,
  persistToFiles: true,
};
```

### Enhanced WorkspaceInfo

```typescript
export interface WorkspaceInfo {
  workingDir: string;
  branch: string;
  originalBranch: string;
  isWorktree: boolean;
  worktreePath?: string;
  logsDir: string;          // NEW: Path to logs directory
  logWriter?: SentinelLogWriter;  // NEW: Active log writer
}
```

### Enhanced BuildAttempt

```typescript
export interface BuildAttempt {
  attemptNumber: number;
  command: string;
  success: boolean;
  errors: BuildError[];
  fixApplied?: string;
  durationMs: number;

  // NEW: Log references
  outputFile?: string;       // Path to full output log
  outputStream?: string;     // Stream name for retrieval
  outputLines?: number;      // Total line count
  outputBytes?: number;      // Total byte size
}
```

---

## Event Specifications

### sentinel:{handle}:log

Emitted for each chunk of output from sentinel processes.

```typescript
{
  type: 'log',
  handle: 'abc123',
  timestamp: '2024-02-09T10:30:15.123Z',
  payload: {
    stream: 'build-1',           // Which log stream
    chunk: 'Compiling...\n',     // The actual content
    cumulative: false,           // This is a delta
    sourceType: 'stdout' | 'stderr',
    lineNumber?: number,         // Starting line number of this chunk
  }
}
```

### sentinel:{handle}:log-complete

Emitted when a log stream is finalized.

```typescript
{
  type: 'log-complete',
  handle: 'abc123',
  timestamp: '2024-02-09T10:30:45.456Z',
  payload: {
    stream: 'build-1',
    exitCode: 1,
    totalLines: 1523,
    totalBytes: 89234,
    durationMs: 30333,
    logPath: '/path/to/logs/build-1.log'
  }
}
```

---

## Integration Points

### 1. PersonaUser Tool Execution

When a persona calls a tool that spawns a sentinel:

```typescript
// PersonaToolExecutor.ts
async executeTool(tool: string, params: any): Promise<ToolResult> {
  if (tool === 'sentinel/run') {
    // Add event listener for real-time updates
    const handle = params.handle || generateHandle();
    const unsubscribe = await subscribeSentinelEvents(handle, (event) => {
      if (event.type === 'log') {
        // Forward to chat as streaming message update
        this.emitToolProgress(tool, event);
      }
    });

    try {
      const result = await Commands.execute(tool, { ...params, handle });
      return result;
    } finally {
      unsubscribe();
    }
  }
  // ... other tools
}
```

### 2. Chat Widget Log Display

New component for inline log display:

```typescript
// widgets/sentinel-log-inline/SentinelLogInlineWidget.ts
export class SentinelLogInlineWidget extends LitElement {
  @property() handle: string;
  @property() stream: string;
  @property() collapsed: boolean = true;
  @property() lines: string[] = [];

  private unsubscribe?: () => void;

  connectedCallback() {
    super.connectedCallback();
    // Subscribe to real-time log events
    this.subscribeToLogs();
  }

  private async subscribeToLogs() {
    this.unsubscribe = await Events.subscribe(
      `sentinel:${this.handle}:log`,
      (event) => {
        if (event.payload.stream === this.stream) {
          this.lines = [...this.lines, ...event.payload.chunk.split('\n')];
          this.requestUpdate();
        }
      }
    );
  }

  render() {
    return html`
      <div class="sentinel-log-inline">
        <div class="header" @click=${this.toggleCollapse}>
          <span class="icon">${this.collapsed ? '▸' : '▾'}</span>
          <span class="stream-name">${this.stream}</span>
          <span class="line-count">${this.lines.length} lines</span>
          <button @click=${this.openFullLog}>📋 view full</button>
        </div>
        ${!this.collapsed ? html`
          <pre class="log-content">${this.lines.slice(-50).join('\n')}</pre>
        ` : ''}
      </div>
    `;
  }
}
```

### 3. Log Viewer Widget Enhancement

Enhance existing log-viewer to support sentinel logs:

```typescript
// widgets/log-viewer/LogViewerWidget.ts
// Add mode for sentinel logs
@property() mode: 'system' | 'sentinel' = 'system';
@property() sentinelHandle?: string;
@property() sentinelStream?: string;

// When mode='sentinel', load from sentinel workspace
private async loadSentinelLogs() {
  const result = await Commands.execute('sentinel/logs/read', {
    handle: this.sentinelHandle,
    stream: this.sentinelStream
  });
  this.content = result.content;
}
```

---

## Testing Strategy

### Unit Tests

```typescript
// tests/unit/SentinelLogWriter.test.ts
describe('SentinelLogWriter', () => {
  it('creates log directory on construction');
  it('writes chunks to named streams');
  it('emits events for real-time streaming');
  it('reads full content from streams');
  it('lists all log streams');
  it('handles concurrent writes');
});
```

### Integration Tests

```typescript
// tests/integration/sentinel-logging.test.ts
describe('Sentinel Logging Integration', () => {
  it('captures full build output to file');
  it('streams output via events in real-time');
  it('execution log references output files');
  it('logs survive sentinel completion');
  it('logs are cleaned up with workspace');
});
```

### E2E Tests

```bash
# Run a build sentinel and verify logs
./jtag sentinel/run --type=build --command="npm run build:ts" --async=false

# Check logs were created
./jtag sentinel/logs/list --handle=$HANDLE
# Expected: ["build-1", "sentinel"]

# Read full output
./jtag sentinel/logs/read --handle=$HANDLE --stream=build-1 | wc -l
# Expected: Thousands of lines (not truncated)
```

---

## Migration Path

1. **Phase 1**: Implement SentinelLogWriter + workspace logging (no breaking changes)
2. **Phase 2**: Add event streaming (additive, no breaking changes)
3. **Phase 3**: Add log commands (additive)
4. **Phase 4**: Chat integration (UI enhancement)
5. **Phase 5**: Verbosity controls (configuration)

Each phase is independently deployable and testable.

---

## Open Questions

1. **Log retention**: How long should sentinel logs be kept by default?
   - Proposal: 7 days, configurable via `sentinel/config`

2. **Log size limits**: Should we cap individual log files?
   - Proposal: 10MB per stream, rotate if exceeded

3. **Archive integration**: Should completed sentinel logs be archived?
   - Proposal: Optional archive command, not automatic

4. **Persona log separation**: Should each persona's sentinel runs have separate log directories?
   - Proposal: Yes, under persona home dir: `~/.continuum/personas/{id}/sentinel-logs/`

---

## Summary

This plan enables complete observability for sentinel execution:

- **Full capture**: No more truncated build output
- **Real-time streaming**: UI shows compilation as it happens
- **Accessible**: CLI commands + event subscriptions
- **Contextual**: Logs tied to specific sentinel runs
- **Controllable**: Verbosity and retention settings

The architecture builds on existing infrastructure (SentinelExecutionLog, SentinelWorkspace, Logger) while adding the missing pieces for complete observability.
