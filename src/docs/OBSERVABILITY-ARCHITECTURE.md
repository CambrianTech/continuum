# Observability Architecture

## Vision

**Complete observability** for a system where multiple AI personas think, act, and collaborate autonomously. Every thought, decision, tool call, and build output must be:

1. **Captured** - Full logs, not truncated
2. **Segregated** - Per-persona, per-subsystem, per-sentinel
3. **Controllable** - Turn on/off via config, state, or command
4. **Streamable** - Real-time events for UI display
5. **Queryable** - Search, filter, correlate across logs

**The ideal**: Watch Helper AI's prefrontal cortex think through a problem, see its tool calls execute, observe the build sentinel fix errors, and drill into any step - all with the ability to mute noisy subsystems.

---

## Three Domains of Observability

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           OBSERVABILITY DOMAINS                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐         │
│  │  SYSTEM LOGS     │  │  COGNITION LOGS  │  │  SENTINEL LOGS   │         │
│  │                  │  │                  │  │                  │         │
│  │  Infrastructure  │  │  AI Thinking     │  │  Autonomous      │         │
│  │  - Daemons       │  │  - Prefrontal    │  │  Execution       │         │
│  │  - Adapters      │  │  - Hippocampus   │  │  - Build output  │         │
│  │  - SQL queries   │  │  - Motor cortex  │  │  - LLM queries   │         │
│  │  - Events        │  │  - Limbic system │  │  - File changes  │         │
│  │                  │  │  - Training      │  │  - Evidence      │         │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘         │
│         │                       │                       │                  │
│         └───────────────────────┴───────────────────────┘                  │
│                                 │                                          │
│                    ┌────────────▼────────────┐                            │
│                    │   UNIFIED CONTROLS      │                            │
│                    │   - LoggingConfig       │                            │
│                    │   - LogLevelRegistry    │                            │
│                    │   - CLI commands        │                            │
│                    │   - Event streaming     │                            │
│                    └─────────────────────────┘                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Logs

Infrastructure logging for daemons, adapters, and core services.

### Directory Structure

```
.continuum/jtag/logs/system/
├── system.log              # Global events (startup, shutdown)
├── sql.log                 # Database operations
├── adapters.log            # AI provider adapter activity
├── tools.log               # System-level tool execution
├── coordination.log        # Coordination state
├── ai-decisions-*.log      # AI decision training data
└── daemons/                # Per-daemon logs (auto-routed)
    ├── ArchiveDaemonServer.log
    ├── SessionDaemonServer.log
    └── ...
```

### Usage

```typescript
import { Logger } from '@system/core/logging/Logger';

// Auto-routes based on component name suffix
const log = Logger.create('ArchiveDaemonServer');  // → daemons/ArchiveDaemonServer.log
const log = Logger.create('SqliteStorageAdapter'); // → adapters/SqliteStorageAdapter.log

log.info('Starting operation');
log.debug('Detailed context', { params });
log.warn('Unusual condition');
log.error('Operation failed', error);
```

### Controls

**Environment variables:**
```bash
LOG_LEVEL=warn           # error | warn | info | debug
LOG_TO_CONSOLE=0         # Disable console output
LOG_TO_FILES=1           # Enable file logging
LOG_FILE_MODE=clean      # clean | append | archive
```

**Runtime level control:**
```typescript
import { LogLevelRegistry, LogLevel } from '@system/core/logging/LogLevelRegistry';

// Set global level
LogLevelRegistry.instance.globalLevel = LogLevel.WARN;

// Set per-component level (suppress noisy components)
LogLevelRegistry.instance.configure({
  'SessionDaemonServer': LogLevel.WARN,    // Only warnings and errors
  'ArchiveDaemonServer': LogLevel.ERROR,   // Only errors
  'PersonaUser': LogLevel.WARN,            // Suppress autonomous loop spam
});
```

---

## 2. Cognition Logs (Neuroanatomy-Inspired)

Per-persona logging organized by cognitive subsystem. Each persona gets isolated log directories with brain-region-inspired categories.

### Directory Structure

```
.continuum/personas/{uniqueId}/logs/
├── prefrontal.log      # High-level cognition, planning, state tracking
├── motor-cortex.log    # Action execution, response generation
├── limbic.log          # Memory, learning, emotion, genome operations
├── hippocampus.log     # Memory consolidation, recall
├── cns.log             # Central nervous system coordination
├── cognition.log       # Decision-making, thought streams
├── tools.log           # Tool execution (per-persona)
├── genome.log          # LoRA adapter paging, eviction
├── training.log        # Fine-tuning data accumulation
└── user.log            # User interaction events
```

### Usage

```typescript
import { SubsystemLogger } from '@system/user/server/modules/being/logging/SubsystemLogger';

class PrefrontalCortex {
  private logger: SubsystemLogger;

  constructor(persona: PersonaUser) {
    this.logger = new SubsystemLogger('prefrontal', persona.id, persona.entity.uniqueId);
  }

  async processThought(thought: string): Promise<void> {
    this.logger.info('Processing thought', { thought });
    // ... cognition ...
    this.logger.debug('Decision made', { action: 'respond' });
  }
}
```

### The On/Off Control System

**LoggingConfig** (`system/core/logging/LoggingConfig.ts`) provides per-persona, per-category control.

**Config file:** `.continuum/logging.json`

```json
{
  "version": 1,
  "defaults": {
    "enabled": false,       // OFF by default - opt-in logging
    "categories": []
  },
  "personas": {
    "*": { "enabled": false },              // All personas off by default
    "helper": {
      "enabled": true,
      "categories": ["cognition", "hippocampus"]  // Only these categories
    },
    "codereview": {
      "enabled": true,
      "categories": ["*"]                   // All categories
    }
  },
  "system": {
    "enabled": true,
    "categories": []
  }
}
```

**Programmatic control:**

```typescript
import { LoggingConfig } from '@system/core/logging/LoggingConfig';

// Check if enabled
LoggingConfig.isEnabled('helper', 'cognition');  // true
LoggingConfig.isEnabled('helper', 'training');   // false

// Enable/disable
LoggingConfig.setEnabled('helper', 'training', true);
LoggingConfig.setPersonaEnabled('teacher', false);  // Disable all for persona

// Hot reload (if edited externally)
LoggingConfig.reload();
```

**CLI commands (planned):**

```bash
# Enable specific persona + category
./jtag logging/enable --persona=helper --category=cognition

# Disable all logging for a persona
./jtag logging/disable --persona=teacher

# Show current config
./jtag logging/config

# Set to verbose (all personas, all categories)
./jtag logging/config --preset=verbose

# Set to minimal (errors only)
./jtag logging/config --preset=minimal
```

### Known Logging Categories

```typescript
export const LOGGING_CATEGORIES = {
  // Persona categories (cognition subsystems)
  COGNITION: 'cognition',      // Task processing, decisions
  HIPPOCAMPUS: 'hippocampus',  // Memory consolidation
  TRAINING: 'training',        // Fine-tuning data
  GENOME: 'genome',            // LoRA adapter paging
  USER: 'user',                // Persona lifecycle
  ADAPTERS: 'adapters',        // AI provider activity

  // System categories
  SERVER: 'server',
  BROWSER: 'browser',
  COMMANDS: 'commands',
  EVENTS: 'events'
} as const;
```

---

## 3. Sentinel Logs

Execution logs for autonomous agents (BuildSentinel, TaskSentinel, etc.). Unlike cognition logs which track thinking, sentinel logs track **doing**.

### Current State

Sentinels currently use `SentinelExecutionLog` for structured action tracking:

```typescript
// SentinelExecutionLog captures:
interface SentinelAction {
  timestamp: string;
  type: 'build' | 'analyze' | 'fix' | 'llm_query' | 'file_edit' | 'escalate';
  intent: string;
  result: 'success' | 'failure' | 'skipped';
  evidence?: {
    output?: string;      // TRUNCATED to ~200 chars
    outputFile?: string;  // Reference to full log
  };
}
```

**Problem**: Build output is captured but truncated. Massive compilation logs are lost.

### Planned Architecture

**Per-sentinel log directories** inside workspace:

```
.sentinel-workspaces/{handle}/
└── logs/
    ├── sentinel.log      # High-level actions
    ├── build-1.log       # FULL output from build attempt 1
    ├── build-2.log       # FULL output from build attempt 2
    ├── llm-requests.log  # LLM queries and responses
    └── evidence/         # Structured evidence files
```

**Real-time streaming via Events:**

```typescript
// Emit as build runs
Events.emit(`sentinel:${handle}:log`, {
  stream: 'build-1',
  chunk: 'Compiling src/index.ts...\n',
  sourceType: 'stdout'
});

// Subscribe for real-time UI
Events.subscribe(`sentinel:${handle}:log`, (event) => {
  appendToUI(event.payload.chunk);
});
```

**CLI commands:**

```bash
# List logs for a sentinel run
./jtag sentinel/logs/list --handle=abc123

# Read full build output (not truncated!)
./jtag sentinel/logs/read --handle=abc123 --stream=build-1

# Tail in real-time
./jtag sentinel/logs/tail --handle=abc123 --stream=build-1
```

---

## Unified Control Architecture

### Configuration Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                    CONFIGURATION SOURCES                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Environment Variables (highest priority for system)         │
│     LOG_LEVEL=warn LOG_TO_CONSOLE=0                            │
│                                                                 │
│  2. LogLevelRegistry (runtime per-component)                   │
│     LogLevelRegistry.instance.configure({...})                 │
│                                                                 │
│  3. LoggingConfig (per-persona, per-category)                  │
│     .continuum/logging.json                                     │
│                                                                 │
│  4. CLI Commands (runtime adjustment)                          │
│     ./jtag logging/enable --persona=helper                     │
│                                                                 │
│  5. State System (persistent preferences)                      │
│     ./jtag state/set --key=logging:helper:enabled --value=true │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Flow: How Logging Decisions Are Made

```typescript
// In SubsystemLogger.ts
private get _enabled(): boolean {
  return LoggingConfig.isEnabled(this.uniqueId, this.subsystem);
}

info(message: string, ...args: unknown[]): void {
  if (!this._enabled) return;  // Early exit if disabled
  this.logger.info(message, ...args);
}
```

```typescript
// In LoggingConfig.ts
static isEnabled(personaId: string, category: string): boolean {
  // 1. Check persona-specific config
  let config = this.config.personas[personaId];

  // 2. Fall back to wildcard "*"
  if (!config) config = this.config.personas['*'];

  // 3. Fall back to defaults
  if (!config) return this.config.defaults.enabled;

  // 4. Check category filter
  if (config.categories?.includes('*')) return true;
  if (config.categories?.length > 0) {
    return config.categories.includes(category);
  }

  return config.enabled;
}
```

### Presets for Quick Configuration

```typescript
const LOGGING_PRESETS = {
  // Everything off except errors
  minimal: {
    defaults: { enabled: false },
    personas: { '*': { enabled: false } },
    system: { enabled: true, categories: ['error'] }
  },

  // System on, personas off
  system_only: {
    defaults: { enabled: false },
    personas: { '*': { enabled: false } },
    system: { enabled: true }
  },

  // Focus on one persona
  focus_helper: {
    defaults: { enabled: false },
    personas: {
      '*': { enabled: false },
      'helper': { enabled: true, categories: ['*'] }
    }
  },

  // Everything on (verbose debugging)
  verbose: {
    defaults: { enabled: true, categories: ['*'] },
    personas: { '*': { enabled: true, categories: ['*'] } },
    system: { enabled: true, categories: ['*'] }
  }
};
```

---

## Event-Based Observability

### Event Types

| Event Pattern | Domain | Description |
|---------------|--------|-------------|
| `log:{component}:{level}` | System | System log entry |
| `persona:{id}:cognition:{subsystem}` | Cognition | Persona thought/decision |
| `sentinel:{handle}:log` | Sentinel | Real-time execution output |
| `sentinel:{handle}:action` | Sentinel | Action taken (build, fix, etc.) |
| `sentinel:{handle}:status` | Sentinel | Status change (running, completed) |
| `tool:{name}:start` | Tools | Tool execution started |
| `tool:{name}:result` | Tools | Tool execution completed |

### Subscribing to Observability Events

```typescript
import { Events } from '@system/core/shared/Events';

// Watch a specific persona's cognition
Events.subscribe('persona:helper:cognition:prefrontal', (event) => {
  console.log('Helper thinking:', event.thought);
});

// Watch all sentinel activity
Events.subscribe('sentinel:*:action', (event) => {
  console.log(`[${event.handle}] ${event.type}: ${event.intent}`);
});

// Watch tool execution across all personas
Events.subscribe('tool:*:result', (event) => {
  console.log(`Tool ${event.name} returned:`, event.result);
});
```

### Real-Time UI Integration

```typescript
// In chat widget, show inline logs for tool calls
class ChatMessage {
  render() {
    if (this.message.toolCall) {
      return html`
        <div class="tool-call">
          <span class="tool-name">${this.message.toolCall.name}</span>
          ${this.message.toolCall.sentinelHandle ? html`
            <sentinel-log-inline
              handle=${this.message.toolCall.sentinelHandle}
              collapsed
            ></sentinel-log-inline>
          ` : ''}
        </div>
      `;
    }
  }
}
```

---

## CLI Commands Reference

### Discovery

```bash
# List all logs with metadata
./jtag logs/list
./jtag logs/list --category=persona
./jtag logs/list --personaUniqueId=helper

# Get statistics
./jtag logs/stats
./jtag logs/stats --category=persona --personaUniqueId=helper
```

### Reading

```bash
# Read log content
./jtag logs/read --log="helper/cognition" --tail=50
./jtag logs/read --log="system/adapters" --startLine=1 --endLine=100

# Search across logs
./jtag logs/search --pattern="ERROR"
./jtag logs/search --pattern="timeout" --contextBefore=5

# Sentinel-specific
./jtag sentinel/logs/list --handle=abc123
./jtag sentinel/logs/read --handle=abc123 --stream=build-1
```

### Configuration

```bash
# Current config
./jtag logging/config

# Enable/disable
./jtag logging/enable --persona=helper --category=cognition
./jtag logging/disable --persona=teacher
./jtag logging/disable --all  # Emergency shutoff

# Presets
./jtag logging/config --preset=minimal
./jtag logging/config --preset=verbose
./jtag logging/config --preset=focus_helper
```

### Maintenance

```bash
# Cleanup old logs
./jtag logs/cleanup --olderThan=7d
./jtag logs/cleanup --category=session --olderThan=1d

# Export for analysis
./jtag logs/export --persona=helper --output=/tmp/helper-logs.tar.gz
```

---

## Implementation Status

### Completed

- [x] Logger.ts core with async write queues
- [x] Per-component auto-routing (daemons/, adapters/, etc.)
- [x] SubsystemLogger for neuroanatomy-inspired persona logs
- [x] LoggingConfig for per-persona/category control
- [x] LogLevelRegistry for runtime level adjustment
- [x] logs/list, logs/read, logs/search, logs/stats commands
- [x] SentinelExecutionLog with streaming events
- [x] SentinelWorkspace for isolated execution

### In Progress

- [ ] Sentinel per-run log directories
- [ ] Full build output capture (not truncated)
- [ ] sentinel/logs/* commands

### Completed (Recent)

- [x] logging/enable command
- [x] logging/disable command
- [x] logging/status command

### Planned

- [ ] Real-time log streaming UI widget
- [ ] Inline chat log display for tool calls
- [ ] Log retention policies
- [ ] Log compression/archival
- [ ] AI-queryable structured logs (natural language queries)
- [ ] Probes - targeted, conditional observation points (requires concurrent architecture)

---

## Performance Considerations

### Current Bottlenecks

1. **254+ open file descriptors** - Close to macOS default limit (256)
2. **Memory pressure** - Each queue holds buffered writes
3. **No compression** - Plain text files grow large
4. **No rotation** - CLEAN truncates, APPEND grows unbounded

### Mitigation Strategies

1. **Aggressive filtering** - Use LoggingConfig to disable unused categories
2. **Level suppression** - Set noisy components to WARN/ERROR only
3. **Retention policies** - Auto-delete logs older than N days
4. **Rust backend** - LoggerModule in continuum-core handles high-throughput

### Rust Logger Integration

The Logger class can route to Rust for high-performance logging:

```typescript
// Logger.ts
if (this.useRustLogger && this.workerClient?.connected) {
  // Send to Rust LoggerModule via Unix socket
  await this.workerClient.writeLog(logFile, message, level);
} else {
  // Fallback to TypeScript file I/O
  this.queueMessage(logFile, message);
}
```

---

## Quick Reference: "I Want To..."

| Goal | Solution |
|------|----------|
| See what a persona is thinking | `./jtag logs/read --log="helper/prefrontal" --tail=50` |
| Watch build output in real-time | `./jtag sentinel/logs/tail --handle=X --stream=build-1` |
| Disable noisy persona logs | `./jtag logging/disable --persona=helper` |
| Enable only cognition logs | `./jtag logging/enable --persona=helper --category=cognition` |
| Find all errors | `./jtag logs/search --pattern="ERROR"` |
| Reduce logging overhead | `./jtag logging/config --preset=minimal` |
| Debug one specific persona | `./jtag logging/config --preset=focus_helper` |
| Clean up old logs | `./jtag logs/cleanup --olderThan=7d` |
| Check current config | `./jtag logging/config` |
| Edit config manually | `vim .continuum/logging.json` (hot reloads) |

---

## 4. Probes (Future)

Probes are **targeted, conditional, parameterized observation points** - more surgical than logging. While logging is "turn on the firehose for category X", probes answer specific questions.

### Why Probes, Not Breakpoints

Traditional debuggers use breakpoints that **stop** execution. This doesn't work for async distributed systems:

- Pausing one persona breaks timing-dependent coordination
- Deadlocks if a persona pauses mid-conversation
- Only works if architecture is truly concurrent (isolated actors, message passing)

Probes **observe without stopping** - they're instrumentation, not interruption.

### Probe Concept

```typescript
// Define a probe - "next time Helper's gating decision has low confidence, capture it"
probe/create --name="gating-low-conf" \
  --target="persona:helper:cognition:gating" \
  --condition="confidence < 0.3" \
  --mode="once"

// Modes:
// - once: Capture next occurrence, then disable
// - count:N: Capture N occurrences, then disable
// - duration:Xs: Capture for X seconds, then disable
// - accumulate: Collect until manually disabled, then dump summary

// Check what it captured
probe/results --name="gating-low-conf"

// Disable
probe/disable --name="gating-low-conf"
```

### Implementation Requirements

Probes need:

1. **Probe registry** - What probes exist, their conditions, their state
2. **Instrumentation points** - Code locations that check for active probes
3. **Condition evaluation** - Fast check: `if (probeActive && conditionMet)`
4. **Result storage** - Captured data with the parameterized limits
5. **Concurrent-safe architecture** - Probes must not interfere with execution

### Probe vs Logging

| Aspect | Logging | Probes |
|--------|---------|--------|
| Scope | Category-wide (cognition, hippocampus) | Single point + condition |
| Duration | Persistent until disabled | Parameterized (once, N times, duration) |
| Output | Stream to file | Captured results, queryable |
| Overhead | Always on when enabled | Only when condition checked |
| Use case | "What's happening?" | "Why did X happen?" |

### Example Use Cases

```bash
# "Why did Helper stay silent on that message?"
probe/create --name="helper-silent" \
  --target="persona:helper:cognition:gating" \
  --condition="shouldRespond == false" \
  --mode="once" \
  --capture="reason,confidence,ragContextSummary"

# "Are we hitting rate limits?"
probe/create --name="rate-limit-hits" \
  --target="persona:*:ratelimiter:blocked" \
  --mode="accumulate" \
  --capture="personaId,roomId,waitTimeSeconds"

# "What's the inference slot denial rate?"
probe/create --name="slot-denials" \
  --target="coordination:inference:denied" \
  --mode="duration:60s" \
  --capture="personaId,reason"
```

### Status

**Not implemented** - Requires proper concurrent architecture underneath. The logging system handles the "firehose when needed" case. Probes are for surgical debugging of specific questions.

---

## Architecture Principles

1. **Segregation by identity** - Each persona's logs are isolated
2. **Segregation by concern** - Subsystems have dedicated log files
3. **Opt-in by default** - Logging is OFF until explicitly enabled
4. **Hot reload** - Config changes take effect without restart
5. **Graceful degradation** - Log failures never crash the system
6. **Event-driven** - Real-time streaming for UI integration
7. **Single source of truth** - LoggingConfig is the authority

---

## Related Documents

- [LOGGING.md](LOGGING.md) - Detailed logging implementation guide
- [LOGGING-PATHS-DESIGN.md](LOGGING-PATHS-DESIGN.md) - Path resolution design
- [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) - Sentinel system design
- [PERSONA-BEING-ARCHITECTURE.md](PERSONA-BEING-ARCHITECTURE.md) - Cognition subsystems

---

**Document version**: 1.0 (February 2026)
**Covers**: System logs, Cognition logs, Sentinel logs, Unified controls
