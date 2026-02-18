# Logging Modules Architecture

## Overview

Modular logging system with clean separation of concerns. Each module has ONE responsibility.

**Design Principles:**
- **Single Responsibility** - Each class does one thing well
- **Dependency Injection** - Modules receive dependencies via constructor
- **No Singletons** (except Logger.ts core) - Testable, composable modules
- **Async/Streaming** - Handle large files without loading into memory
- **Type-safe** - Full TypeScript interfaces

---

## Module Hierarchy

```
Logger.ts (Core - Singleton)
  ├── ComponentLogger (Instance per component)
  └── LogFileRegistry (Discovery)
      └── LogMetadata[]

LogReader (Line-by-line reading)
  └── ReadResult { lines, hasMore }

LogSearcher (Pattern matching)
  └── SearchResult { matches, context }

LogIterator (Handle-based streaming)
  └── IteratorHandle { id, position, fileStream }

LogQueryEngine (High-level queries)
  └── Uses: LogFileRegistry, LogReader, LogSearcher
```

**Data Flow:**
1. LogFileRegistry discovers all log files
2. LogReader reads specific line ranges
3. LogSearcher finds patterns with context
4. LogIterator maintains state for large file processing
5. LogQueryEngine orchestrates all modules for complex queries

---

## Module 1: LogFileRegistry

**Purpose:** Dynamic log file discovery and metadata

**Location:** `system/core/logging/LogFileRegistry.ts`

**Responsibilities:**
- Discover all Logger.ts-managed files
- Add external logs (npm-start.log, etc.)
- Provide metadata (size, modified time, line count estimate)
- Group by category (system, persona, session)

**Interface:**
```typescript
export interface LogMetadata {
  filePath: string;
  category: 'system' | 'persona' | 'session' | 'external';
  component?: string;         // e.g., 'AIProviderDaemon'
  personaId?: string;         // For persona logs
  personaName?: string;       // e.g., 'Helper AI'
  logType?: string;           // e.g., 'adapters', 'cognition', 'tools'
  sizeBytes: number;
  lastModified: Date;
  lineCountEstimate: number;  // Rough estimate (bytes / 100)
  isActive: boolean;          // Currently being written to
}

export interface LogFileRegistry {
  /**
   * Discover all log files (scans filesystem)
   */
  discover(): Promise<LogMetadata[]>;

  /**
   * Get metadata for specific log file
   */
  getMetadata(filePath: string): Promise<LogMetadata | null>;

  /**
   * Filter logs by criteria
   */
  filter(criteria: {
    category?: string;
    component?: string;
    personaId?: string;
    logType?: string;
    minSize?: number;
    maxSize?: number;
    modifiedAfter?: Date;
  }): Promise<LogMetadata[]>;

  /**
   * Register external log (e.g., npm-start.log)
   */
  registerExternal(filePath: string, component: string): void;
}
```

**Implementation Notes:**
- Scans `.continuum/jtag/logs/system/` for system logs
- Scans `.continuum/personas/*/logs/` for persona logs
- Scans `.continuum/sessions/*/logs/` for session logs
- Uses fs.stat for size/modified time
- Line count estimate = fileSize / 100 (average 100 bytes per line)
- Cache results for 5 seconds (avoid repeated filesystem scans)

---

## Module 2: LogReader

**Purpose:** Read specific line ranges from log files

**Location:** `system/core/logging/LogReader.ts`

**Responsibilities:**
- Read lines N-M from any log file
- Stream large files (don't load entire file)
- Handle file encoding (UTF-8)
- Provide line numbers with content

**Interface:**
```typescript
export interface ReadResult {
  lines: LogLine[];
  totalLines: number;
  hasMore: boolean;
  nextOffset: number;
}

export interface LogLine {
  lineNumber: number;
  content: string;
  timestamp?: Date;    // Parsed from log line if available
  level?: string;      // e.g., 'INFO', 'ERROR'
  component?: string;  // e.g., 'AIProviderDaemon'
}

export interface LogReader {
  /**
   * Read specific line range
   * @param startLine - 1-indexed line number
   * @param endLine - 1-indexed line number (inclusive)
   */
  read(filePath: string, startLine: number, endLine: number): Promise<ReadResult>;

  /**
   * Read last N lines (tail)
   */
  tail(filePath: string, lineCount: number): Promise<ReadResult>;

  /**
   * Count total lines in file (async, uses streaming)
   */
  countLines(filePath: string): Promise<number>;

  /**
   * Parse log line into structured format
   */
  parseLine(line: string, lineNumber: number): LogLine;
}
```

**Implementation Notes:**
- Use Node.js readline for streaming line-by-line
- For tail(), seek to end and read backwards (optimize for recent logs)
- Parse timestamps using regex: `/^\[(\d{4}-\d{2}-\d{2}T[\d:.]+Z)\]/`
- Parse log level using regex: `/\[(DEBUG|INFO|WARN|ERROR)\]/`
- Parse component using regex: `/^\[.*?\]\s*\[.*?\]\s*(\w+):/`
- Cache line counts for 60 seconds (expensive operation)

---

## Module 3: LogSearcher

**Purpose:** Pattern matching across log files with context

**Location:** `system/core/logging/LogSearcher.ts`

**Responsibilities:**
- Search for regex patterns
- Return matching lines with context (N lines before/after)
- Support multi-file search
- Highlight matches

**Interface:**
```typescript
export interface SearchResult {
  matches: SearchMatch[];
  totalMatches: number;
  filesSearched: number;
  durationMs: number;
}

export interface SearchMatch {
  filePath: string;
  lineNumber: number;
  line: LogLine;
  context: {
    before: LogLine[];
    after: LogLine[];
  };
  highlightedContent: string;  // With ANSI color codes
}

export interface LogSearcher {
  /**
   * Search single file
   */
  search(filePath: string, pattern: string | RegExp, options?: SearchOptions): Promise<SearchResult>;

  /**
   * Search multiple files
   */
  searchMultiple(filePaths: string[], pattern: string | RegExp, options?: SearchOptions): Promise<SearchResult>;

  /**
   * Search all files matching criteria
   */
  searchByCriteria(
    criteria: { category?: string; component?: string; personaId?: string },
    pattern: string | RegExp,
    options?: SearchOptions
  ): Promise<SearchResult>;
}

export interface SearchOptions {
  contextLines?: number;      // Lines before/after match (default: 2)
  caseSensitive?: boolean;    // Default: false
  maxMatches?: number;        // Stop after N matches (default: 100)
  logLevel?: string;          // Only match lines with this level
  component?: string;         // Only match lines from this component
  after?: Date;               // Only match lines after this timestamp
  before?: Date;              // Only match lines before this timestamp
}
```

**Implementation Notes:**
- Use streaming search (don't load entire file)
- Stop early if maxMatches reached
- Use chalk for syntax highlighting in terminal
- Cache context lines (if searching same file multiple times)
- Support both string patterns (convert to regex) and RegExp objects

---

## Module 4: LogIterator

**Purpose:** Handle-based streaming for large file processing

**Location:** `system/core/logging/LogIterator.ts`

**Responsibilities:**
- Maintain file position across multiple reads
- Provide "open, read, read, close" pattern
- Track active handles (prevent leaks)
- Auto-close on timeout

**Interface:**
```typescript
export interface IteratorHandle {
  id: string;              // UUID for handle
  filePath: string;
  position: number;        // Current byte offset
  lineNumber: number;      // Current line number
  createdAt: Date;
  lastAccessedAt: Date;
  expiresAt: Date;         // Auto-close after 5 minutes idle
  isEOF: boolean;
}

export interface LogIterator {
  /**
   * Open file for iterative reading
   */
  open(filePath: string): Promise<IteratorHandle>;

  /**
   * Read next N lines from handle
   */
  read(handleId: string, lineCount: number): Promise<ReadResult>;

  /**
   * Seek to specific line
   */
  seek(handleId: string, lineNumber: number): Promise<void>;

  /**
   * Close handle and free resources
   */
  close(handleId: string): Promise<void>;

  /**
   * Get handle metadata
   */
  getHandle(handleId: string): IteratorHandle | null;

  /**
   * List all active handles (for debugging)
   */
  listHandles(): IteratorHandle[];

  /**
   * Close expired handles (called periodically)
   */
  cleanupExpired(): Promise<number>;
}
```

**Implementation Notes:**
- Store active handles in Map<string, IteratorHandle>
- Use fs.createReadStream with start/end byte offsets
- Update lastAccessedAt on every read()
- Auto-cleanup expired handles every 60 seconds (setInterval)
- Expire handles after 5 minutes idle (prevent leaks)
- Max 100 concurrent handles (throw error if exceeded)

---

## Module 5: LogQueryEngine

**Purpose:** High-level query orchestration

**Location:** `system/core/logging/LogQueryEngine.ts`

**Responsibilities:**
- Compose LogFileRegistry, LogReader, LogSearcher for complex queries
- Implement common query patterns (e.g., "show me all errors in last hour")
- Provide simple API for commands to use

**Interface:**
```typescript
export interface QueryResult {
  logs: LogMetadata[];      // Matching log files
  lines: LogLine[];         // Matching lines
  matches: SearchMatch[];   // Search matches with context
  summary: QuerySummary;
}

export interface QuerySummary {
  totalFiles: number;
  totalLines: number;
  totalMatches: number;
  durationMs: number;
}

export interface LogQueryEngine {
  /**
   * List all available logs
   */
  listLogs(filter?: {
    category?: string;
    component?: string;
    personaId?: string;
  }): Promise<LogMetadata[]>;

  /**
   * Read from specific log
   */
  readLog(filePath: string, startLine?: number, endLine?: number): Promise<ReadResult>;

  /**
   * Tail log (last N lines)
   */
  tailLog(filePath: string, lineCount?: number): Promise<ReadResult>;

  /**
   * Search logs
   */
  searchLogs(query: LogQuery): Promise<QueryResult>;

  /**
   * Get summary statistics
   */
  getStats(): Promise<LogStats>;
}

export interface LogQuery {
  pattern: string | RegExp;
  files?: string[];          // Specific files (or all if omitted)
  category?: string;         // Filter by category
  component?: string;        // Filter by component
  personaId?: string;        // Filter by persona
  logLevel?: string;         // Filter by log level
  after?: Date;              // Only lines after timestamp
  before?: Date;             // Only lines before timestamp
  contextLines?: number;     // Context around matches
  maxMatches?: number;       // Stop after N matches
}

export interface LogStats {
  totalFiles: number;
  totalSizeMB: number;
  byCategory: Record<string, number>;
  byComponent: Record<string, number>;
  largestFiles: Array<{ filePath: string; sizeMB: number }>;
  oldestFiles: Array<{ filePath: string; age: string }>;
}
```

**Implementation Notes:**
- Inject dependencies: LogFileRegistry, LogReader, LogSearcher, LogIterator
- Use LogFileRegistry.filter() to narrow down files before searching
- Use LogSearcher for pattern matching
- Use LogReader for simple reads/tails
- Cache query results for 30 seconds (avoid repeated scans)

---

## Module 6: Logger.ts Optimizations

**Purpose:** Fix bottlenecks in existing Logger.ts

**Location:** `system/core/logging/Logger.ts`

**Changes:**

### Optimization 1: Parallel Shutdown Flush
```typescript
// Current (SLOW - serial):
shutdown(): void {
  for (const logFile of this.logQueues.keys()) {
    this.flushQueue(logFile);  // Serial
  }
  for (const stream of this.fileStreams.values()) {
    stream.end();
  }
}

// Optimized (FAST - parallel):
async shutdown(): Promise<void> {
  // Flush all queues in parallel
  const flushPromises = Array.from(this.logQueues.keys()).map(logFile =>
    Promise.resolve(this.flushQueue(logFile))
  );
  await Promise.all(flushPromises);

  // Close all streams in parallel
  const closePromises = Array.from(this.fileStreams.values()).map(stream =>
    new Promise<void>((resolve) => {
      stream.end(() => resolve());
    })
  );
  await Promise.all(closePromises);
}
```

### Optimization 2: File Descriptor Monitoring
```typescript
private fileDescriptorWarningThreshold = 200;  // Warn at 200 open files

private checkFileDescriptorLimit(): void {
  const openFiles = this.fileStreams.size;
  if (openFiles >= this.fileDescriptorWarningThreshold) {
    console.warn(`⚠️ [Logger] ${openFiles} open log files (approaching system limit)`);
  }
}

// Call in createWithFile() and getFileStream()
```

### Optimization 3: Lazy Stream Creation
```typescript
// Don't open file streams until first write
private getOrCreateStream(logFilePath: string): fs.WriteStream {
  if (this.fileStreams.has(logFilePath)) {
    return this.fileStreams.get(logFilePath)!;
  }
  // Create on first write (not on logger creation)
  return this.createStream(logFilePath);
}
```

### Optimization 4: Stream Pooling (Future)
```typescript
// For >500 log files, implement stream pooling:
// - Keep 100 most-recently-used streams open
// - Close LRU streams when pool full
// - Reopen on demand
private maxOpenStreams = 100;
private streamLRU: Map<string, number> = new Map();  // filePath -> lastAccessTime
```

---

## Module Dependencies

```
LogQueryEngine
  ├── LogFileRegistry (discovers files)
  ├── LogReader (reads line ranges)
  ├── LogSearcher (pattern matching)
  └── LogIterator (large file streaming)

Commands (logs/query, logs/tail, logs/search)
  └── LogQueryEngine (single entry point)

BaseUser.state (persistence layer)
  ├── Log bookmarks (current position)
  ├── Debug flags (verbosity, filters)
  └── Search history
```

**Dependency Injection Pattern:**
```typescript
// In AIProviderDaemon initialization:
const registry = new LogFileRegistry();
const reader = new LogReader();
const searcher = new LogSearcher(reader);  // Inject reader
const iterator = new LogIterator(reader);  // Inject reader
const queryEngine = new LogQueryEngine(registry, reader, searcher, iterator);

// Make queryEngine available to commands
Commands.register('logs/query', new LogsQueryCommand(queryEngine));
```

---

## Testing Strategy

### Unit Tests (Isolated Modules)
```bash
npx vitest system/core/logging/LogFileRegistry.test.ts
npx vitest system/core/logging/LogReader.test.ts
npx vitest system/core/logging/LogSearcher.test.ts
npx vitest system/core/logging/LogIterator.test.ts
```

**Test fixtures:**
- Create temp log files with known content
- Test edge cases (empty files, huge files, binary data)
- Mock filesystem for fast tests

### Integration Tests (Module Interactions)
```bash
npx vitest system/core/logging/LogQueryEngine.test.ts
```

**Test scenarios:**
- Query across multiple personas
- Search with context
- Iterator handle lifecycle
- Large file streaming (1GB+ logs)

### System Tests (End-to-End)
```bash
npm start
./jtag logs/list
./jtag logs/query --pattern="ERROR" --component="AIProviderDaemon"
./jtag logs/tail --file="helper/logs/cognition.log" --lines=50
```

---

## Implementation Order

### Phase 1: Core Modules (No Commands Yet)
1. ✅ Document architecture (this file)
2. ⬜ Implement LogFileRegistry
3. ⬜ Implement LogReader
4. ⬜ Implement LogSearcher
5. ⬜ Implement LogIterator
6. ⬜ Implement LogQueryEngine
7. ⬜ Unit tests for all modules
8. ⬜ Integration tests

### Phase 2: Logger.ts Optimizations
1. ⬜ Parallel shutdown flush
2. ⬜ File descriptor monitoring
3. ⬜ Lazy stream creation
4. ⬜ Test under load (254 files)

### Phase 3: Commands (After Modules Solid)
1. ⬜ Design command schemas (logs/query, logs/tail, logs/search, etc.)
2. ⬜ Implement shared command logic
3. ⬜ Implement browser/server handlers
4. ⬜ Test with personas

### Phase 4: State Integration
1. ⬜ Design state key naming convention (e.g., `log:bookmark:{filePath}`)
2. ⬜ Add bookmark get/set helpers
3. ⬜ Integrate with LogIterator (auto-resume from bookmark)
4. ⬜ Test persistence across sessions

---

## File Structure

```
system/core/logging/
├── Logger.ts                   # Core (existing)
├── LogFileRegistry.ts          # NEW - Discovery
├── LogReader.ts                # NEW - Line reading
├── LogSearcher.ts              # NEW - Pattern matching
├── LogIterator.ts              # NEW - Streaming handles
├── LogQueryEngine.ts           # NEW - High-level queries
├── tests/
│   ├── LogFileRegistry.test.ts
│   ├── LogReader.test.ts
│   ├── LogSearcher.test.ts
│   ├── LogIterator.test.ts
│   └── LogQueryEngine.test.ts
└── fixtures/                   # Test data
    ├── sample-system.log
    ├── sample-persona.log
    └── large-file.log (generated)

commands/logs/
├── shared/
│   ├── LogsTypes.ts            # Command schemas
│   └── LogsShared.ts           # Shared logic
├── browser/
│   └── LogsBrowser.ts          # Browser handlers
└── server/
    ├── LogsQueryServerCommand.ts
    ├── LogsTailServerCommand.ts
    ├── LogsSearchServerCommand.ts
    ├── LogsListServerCommand.ts
    └── LogsIteratorServerCommand.ts
```

---

## Success Metrics

**Before optimization:**
- 254 open file descriptors (at limit)
- Shutdown takes ~5-10 seconds (serial flush)
- No programmatic log access (only tail -f)

**After Phase 1 (Modules):**
- Programmatic log access via LogQueryEngine
- Search across all logs in <1 second
- Stream 1GB logs without memory pressure

**After Phase 2 (Optimizations):**
- Shutdown takes <500ms (parallel flush)
- File descriptor monitoring warns before hitting limit
- Lazy stream creation reduces idle memory

**After Phase 3 (Commands):**
- Personas can query logs via `logs/query`
- Bookmark support via BaseUser.state
- Iterator handles for large log processing

---

## Future Enhancements (Phase 5+)

1. **Log Rotation & Compression**
   - Automatic .gz compression after 1 hour
   - Delete logs older than 7 days
   - Background worker for compression

2. **Structured Logging (SQLite)**
   - Migrate from plain text to SQLite
   - Single database file for all logs
   - Fast indexed queries
   - Retention policies via SQL

3. **Real-time Log Streaming**
   - WebSocket-based log tailing
   - Subscribe to log events
   - Filter by level/component

4. **Log Dashboard UI**
   - Web-based log viewer
   - Search with syntax highlighting
   - Filter by multiple criteria
   - Export to file

---

**Document Version:** 1.0 (December 2025)
**Status:** Design phase - not yet implemented
**Next Step:** Implement LogFileRegistry module
