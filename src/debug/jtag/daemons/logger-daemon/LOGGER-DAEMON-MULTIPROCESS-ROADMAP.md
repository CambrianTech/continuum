# Phase 5: LoggerDaemon Multiprocess Architecture

**Status**: Ready to implement
**Priority**: HIGH - All 13 daemons already use Logger.ts
**Estimated Complexity**: Medium (hand-code infrastructure, no generator)

---

## Executive Summary

**Goal**: Transform Logger.ts from in-process singleton into a separate-process LoggerDaemon that aggregates logs from all daemons via IPC.

**Why LoggerDaemon (not HealthDaemon)**:
- HIGH value: All 13 daemons already use Logger.ts (immediate benefit)
- Natural fit: Logging as service is textbook multiprocess pattern
- Completes Phase 4B story: Uniform logging → Centralized service
- Solves ConsoleDaemon recursion concern through process boundaries

**Key Insight**: Process boundaries naturally eliminate concerns about LoggerDaemon logging itself - it's in a separate process with no circular dependency.

---

## Architecture Overview

### Current State (Phase 4B Complete)

All 13 daemons use Logger.ts singleton:

```typescript
// Every daemon constructor (standardized in Phase 4B)
constructor(context: JTAGContext, router: JTAGRouter) {
  super(context, router);
  const className = this.constructor.name;
  this.log = Logger.create(className, `daemons/${className}`);
}
```

**Logger.ts Architecture** (system/core/logging/Logger.ts):
- Singleton with file streams (Map<string, WriteStream>)
- Queue-based buffering (100ms flush interval)
- Multiple categories with directory support
- Fire-and-forget queueing (never blocks caller)
- Shutdown handling (flush all queues, close streams)

### Target State (Phase 5)

LoggerDaemon runs as **separate Node.js process**:

```
┌─────────────────────────────────────────────┐
│  Main Process (JTAGSystemServer)            │
│                                             │
│  ┌──────────────┐   ┌──────────────┐      │
│  │ DataDaemon   │   │ UserDaemon   │      │
│  │ log.info()   │   │ log.error()  │      │
│  └──────┬───────┘   └──────┬───────┘      │
│         │                  │               │
│         └──────────┬───────┘               │
│                    ▼                        │
│         ┌──────────────────┐               │
│         │ LoggerProxy      │               │
│         │ (IPC client)     │               │
│         └──────────┬───────┘               │
└────────────────────┼────────────────────────┘
                     │ JSON over stdin/stdout
                     │
┌────────────────────▼────────────────────────┐
│  LoggerDaemon Process                       │
│                                             │
│  ┌──────────────────────────────┐          │
│  │ LoggerDaemonServer           │          │
│  │ - Receives log IPC messages  │          │
│  │ - Writes to files            │          │
│  │ - Manages streams/buffers    │          │
│  │ - Handles rotation           │          │
│  └──────────────────────────────┘          │
│                                             │
│  .continuum/jtag/logs/system/daemons/       │
│    ├── DataDaemonServer.log                │
│    ├── UserDaemonServer.log                │
│    └── ...                                  │
└─────────────────────────────────────────────┘
```

**Key Transformations**:
1. Logger.ts → LoggerProxy (lightweight IPC client)
2. Logger.ts core logic → LoggerDaemonServer (runs in separate process)
3. File I/O moves from main process to LoggerDaemon process
4. All daemons continue using same `log.info()` API (no code changes)

---

## IPC Protocol Design

### Message Format (JSON over stdin/stdout)

**Log Message** (main process → LoggerDaemon):
```typescript
interface LogMessage {
  type: 'log';
  level: 'debug' | 'info' | 'warn' | 'error';
  component: string;        // e.g., 'DataDaemonServer'
  category: string;         // e.g., 'daemons/DataDaemonServer'
  message: string;
  args: any[];              // Serialized additional arguments
  timestamp: string;        // ISO 8601 timestamp

  // Per-instance settings (passed with each message)
  settings: {
    fileMode: FileMode;     // CLEAN, APPEND, ARCHIVE for THIS logger
    flushIntervalMs: number; // Flush interval for THIS logger's file
    maxQueueSize: number;    // Max queue size for THIS logger's file
    enabled: boolean;        // Whether logging is enabled for THIS logger
  };
}
```

**Per-Instance Settings** (KEY ARCHITECTURE):
- Each `this.log` instance has OWN settings (not global)
- Settings travel WITH each log message (self-contained)
- LoggerDaemon applies settings to specific file/category
- Example: DataDaemon DEBUG mode, HealthDaemon WARN only
- Example: DataDaemon APPEND mode, EventsDaemon CLEAN mode

**Control Messages**:
```typescript
interface FlushMessage {
  type: 'flush';
  category?: string;        // Flush specific category, or all if undefined
}

interface ShutdownMessage {
  type: 'shutdown';
}

interface HealthCheckMessage {
  type: 'ping';
}

interface HealthCheckResponse {
  type: 'pong';
  stats: {
    queued: number;         // Total queued messages
    written: number;        // Total messages written
    streams: number;        // Active file streams
    uptime: number;         // Process uptime in seconds
  };
}
```

### Communication Flow

**Normal Log Call**:
```typescript
// Daemon code (unchanged)
this.log.info('User authenticated', { userId, sessionId });

// LoggerProxy (new)
const message: LogMessage = {
  type: 'log',
  level: 'info',
  component: 'UserDaemonServer',
  category: 'daemons/UserDaemonServer',
  message: 'User authenticated',
  args: [{ userId: '123', sessionId: '456' }],
  timestamp: new Date().toISOString()
};
process.send(message); // Fire-and-forget via IPC

// LoggerDaemonServer (new process)
process.on('message', (msg: LogMessage) => {
  if (msg.type === 'log') {
    this.queueMessage(msg.category, this.formatLogLine(msg));
  }
});
```

**Graceful Shutdown**:
```typescript
// Main process shutdown
await loggerProxy.shutdown();  // Send shutdown message
await loggerProcess.waitForExit(5000);  // Wait up to 5 seconds

// LoggerDaemon process
process.on('message', (msg: ShutdownMessage) => {
  if (msg.type === 'shutdown') {
    this.flushAllQueues();
    this.closeAllStreams();
    process.exit(0);
  }
});
```

---

## Process Lifecycle Management

### DaemonProcessSpawner Class

**Location**: `system/core/process/DaemonProcessSpawner.ts`

**Responsibilities**:
- Spawn LoggerDaemon as child process
- Monitor health (heartbeat every 30s)
- Handle crashes (restart with exponential backoff)
- Graceful shutdown (send shutdown message, wait, kill if timeout)
- IPC message routing

**API**:
```typescript
class DaemonProcessSpawner {
  constructor(
    private daemonName: string,
    private entryPoint: string,  // e.g., 'daemons/logger-daemon/server/LoggerDaemonProcess.ts'
    private ipcHandler: (message: any) => void
  ) {}

  async start(): Promise<void>
  async stop(timeoutMs: number = 5000): Promise<void>
  send(message: any): void
  isHealthy(): boolean
  getStats(): ProcessStats
}

interface ProcessStats {
  pid: number;
  uptime: number;
  memoryUsage: number;
  cpuUsage: number;
  lastHeartbeat: Date;
}
```

**Lifecycle States**:
```
┌─────────┐  start()   ┌─────────┐  healthy   ┌─────────┐
│ STOPPED │──────────> │ STARTING│──────────> │ RUNNING │
└─────────┘            └─────────┘            └────┬────┘
     ▲                                              │
     │                                              │ crash
     │ stop()                                       ▼
     │                 ┌─────────┐  restart   ┌──────────┐
     └─────────────────│ STOPPED │<───────────│ CRASHED  │
                       └─────────┘            └──────────┘
```

### LoggerDaemon Process Entry Point

**Location**: `daemons/logger-daemon/server/LoggerDaemonProcess.ts`

**Purpose**: Standalone Node.js process that runs LoggerDaemonServer in isolation.

```typescript
/**
 * LoggerDaemon Process Entry Point
 *
 * Standalone Node.js process for centralized log aggregation.
 * Receives log messages via IPC, writes to categorized files.
 */

import { LoggerDaemonServer } from './LoggerDaemonServer';

// Create daemon instance (no JTAG dependencies - runs standalone)
const daemon = new LoggerDaemonServer({
  environment: 'server',
  uuid: 'logger-daemon-process'
});

// Initialize daemon
daemon.initialize().then(() => {
  console.log('[LoggerDaemon] Process started successfully');

  // Send ready signal to parent
  if (process.send) {
    process.send({ type: 'ready' });
  }
}).catch(error => {
  console.error('[LoggerDaemon] Failed to initialize:', error);
  process.exit(1);
});

// Handle IPC messages from parent process
process.on('message', async (message: any) => {
  try {
    await daemon.handleMessage(message);
  } catch (error) {
    console.error('[LoggerDaemon] Failed to handle message:', error);
  }
});

// Graceful shutdown
process.on('SIGTERM', async () => {
  console.log('[LoggerDaemon] Received SIGTERM, shutting down...');
  await daemon.shutdown();
  process.exit(0);
});

process.on('SIGINT', async () => {
  console.log('[LoggerDaemon] Received SIGINT, shutting down...');
  await daemon.shutdown();
  process.exit(0);
});
```

---

## Implementation Phases

### Phase 5A: Infrastructure (Week 1)

**Goal**: Create multiprocess infrastructure without touching existing daemons.

**Files to Create**:
1. `system/core/process/DaemonProcessSpawner.ts` (250 lines)
   - Process spawning and lifecycle management
   - Health monitoring with heartbeat
   - Crash detection and restart logic
   - IPC message routing

2. `daemons/logger-daemon/shared/LoggerDaemonTypes.ts` (100 lines)
   - LogMessage, FlushMessage, ShutdownMessage interfaces
   - IPC protocol types
   - Stats and health check types

3. `daemons/logger-daemon/server/LoggerDaemonProcess.ts` (80 lines)
   - Entry point for standalone process
   - IPC message handling
   - Graceful shutdown

**Test Criteria**:
- Can spawn LoggerDaemon process successfully
- Process responds to health checks (ping/pong)
- Process can be gracefully shut down (SIGTERM)
- Process restarts on crash (kill -9)
- IPC messages sent/received correctly

**Success Metrics**:
- LoggerDaemon process PID visible in `ps aux`
- Health check returns stats every 30s
- Shutdown completes within 5 seconds
- Restart happens within 1 second of crash

### Phase 5B: LoggerDaemon Implementation (Week 1-2)

**Goal**: Implement LoggerDaemonServer with all Logger.ts features.

**Files to Create**:
1. `daemons/logger-daemon/shared/LoggerDaemon.ts` (150 lines)
   - Base class extending DaemonBase
   - Abstract methods for environment-specific implementations
   - Message handling interface

2. `daemons/logger-daemon/server/LoggerDaemonServer.ts` (400 lines)
   - Extends LoggerDaemon
   - File stream management (Map<string, WriteStream>)
   - Queue-based buffering (100ms flush interval)
   - Category support with subdirectory creation
   - Shutdown handling (flush all, close streams)
   - **Migrates core logic from Logger.ts lines 80-343**

**Migration Map** (Logger.ts → LoggerDaemonServer):
- `Logger.getInstance()` → `LoggerDaemonServer` constructor
- `Logger.create()` → Register component in daemon
- `Logger.queueMessage()` → IPC message handler
- `Logger.flushQueue()` → Timer-based flushing (unchanged)
- `Logger.getFileStream()` → Stream management (unchanged)
- `Logger.shutdown()` → Daemon shutdown (unchanged)

**Test Criteria**:
- LoggerDaemon receives log messages via IPC
- Log files created in correct directories
- Queue flushing works (100ms interval)
- Multiple categories work (daemons/, sql/, persona-mind/)
- Shutdown flushes all queues before exit
- File mode (CLEAN/APPEND) works correctly

**Success Metrics**:
- All daemon logs appear in .continuum/jtag/logs/system/daemons/
- No log messages lost during shutdown
- File streams properly closed (no leaks)
- Buffering reduces file I/O (verify with strace)

### Phase 5C: LoggerProxy Integration (Week 2)

**Goal**: Keep Logger.ts as IPC client (LoggerProxy), add per-instance configuration.

**Critical Architecture Decision** (per user feedback):
- **KEEP Logger.ts** - becomes LoggerProxy (client to LoggerDaemon)
- **Each `this.log` instance** has OWN file, OWN rules, OWN settings
- **Per-instance control**: Log level, file mode, flush interval, etc.
- **API unchanged**: All daemons continue using `this.log.info()` exactly as before

**Files to Create**:
1. `system/core/logging/LoggerProxy.ts` (300 lines)
   - IPC client (sends log messages to LoggerDaemon)
   - ComponentLogger interface (unchanged API)
   - **Per-instance settings** (log level, file mode, enabled/disabled)
   - Fire-and-forget message sending
   - Fallback to console if LoggerDaemon unavailable
   - Queues messages during daemon startup

2. `system/core/logging/LoggerSettings.ts` (150 lines)
   - Per-logger configuration
   - Settings: level, fileMode, enabled, flushIntervalMs, maxQueueSize
   - Runtime updates (change settings without restart)
   - Persistence (.continuum/jtag/config/logger-settings.json)
   - Default settings with per-component overrides

**Per-Instance Settings Architecture**:
```typescript
interface LoggerSettings {
  component: string;           // e.g., 'DataDaemonServer'
  category: string;            // e.g., 'daemons/DataDaemonServer'
  level: LogLevel;             // DEBUG, INFO, WARN, ERROR
  fileMode: FileMode;          // CLEAN, APPEND, ARCHIVE
  enabled: boolean;            // Can disable specific loggers
  flushIntervalMs: number;     // How often to flush (default: 100ms)
  maxQueueSize: number;        // Max buffered messages (default: 1000)
}

// Example: Different settings for different daemons
const settings = {
  'DataDaemonServer': {
    level: LogLevel.DEBUG,     // Verbose for debugging
    fileMode: FileMode.APPEND, // Keep history
    flushIntervalMs: 50        // Faster flushing
  },
  'HealthDaemonServer': {
    level: LogLevel.WARN,      // Only warnings/errors
    fileMode: FileMode.CLEAN,  // Start fresh
    enabled: false             // Disable health daemon logs
  }
};
```

**API Compatibility** (UNCHANGED):
```typescript
// Daemon code (ZERO CHANGES)
constructor(context: JTAGContext, router: JTAGRouter) {
  super(context, router);
  const className = this.constructor.name;
  this.log = Logger.create(className, `daemons/${className}`);
  // Settings loaded automatically from LoggerSettings
}

this.log.info('User authenticated', { userId });
// - Checks per-instance settings (level, enabled)
// - Sends to LoggerDaemon via IPC if enabled
// - LoggerDaemon writes to separate file per category
// - Each file has OWN rules (mode, flush interval, max size)
```

**Migration Strategy**:
1. Create LoggerSettings.ts (per-instance configuration)
2. Rename `Logger.ts` → `Logger.ts.backup` (temporary backup)
3. Create `LoggerProxy.ts` with IDENTICAL API + settings support
4. Export as `Logger` (so all imports still work: `import { Logger }`)
5. Test: All daemons work without code changes
6. Verify: Each daemon logs to OWN file with OWN settings
7. Delete backup after validation

**Test Criteria**:
- All 13 daemons log successfully through LoggerProxy
- No code changes needed in daemon constructors
- Logs appear in same directories as Phase 4B
- Fallback to console works if LoggerDaemon crashes
- Messages queued during LoggerDaemon startup

**Success Metrics**:
- Zero daemon code changes (only import paths)
- All Phase 4B tests still pass
- No log messages lost
- System continues working if LoggerDaemon unavailable

### Phase 5D: System Integration (Week 2)

**Goal**: Integrate LoggerDaemon into JTAGSystem startup.

**Files to Modify**:
1. `system/core/system/server/JTAGSystemServer.ts`
   - Spawn LoggerDaemon before other daemons
   - Wait for ready signal
   - Register for shutdown

**Startup Sequence**:
```typescript
async connect(): Promise<void> {
  this.log.info('🚀 Starting JTAG System...');

  // 1. Spawn LoggerDaemon FIRST (before other daemons)
  this.loggerDaemonSpawner = new DaemonProcessSpawner(
    'logger-daemon',
    'daemons/logger-daemon/server/LoggerDaemonProcess.ts',
    this.handleLoggerMessage.bind(this)
  );
  await this.loggerDaemonSpawner.start();

  // 2. Wait for LoggerDaemon ready signal (up to 5 seconds)
  await this.waitForLoggerReady(5000);

  // 3. Setup all other daemons (now log through LoggerProxy → LoggerDaemon)
  await this.setupDaemons();

  this.log.info('✅ JTAG System connected');
}

async shutdown(): Promise<void> {
  this.log.info('🔌 Shutting down JTAG System...');

  // 1. Shutdown all daemons (generate log messages)
  await this.shutdownDaemons();

  // 2. Shutdown LoggerDaemon LAST (after all other daemons)
  await this.loggerDaemonSpawner.stop(5000);

  this.log.info('✅ JTAG System shutdown complete');
}
```

**Test Criteria**:
- LoggerDaemon starts before other daemons
- System waits for ready signal (max 5s)
- All daemon logs routed through LoggerDaemon
- LoggerDaemon shuts down last (after all daemons)
- No log messages lost during startup/shutdown

**Success Metrics**:
- System startup succeeds (all daemons initialized)
- Logs appear in correct directories
- `npm start` works end-to-end
- `./jtag ping` confirms system operational
- Precommit hook passes (CRUD + State tests)

---

## Files to Create/Modify

### New Files (Phase 5)

```
system/core/process/
└── DaemonProcessSpawner.ts          # 250 lines - Process lifecycle management

daemons/logger-daemon/
├── LOGGER-DAEMON-MULTIPROCESS-ROADMAP.md  # This file
├── shared/
│   ├── LoggerDaemon.ts              # 150 lines - Base daemon class
│   └── LoggerDaemonTypes.ts         # 100 lines - IPC protocol types
└── server/
    ├── LoggerDaemonServer.ts        # 400 lines - Main implementation
    └── LoggerDaemonProcess.ts       # 80 lines - Entry point

system/core/logging/
├── Logger.legacy.ts                 # Backup of original Logger.ts
└── LoggerProxy.ts                   # 200 lines - IPC client (replaces Logger.ts)
```

**Total New Code**: ~1,180 lines

### Modified Files (Phase 5)

```
system/core/system/server/JTAGSystemServer.ts
- Add LoggerDaemon spawning to connect() method
- Add shutdown sequencing (LoggerDaemon last)
- ~50 lines added

system/core/logging/Logger.ts
- DELETE THIS FILE (replaced by LoggerProxy.ts)
- All imports updated automatically (export name unchanged)
```

---

## Test Strategy

### Unit Tests (Isolated Components)

**DaemonProcessSpawner**:
```bash
npx vitest tests/unit/DaemonProcessSpawner.test.ts
```
- Can spawn process successfully
- Health checks work (ping/pong)
- Graceful shutdown works (SIGTERM)
- Crash detection and restart work
- IPC message routing works

**LoggerDaemonServer**:
```bash
npx vitest tests/unit/LoggerDaemonServer.test.ts
```
- Handles log messages via IPC
- Creates files in correct directories
- Queue flushing works (100ms interval)
- Multiple categories work
- Shutdown flushes all queues
- File mode (CLEAN/APPEND) works

**LoggerProxy**:
```bash
npx vitest tests/unit/LoggerProxy.test.ts
```
- Sends IPC messages correctly
- API matches original Logger.ts
- Fallback to console works
- Queues messages during daemon startup

### Integration Tests (Real System)

**End-to-End Logging**:
```bash
npx vitest tests/integration/logger-daemon-e2e.test.ts
```
- LoggerDaemon spawns successfully
- All 13 daemons log through LoggerProxy
- Logs appear in correct directories
- Shutdown completes gracefully
- No log messages lost

**Crash Recovery**:
```bash
npx vitest tests/integration/logger-daemon-crash.test.ts
```
- Kill LoggerDaemon process (kill -9)
- System detects crash within 1 second
- LoggerDaemon restarts automatically
- Buffered messages delivered after restart
- No permanent data loss

### System Tests (npm start)

**Deployment Validation**:
```bash
cd src/debug/jtag
npm start  # Wait 90+ seconds

# Verify LoggerDaemon running
ps aux | grep LoggerDaemon  # Should show separate process

# Verify logs being written
tail -f .continuum/jtag/logs/system/daemons/DataDaemonServer.log

# Verify system operational
./jtag ping  # Should show 2 processes now (main + LoggerDaemon)

# Verify graceful shutdown
pkill -TERM jtag  # Main process shutdown
# LoggerDaemon should flush and exit within 5 seconds
```

**Precommit Hook** (existing tests should pass):
```bash
./scripts/git-precommit.sh
# CRUD test: Validates data operations work
# State test: Validates user state works
# Both should pass with LoggerDaemon (zero code changes)
```

---

## Success Criteria

### Phase 5 Complete When:

1. **Process Isolation**: LoggerDaemon runs in separate Node.js process
2. **Zero Code Changes**: All 13 daemons work without modifications
3. **All Tests Pass**: Unit, integration, system, and precommit tests
4. **Graceful Shutdown**: LoggerDaemon flushes all queues before exit (5s max)
5. **Crash Recovery**: LoggerDaemon restarts automatically within 1 second
6. **Performance**: No noticeable latency increase (fire-and-forget IPC)
7. **Observability**: `ps aux` shows 2 processes (main + LoggerDaemon)

### Metrics to Track:

- **Startup Time**: LoggerDaemon ready signal within 5 seconds
- **IPC Latency**: Message send time < 1ms (fire-and-forget)
- **Queue Depth**: Average queued messages < 100 (100ms flush interval)
- **Memory Usage**: LoggerDaemon RSS < 50MB (file streams only)
- **Log Throughput**: 1000+ messages/second sustained
- **Crash Recovery**: Restart within 1 second, zero lost messages

---

## Future Enhancements (Phase 6+)

**Not in Phase 5, but enabled by this architecture:**

1. **Daemon Generator** (Phase 6)
   - Create template for multiprocess daemons
   - Based on LoggerDaemon lessons learned
   - Generate HealthDaemon, MetricsDaemon, etc.

2. **Entity-Based Configuration** (Phase 7)
   - SystemSchedulingConfigEntity
   - Per-daemon resource limits (CPU, memory, file handles)
   - Dynamic configuration without restart

3. **Ares System Optimizer** (Phase 8)
   - Ares persona monitors LoggerDaemon metrics
   - Proposes configuration changes democratically
   - Optimizes log levels, flush intervals, buffer sizes

4. **Multi-Instance LoggerDaemon** (Future)
   - Horizontal scaling: Multiple LoggerDaemon processes
   - Sharding by category or load
   - Log aggregation across multiple machines

5. **Log Streaming** (Future)
   - WebSocket endpoint for real-time log streaming
   - Widget integration (live log viewer)
   - Filtering and search

---

## Risk Analysis

### Potential Issues and Mitigations

**Risk**: IPC message queue grows unbounded during LoggerDaemon downtime
**Mitigation**: LoggerProxy buffers up to 10,000 messages, drops oldest if exceeded
**Fallback**: Log to console if buffer full

**Risk**: LoggerDaemon crash loses buffered messages
**Mitigation**: Main process buffers messages during startup/crash
**Fallback**: Flush to console on main process shutdown

**Risk**: LoggerDaemon startup delay blocks system initialization
**Mitigation**: 5-second timeout for ready signal, continue without logging
**Fallback**: LoggerProxy falls back to console if daemon unavailable

**Risk**: Process spawning fails (permissions, ulimit, etc.)
**Mitigation**: Comprehensive error handling in DaemonProcessSpawner
**Fallback**: Run Logger.ts in-process (graceful degradation)

**Risk**: File descriptor exhaustion (too many log files)
**Mitigation**: LoggerDaemon closes inactive streams after 60s
**Fallback**: Consolidate logs to fewer categories

---

## Comparison: LoggerDaemon vs HealthDaemon

**Why LoggerDaemon is BETTER for Phase 5:**

| Criterion | LoggerDaemon | HealthDaemon |
|-----------|-------------|--------------|
| **Immediate Value** | HIGH - All 13 daemons benefit | LOW - New feature, no current users |
| **Complexity** | MEDIUM - Well-defined problem | MEDIUM - Must design health checks first |
| **Risk** | LOW - Logger.ts already proven | MEDIUM - New architectural pattern |
| **Architectural Fit** | PERFECT - Logging as service is textbook multiprocess | GOOD - Health monitoring fits multiprocess |
| **Phase 4B Synergy** | EXCELLENT - Completes uniform logging story | NONE - Unrelated to Phase 4B |
| **Learning Value** | HIGH - Proves multiprocess works for critical service | MEDIUM - Proves multiprocess works for utility |
| **Generator Template** | EXCELLENT - Representative daemon pattern | GOOD - Simpler daemon pattern |

**Conclusion**: LoggerDaemon provides more immediate value, better architecture fit, and completes the Phase 4B story.

---

## Next Steps

**Immediate** (This Week):
1. Review this roadmap with user (get approval)
2. Create Phase 5A infrastructure:
   - DaemonProcessSpawner.ts
   - LoggerDaemonTypes.ts
   - LoggerDaemonProcess.ts
3. Test process spawning in isolation
4. Verify health checks and shutdown work

**Short-Term** (Week 2):
1. Implement Phase 5B (LoggerDaemonServer)
2. Migrate core logic from Logger.ts
3. Test log aggregation in isolation
4. Verify all Logger.ts features work

**Medium-Term** (Week 2-3):
1. Create Phase 5C (LoggerProxy)
2. Replace Logger.ts in main process
3. Test all daemons with LoggerProxy
4. Verify zero code changes needed

**Long-Term** (Week 3-4):
1. Integrate Phase 5D (JTAGSystem)
2. Test end-to-end system
3. Deploy with `npm start`
4. Verify all tests pass

**After Phase 5**:
- Create daemon generator (Phase 6)
- Implement entity-based configuration (Phase 7)
- Deploy Ares persona for system optimization (Phase 8)

---

## Questions for User

Before starting implementation:

1. **Approval**: Does this roadmap align with your vision?
2. **Priorities**: Should we proceed with Phase 5A immediately?
3. **Scope**: Any features to add/remove from Phase 5?
4. **Timeline**: Is 3-4 weeks reasonable, or should we compress/expand?
5. **Testing**: Is the test strategy comprehensive enough?

**Ready to begin Phase 5A when you give the green light!** 🚀
