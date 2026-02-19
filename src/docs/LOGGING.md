# Logging Architecture

## Overview

The Continuum logging system handles **254+ concurrent log files** across system, persona, and session contexts using a centralized `Logger.ts` with async write queues.

**Key Statistics:**
- **System logs**: 7 files (4MB) - Shared infrastructure
- **Persona logs**: 127 files (60MB) - 11 personas × 12 logs each
- **Session logs**: 117 files (7.2GB) - Historical validation runs

---

## Architecture

### Logger.ts Core (`system/core/logging/Logger.ts`)

**Design Principles:**
- **Centralized routing**: All logs route through Logger.ts
- **Async write queues**: Non-blocking writes with in-memory queues per file
- **File mode control**: CLEAN (truncate), APPEND, ARCHIVE
- **Environment-aware**: Respects LOG_TO_CONSOLE=0 config
- **Type-safe**: ComponentLogger interface for structured logging

**Key Methods:**
```typescript
Logger.createWithFile(component: string, logFilePath: string, mode: FileMode): ComponentLogger
Logger.createForPersona(persona: PersonaUser, logCategory: string): ComponentLogger
```

**Write Queue Implementation:**
- Each log file gets its own write queue
- Writes are serialized per file (parallel across files)
- Async flush on process exit
- Conservative error handling (never crash on log failure)

---

## Log Hierarchy

### 1. System Logs (`.continuum/jtag/logs/system/`)

**Shared infrastructure logs** - used by all daemons and system modules.

| File | Component | Purpose |
|------|-----------|---------|
| `system.log` | Global | System-wide events (startup, shutdown, errors) |
| `sql.log` | DataDaemon | All SQL queries and database operations |
| `adapters.log` | AIProviderDaemon | Adapter initialization, model selection |
| `tools.log` | PersonaToolExecutor | System-level tool execution |
| `coordination.log` | BaseCoordinationStream | Temperature tracking, coordination state |
| `coordination-decisions.log` | CoordinationDecisionLogger | Coordination decision records |
| `ai-decisions-{sessionId}.log` | AIDecisionLogger | AI decision training data |

**Usage Pattern:**
```typescript
import { Logger, FileMode } from '@system/core/logging/Logger';
import { SystemPaths } from '@system/core/config/SystemPaths';
import * as path from 'path';

const logger = Logger.createWithFile(
  'MyComponent',
  path.join(SystemPaths.logs.system, 'myfile.log'),
  FileMode.CLEAN
);

logger.info('Starting component');
logger.error('Something failed', error);
```

---

### 2. Persona Logs (`.continuum/personas/{uniqueId}/logs/`)

**Per-persona logging** - each PersonaUser gets 12 isolated log files.

#### Standard Log Categories

| Category | File | Component | Purpose |
|----------|------|-----------|---------|
| **adapters** | `adapters.log` | BaseAIProviderAdapter | AI provider requests, responses, errors |
| **cns** | `cns.log` | PersonaCentralNervousSystem | Central nervous system coordination |
| **cognition** | `cognition.log` | PersonaCognition | Decision-making, thought streams |
| **genome** | `genome.log` | PersonaGenome | LoRA adapter paging, eviction |
| **hippocampus** | `hippocampus.log` | Hippocampus | Memory storage, retrieval, consolidation |
| **limbic** | `limbic.log` | LimbicSystem | Memory, learning, genome operations |
| **motor-cortex** | `motor-cortex.log` | MotorCortex | Action execution, response generation |
| **personalogger** | `personalogger.log` | PersonaLogger | Persona lifecycle (boot, shutdown) |
| **prefrontal** | `prefrontal.log` | PrefrontalCortex | High-level thoughts, planning, cognition |
| **tools** | `tools.log` | PersonaToolExecutor | Tool execution (per-persona) |
| **training** | `training.log` | TrainingDataAccumulator | Fine-tuning data accumulation |
| **user** | `user.log` | PersonaUser | User interaction events |

#### Usage Pattern

```typescript
// In PersonaUser or any persona module:
const logger = Logger.createForPersona(this, 'cognition');
logger.info('Processing thought stream');
logger.debug('Temperature:', temperature);
```

**Path Resolution:**
- `Logger.createForPersona()` automatically constructs path: `.continuum/personas/{uniqueId}/logs/{category}.log`
- Always uses FileMode.CLEAN (truncate on session start)
- Each persona's logs are isolated (no cross-contamination)

---

### 3. Session Logs (`.continuum/sessions/`)

**Historical validation runs** - created during integration tests.

**Structure:**
```
.continuum/sessions/validation/run_{timestamp}/logs/
├── server-console-log.log
├── server-console-error.log
├── server-console-warn.log
├── server-console-debug.log
└── browser-console-log.log
```

**Note:** These accumulate over time (7.2GB currently). Consider periodic cleanup script.

---

## File Modes

Logger.ts supports three file modes via `FileMode` enum:

### CLEAN Mode (Default for Most Logs)
```typescript
FileMode.CLEAN
```
- **Behavior**: Truncates file on session start
- **Use case**: Fresh logs every deployment (most system + persona logs)
- **Pros**: Easy to read, no clutter from old sessions
- **Cons**: Loses historical data

### APPEND Mode
```typescript
FileMode.APPEND
```
- **Behavior**: Appends to existing file
- **Use case**: Long-running accumulation (training data, session logs)
- **Pros**: Historical continuity
- **Cons**: Files grow indefinitely

### ARCHIVE Mode
```typescript
FileMode.ARCHIVE
```
- **Behavior**: Moves old file to `{filename}.{timestamp}.log`, starts fresh
- **Use case**: Keep history but avoid unbounded growth
- **Pros**: Historical backups + fresh session logs
- **Cons**: Disk usage grows (need cleanup strategy)

---

## Performance Considerations

### Current Implementation (Single-Process)

**Strengths:**
- Simple architecture - one process, one Logger.ts instance
- Async write queues prevent blocking on I/O
- Per-file streams are isolated (parallel writes across files)

**Bottlenecks:**
1. **254+ open file descriptors** - OS limits apply
2. **Memory pressure** - Each queue holds buffered writes in RAM
3. **Flush latency** - Process exit must flush all queues serially
4. **No compression** - Log files stored as plain text
5. **No rotation** - CLEAN mode loses history, APPEND mode grows unbounded

### Multi-Process Optimization Strategy

**Option 1: Worker Thread Pool**
```
Main Process
├── Logger.ts (coordinator)
└── Worker Pool (4 threads)
    ├── Worker 1: System logs
    ├── Worker 2: Persona logs (helper, teacher, codereview)
    ├── Worker 3: Persona logs (deepseek, groq, gpt)
    └── Worker 4: Session logs
```

**Benefits:**
- Parallel writes across CPU cores
- Isolates file descriptor limits per worker
- Main thread stays responsive

**Drawbacks:**
- Message passing overhead (serialize log entries)
- Complex failure handling (what if worker crashes?)
- TypeScript worker_threads API complexity

---

**Option 2: Dedicated Log Daemon (Message Queue)**
```
                ┌──────────────┐
                │ Log Daemon   │
                │ (Standalone) │
                └──────────────┘
                       ↑
                  (Unix Socket / IPC)
                       ↓
    ┌──────────────────┼──────────────────┐
    ↓                  ↓                   ↓
[Main Process]  [Persona Process]  [Test Process]
    │                  │                   │
Logger.ts → Queue → Daemon Writes to Disk
```

**Benefits:**
- Complete isolation from main process
- Can restart log daemon without restarting server
- Centralized log rotation/compression/archival
- Horizontal scaling (shard personas across log daemon instances)

**Drawbacks:**
- IPC overhead (Unix sockets slower than in-memory queues)
- Daemon lifecycle management (who starts/stops it?)
- Added complexity for debugging (logs are now async with daemon)

---

**Option 3: Structured Logging + SQLite**
```
All logs → Logger.ts → SQLite DB (single file)
                       ↓
            [Indexed queries, retention policies]
```

**Benefits:**
- Single file instead of 254 files
- Fast indexed queries (filter by persona, component, timestamp)
- Atomic transaction safety
- Built-in compression (SQLite VACUUM)
- Retention policies via SQL (DELETE old logs)

**Drawbacks:**
- SQLite write throughput limits (single writer lock)
- Requires schema design (log_entries table)
- Harder to tail -f for live debugging
- Adds dependency on SQLite (already used for data, but logs are separate concern)

---

**Option 4: Hybrid (Short-term + Long-term)**
```
Logger.ts (in-memory queue) → Write to .log files (short-term, last 1 hour)
                            ↓
            Background archival → Compress + move to .log.gz (long-term)
```

**Benefits:**
- Fast writes to active logs (current behavior)
- Automatic compression of old logs (free disk space)
- Keeps recent logs readable as plain text
- Simple retention policy (keep last N hours uncompressed)

**Drawbacks:**
- Still have 254 open file descriptors
- Archival job adds system load
- Need compression library (gzip)

---

## Recommendations

### Phase 1: Immediate Optimizations (No Architecture Change)
1. **Reduce log file count** - Merge rarely-used categories (e.g., soul.log + body.log → state.log)
2. **Implement log levels** - INFO, DEBUG, WARN, ERROR with runtime filtering
3. **Add file descriptor monitoring** - Warn when approaching OS limits (ulimit -n)
4. **Implement flush metrics** - Measure flush time on exit, identify slow files

### Phase 2: Compression + Rotation (Hybrid Approach)
1. **Add ARCHIVE mode logic** - Rotate old logs to .gz every 1 hour
2. **Retention policies** - Delete logs older than 7 days (configurable)
3. **Background worker** - Node.js worker thread for compression (don't block main)

### Phase 3: Structured Logging (If Needed)
1. **Migrate to SQLite log store** - Single database file for all logs
2. **Build log query CLI** - `./jtag logs/query --persona=helper --component=cognition --since=1h`
3. **Add log dashboard** - Web UI for log exploration (future)

---

## Current Status (December 2025)

### ✅ Completed
- Centralized routing through Logger.ts
- Per-persona log isolation
- System log organization
- Async write queues
- CLEAN mode (truncate on start)
- LOG_TO_CONSOLE=0 config respect

### 🚧 In Progress
- Performance analysis (this document)
- Multi-process optimization design

### 📋 Planned
- Log compression + rotation
- Structured logging (SQLite)
- Log query CLI commands
- Retention policies
- Dashboard UI

---

## CLI Commands (Planned)

```bash
# View recent logs
./jtag logs/tail --persona=helper --category=cognition --lines=50

# Query logs
./jtag logs/query --component=AIProviderDaemon --level=ERROR --since=1h

# Export logs
./jtag logs/export --persona=helper --output=/tmp/helper-logs.tar.gz

# Cleanup old logs
./jtag logs/cleanup --older-than=7d --dry-run

# Log statistics
./jtag logs/stats --by-persona --by-size
```

---

## Path Reference

**Single Source of Truth: SystemPaths.ts**

```typescript
import { SystemPaths } from '@system/core/config/SystemPaths';

// System logs directory
SystemPaths.logs.system
// → .continuum/jtag/logs/system/

// Persona logs directory
SystemPaths.logs.personas('helper-unique-id')
// → .continuum/personas/helper-unique-id/logs/

// Session logs root
SystemPaths.logs.root
// → .continuum/jtag/logs/
```

**NEVER hardcode paths** - always use SystemPaths for consistency.

---

## Common Patterns

### Pattern 1: Component Logger (System Log)
```typescript
import { Logger, FileMode } from '@system/core/logging/Logger';
import { SystemPaths } from '@system/core/config/SystemPaths';
import * as path from 'path';

export class MyDaemon {
  private logger = Logger.createWithFile(
    'MyDaemon',
    path.join(SystemPaths.logs.system, 'mydaemon.log'),
    FileMode.CLEAN
  );

  async doWork(): Promise<void> {
    this.logger.info('Starting work');
    try {
      // ... work ...
      this.logger.info('Work completed');
    } catch (error) {
      this.logger.error('Work failed', error);
    }
  }
}
```

### Pattern 2: Persona Logger (Per-Persona Log)
```typescript
import { Logger } from '@system/core/logging/Logger';
import type { PersonaUser } from '@system/user/server/PersonaUser';

export class PersonaCognition {
  private logger: ComponentLogger;

  constructor(private persona: PersonaUser) {
    this.logger = Logger.createForPersona(persona, 'cognition');
  }

  async processThought(): Promise<void> {
    this.logger.info('Processing thought stream');
    this.logger.debug('Temperature:', this.persona.state.temperature);
  }
}
```

### Pattern 3: Injected Logger (Queue, Singleton, etc.)
```typescript
export class OllamaRequestQueue {
  private log: (message: string) => void;

  constructor(maxConcurrent: number, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
    this.log('🔧 Queue initialized');
  }

  enqueue(request: Request): void {
    this.log(`🔄 Enqueued request ${request.id}`);
  }
}

// Usage:
const queue = new OllamaRequestQueue(
  12,
  (msg: string) => this.log(null, 'info', msg) // Inject adapter logger
);
```

### Pattern 4: Conditional Logging (Avoid Spam)
```typescript
// ❌ BAD - Logs every iteration (spam)
for (const item of items) {
  logger.debug('Processing item', item);
}

// ✅ GOOD - Log summary only
logger.info(`Processing ${items.length} items`);
for (const item of items) {
  // ... work ...
}
logger.info('All items processed');
```

---

## Migration Checklist (For Future Console.log Cleanup)

When converting console.log to Logger.ts:

1. **Identify log context**
   - System-wide? → Use system log
   - Persona-specific? → Use persona log
   - Queue/singleton? → Inject logger via constructor

2. **Choose appropriate file**
   - AI provider operations → adapters.log
   - Decision-making → cognition.log
   - Memory operations → hippocampus.log
   - Tool execution → tools.log
   - Training data → training.log

3. **Fix path construction**
   ```typescript
   // ❌ WRONG - bare filename causes path.dirname() → '.'
   Logger.createWithFile('Component', 'myfile', FileMode.CLEAN);

   // ✅ CORRECT - full path with SystemPaths
   Logger.createWithFile(
     'Component',
     path.join(SystemPaths.logs.system, 'myfile.log'),
     FileMode.CLEAN
   );
   ```

4. **Respect LOG_TO_CONSOLE config**
   - Logger.ts automatically checks LOG_TO_CONSOLE=0
   - No need to manually check in component code

5. **Use appropriate log level**
   - `logger.info()` - Normal operations
   - `logger.debug()` - Verbose debugging (filterable later)
   - `logger.warn()` - Unexpected but recoverable
   - `logger.error()` - Failures requiring attention

6. **Test after conversion**
   ```bash
   npm run build:ts
   npm start
   # Verify log file created in correct location
   ls -lh .continuum/jtag/logs/system/
   tail -20 .continuum/jtag/logs/system/myfile.log
   ```

---

## File Descriptor Limits

**Current usage**: 254 open log files

**macOS default**: `ulimit -n` → typically 256 (VERY CLOSE!)

**Recommendations**:
1. **Check current limit**: `ulimit -n`
2. **Increase if needed**: `ulimit -n 1024` (session-only) or edit `/etc/security/limits.conf` (permanent)
3. **Monitor usage**: Add file descriptor count logging to startup

**If hitting limits:**
- Merge rarely-used log categories
- Implement log rotation (close old files)
- Move to structured logging (single SQLite file)

---

## CLI Commands

The logging system provides four commands for discovering, reading, and analyzing logs.

### `logs/list` - Discover Available Logs

List all logs with filtering by category, persona, component, or type.

**Parameters:**
- `category` (optional): Filter by 'system', 'persona', 'session', or 'external'
- `personaId` (optional): Filter by persona UUID
- `personaUniqueId` (optional): Filter by persona uniqueId (e.g., 'helper', 'claudeassistant')
- `component` (optional): Filter by component name
- `logType` (optional): Filter by log type (e.g., 'cognition', 'tools', 'adapters')

**Examples:**
```bash
# List all logs
./jtag logs/list

# List only persona logs
./jtag logs/list --category=persona

# List logs for a specific persona (accepts both id and uniqueId)
./jtag logs/list --personaUniqueId=helper

# List all system logs
./jtag logs/list --category=system

# List cognition logs across all personas
./jtag logs/list --logType=cognition
```

**Output:**
```json
{
  "success": true,
  "logs": [
    {
      "name": "helper/cognition",
      "path": "/path/to/.continuum/personas/helper/logs/cognition.log",
      "category": "persona",
      "component": "PersonaCognition",
      "personaName": "Helper AI",
      "logType": "cognition",
      "sizeMB": 2.5,
      "lineCount": 2543,
      "lastModified": "2025-12-03T04:37:51.070Z",
      "isActive": true
    }
  ],
  "summary": {
    "totalFiles": 127,
    "totalSizeMB": 60.2,
    "categories": {
      "persona": 127
    }
  }
}
```

**Key Features:**
- **Simple naming**: `helper/cognition` instead of full filesystem path
- **Rich metadata**: Size, line count, last modified, active status
- **Summary statistics**: Total files, size, breakdown by category

---

### `logs/read` - Read Log Content

Read specific line ranges with level/component filtering and tail mode support.

**Parameters:**
- `log` (required): Simple log name from `logs/list` (e.g., 'helper/cognition')
- `startLine` (optional): Starting line number (1-indexed)
- `endLine` (optional): Ending line number (inclusive)
- `tail` (optional): Read last N lines (tail mode)
- `level` (optional): Filter by log level ('INFO', 'WARN', 'ERROR', 'DEBUG')
- `component` (optional): Filter by component name

**Examples:**
```bash
# Read last 50 lines (tail mode)
./jtag logs/read --log="helper/cognition" --tail=50

# Read specific line range
./jtag logs/read --log="system/adapters" --startLine=1 --endLine=100

# Read full log
./jtag logs/read --log="system/coordination"

# Filter by log level
./jtag logs/read --log="system/system" --level=ERROR --tail=100

# Continue reading from bookmark
./jtag logs/read --log="helper/cognition" --startLine=2544
```

**Output:**
```json
{
  "success": true,
  "log": "helper/cognition",
  "lines": [
    {
      "lineNumber": 1,
      "content": "[INFO] PersonaCognition: Initializing cognition subsystem...",
      "level": "INFO",
      "timestamp": "2025-12-03T04:37:51.070Z"
    }
  ],
  "totalLines": 2543,
  "hasMore": true,
  "nextLine": 101
}
```

**Navigation Fields:**
- `totalLines`: Total lines in the entire log file
- `hasMore`: True if more lines available after this range
- `nextLine`: Line number to use for next read (pagination)

**Bookmark Workflow:**
```bash
# 1. Read first chunk
./jtag logs/read --log="system/ai-decisions" --startLine=1 --endLine=100
# Returns: { totalLines: 1353, hasMore: true, nextLine: 101 }

# 2. Save bookmark position
./jtag state/update --key="log:ai-decisions:position" --value=101

# 3. Resume reading later
BOOKMARK=$(./jtag state/get --key="log:ai-decisions:position" | jq -r '.value')
./jtag logs/read --log="system/ai-decisions" --startLine=$BOOKMARK --endLine=$((BOOKMARK + 99))
```

---

### `logs/search` - Pattern Matching Across Logs

Search for patterns with context lines (before/after matches).

**Parameters:**
- `pattern` (required): Regular expression or literal string to match
- `caseSensitive` (optional): Case-sensitive matching (default: false)
- `contextBefore` (optional): Lines to show before each match (default: 2)
- `contextAfter` (optional): Lines to show after each match (default: 2)
- `maxMatches` (optional): Maximum matches to return (default: 100)

**Examples:**
```bash
# Search for errors across all logs
./jtag logs/search --pattern="ERROR"

# Search for specific pattern with context
./jtag logs/search --pattern="Ollama.*timeout" --contextBefore=5 --contextAfter=5

# Case-sensitive search
./jtag logs/search --pattern="PersonaCognition" --caseSensitive=true
```

**Output:**
```json
{
  "success": true,
  "pattern": "ERROR",
  "matches": [
    {
      "filePath": "/path/to/helper/cognition.log",
      "lineNumber": 542,
      "line": {
        "lineNumber": 542,
        "content": "[ERROR] PersonaCognition: Failed to process thought stream",
        "level": "ERROR"
      },
      "context": {
        "before": [/* 2 lines before */],
        "after": [/* 2 lines after */]
      },
      "highlightedContent": "[ERROR] PersonaCognition: Failed..."
    }
  ],
  "totalMatches": 37,
  "searchedFiles": 254
}
```

---

### `logs/stats` - Log Statistics

Get aggregate statistics about log files.

**Parameters:**
- `category` (optional): Filter by category
- `personaId` (optional): Filter by persona
- `includeEmpty` (optional): Include empty logs (default: false)

**Examples:**
```bash
# Overall statistics
./jtag logs/stats

# Persona-specific stats
./jtag logs/stats --category=persona

# Individual persona stats
./jtag logs/stats --personaUniqueId=helper
```

**Output:**
```json
{
  "success": true,
  "totalFiles": 254,
  "totalSizeMB": 71.4,
  "totalLines": 125432,
  "byCategory": {
    "system": { "files": 7, "sizeMB": 4.2, "lines": 5432 },
    "persona": { "files": 127, "sizeMB": 60.0, "lines": 115000 },
    "session": { "files": 117, "sizeMB": 7.2, "lines": 5000 }
  },
  "topLogs": [
    { "name": "system/ai-decisions", "sizeMB": 15.2, "lines": 45000 },
    { "name": "helper/cognition", "sizeMB": 8.5, "lines": 25000 }
  ]
}
```

---

## Advanced Workflows

### Persistent Investigation Trail

Combine `logs/` commands with `state/` commands for resumable debugging across sessions.

**Pattern:**
```bash
# 1. Discover relevant logs
./jtag logs/list --category=system | jq '.logs[] | select(.sizeMB > 1)'

# 2. Start investigation
./jtag logs/read --log="system/ai-decisions" --startLine=1 --endLine=100

# 3. Mark important lines (using state commands)
./jtag state/update --key="investigation:ai-bug:line542" --value="ERROR: timeout pattern"
./jtag state/update --key="investigation:ai-bug:line1205" --value="Recovery attempt failed"

# 4. Save bookmark for next session
./jtag state/update --key="log:ai-decisions:bookmark" --value=101

# 5. Search for related patterns
./jtag logs/search --pattern="timeout" --contextBefore=5

# 6. Resume investigation later
BOOKMARK=$(./jtag state/get --key="log:ai-decisions:bookmark" | jq -r '.value')
./jtag logs/read --log="system/ai-decisions" --startLine=$BOOKMARK --endLine=$((BOOKMARK + 99))
```

**Benefits:**
- Resumable across sessions (bookmarks persist in state)
- Mark your investigation trail (important lines tagged)
- Build context incrementally
- Share investigation state between personas/developers

---

### Real-Time Monitoring (Tail Mode)

Monitor active logs for live debugging.

```bash
# Watch last 50 lines of coordination log
watch -n 1 './jtag logs/read --log="system/coordination" --tail=50'

# Monitor errors across all persona logs
watch -n 2 './jtag logs/search --pattern="ERROR" --maxMatches=20'

# Track specific persona's cognition in real-time
watch -n 1 './jtag logs/read --log="helper/cognition" --tail=20 --level=INFO'
```

---

### Cross-Log Analysis

Search patterns across multiple logs and correlate findings.

```bash
# Find all timeout errors across system
./jtag logs/search --pattern="timeout" > timeouts.json

# Count errors by persona
for persona in helper teacher codereview; do
  echo "$persona: $(./jtag logs/search --pattern="ERROR" | jq ".matches[] | select(.filePath | contains(\"$persona\"))" | wc -l) errors"
done

# Compare cognition logs between personas
./jtag logs/read --log="helper/cognition" --tail=50 > helper_cognition.log
./jtag logs/read --log="teacher/cognition" --tail=50 > teacher_cognition.log
diff helper_cognition.log teacher_cognition.log
```

---

## Testing

```bash
# Build and deploy
npm run build:ts
npm start

# Verify system logs
ls -lh .continuum/jtag/logs/system/
tail -20 .continuum/jtag/logs/system/system.log

# Verify persona logs
ls -lh .continuum/personas/helper/logs/
tail -20 .continuum/personas/helper/logs/cognition.log

# Check log file count
find .continuum -name "*.log" -type f | wc -l

# Monitor file descriptors (macOS)
lsof -p $(pgrep -f "npm start") | grep -c ".log"
```

---

## Future Vision

**Ultimate goal**: AI-queryable structured logs

```typescript
// Natural language log queries
await Commands.execute('logs/query', {
  question: 'What errors happened in the last hour?'
});

// → AI reads log database, returns:
// "Found 3 errors:
//  1. Ollama timeout in helper persona at 14:32
//  2. PricingManager missing model in deepseek at 14:45
//  3. WebSocket disconnect in session abc123 at 15:01"
```

This requires structured logging (SQLite) + semantic search (RAG) over log database.

---

**Document version**: 1.0 (December 2025)
**Author**: Claude (Memento) via log analysis and system exploration
**Status**: Initial draft - needs user review and optimization plan approval
