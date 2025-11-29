# Logging Paths Design - Single Source of Truth

**Status**: DESIGN DOCUMENT - Implementation NOT started
**Created**: 2025-11-29
**Purpose**: Define EXACTLY where every log file belongs and why

---

## Current State (What Actually Exists)

```
.continuum/
├── jtag/
│   ├── system/logs/
│   │   ├── npm-start.log          # Everything floods here (SQL, tools, system)
│   │   └── cognition.log
│   └── personas/{name}/logs/       # WRONG - cognitive logs at wrong location
│       └── *.log
└── personas/{name-id}/             # Correct directories exist but empty logs/
    └── logs/                       # Empty or old migrated files
```

**Problems:**
1. `npm-start.log` flooded with SQL queries and tool execution
2. Persona logs at `.continuum/jtag/personas/{name}/logs/` (wrong - missing UUID in path)
3. Should be at `.continuum/personas/{name-id}/logs/` (correct directories exist)
4. No separate log files for SQL, tools, errors

---

## The Problem

---

## The Solution: Two Log Categories

### 1. Infrastructure Logs (System-Wide)
**Location**: `.continuum/jtag/logs/`
**Purpose**: System operations that aren't persona-specific
**Examples**: SQL queries, tool execution errors, system initialization

| Category | File Path | What Goes Here | Used By |
|----------|-----------|----------------|---------|
| SQL | `.continuum/jtag/logs/sql.log` | All database queries, connection management | SqliteStorageAdapter, SqliteQueryExecutor, SqliteWriteManager |
| Tools | `.continuum/jtag/logs/tools.log` | Tool execution (success/failure, params, results) | PersonaToolExecutor |
| System | `.continuum/jtag/logs/system.log` | System initialization, daemon startup, process management | DaemonManager, ProcessRegistry |
| Errors | `.continuum/jtag/logs/errors.log` | All ERROR level logs from any source | Logger (error sink) |

**Key Property**: These logs are INFRASTRUCTURE - they exist regardless of which personas are active.

---

### 2. Persona Cognitive Logs (Per-Persona)
**Location**: `.continuum/personas/{name-id}/logs/`
**Purpose**: Track cognitive processes for a specific persona (their "mind")
**Examples**: Thought generation, action selection, state changes

| Subsystem | File Path | What Goes Here | Used By |
|-----------|-----------|----------------|---------|
| Mind | `.continuum/personas/{name-id}/logs/mind.log` | Thought generation, decision-making, cognitive cycles | PersonaMind |
| Body | `.continuum/personas/{name-id}/logs/body.log` | Action execution, tool usage, physical interactions | PersonaBody |
| Soul | `.continuum/personas/{name-id}/logs/soul.log` | Goal selection, value alignment, motivations | PersonaSoul |
| CNS | `.continuum/personas/{name-id}/logs/cns.log` | Coordination, state transitions, system integration | PersonaCNS |
| Tools | `.continuum/personas/{name-id}/logs/tools.log` | Tool execution by this persona (params, results, errors) | PersonaToolExecutor |

**Key Property**: These logs are PERSONA-SPECIFIC - each persona has its own set tracking its cognitive processes and tool usage.

**Directory Name Format**: `{name}-{shortId}` (e.g., `claude-assistant-79a5e548`)
- **Why**: Personas can share the same display name but must have unique directories
- **Where this is enforced**: PersonaUser creation (TODO: find this code)

---

### 3. Persona Debug Logs (Per-Persona)
**Location**: `.continuum/personas/{name-id}/logs/`
**Purpose**: Debug logs for subprocess wrappers and internal mechanics
**Examples**: Hippocampus subprocess stdio, PersonaLogger output

| Log Type | File Path | What Goes Here | Used By |
|----------|-----------|----------------|---------|
| Hippocampus | `.continuum/personas/{name-id}/logs/hippocampus.log` | Subprocess stdout/stderr | PersonaLogger (subprocess wrapper) |
| PersonaLogger | `.continuum/personas/{name-id}/logs/personalogger.log` | Subprocess management, crashes | PersonaLogger |

**Key Property**: These are DEBUGGING logs - same location as cognitive logs for simplicity.

**REMOVED**: Session timestamp nesting (`.../sessions/{timestamp}/logs/`) - meaningless complexity.

---

## Log File Lifecycle

**Strategy: Overwrite on restart**
- All log files opened in **write mode** (truncate existing content)
- Each `npm start` = fresh logs
- No timestamps in filenames, no rotation, no archiving (for now)

**Why:**
- Debugging = current behavior, not archaeology
- Simple: always know where logs are (`tail -f mind.log`)
- Clean directories (no buildup of old logs)
- Future: easy to add rotation/archiving as Logger feature if needed for training

**Exception:**
- `npm-start.log` may append (system-level, managed by npm scripts)

---

## SystemPaths Registry Pattern

**Current Problem**: SystemPaths constructs paths using `personaName` but directories use `{name}-{shortId}`.

### Required Changes to SystemPaths

**Key insight**: `UserEntity.uniqueId` already contains the correct format (`{name}-{shortId}`). Just use it directly!

```typescript
// BEFORE (WRONG):
logs: {
  personas: (personaName: string): string => {
    const safeName = personaName.replace(/[^a-zA-Z0-9-_]/g, '-').toLowerCase();
    return path.join(baseRoot, 'personas', safeName, 'logs');
    // ❌ Constructs ".continuum/personas/claude-assistant/logs/" (wrong!)
  }
}

// AFTER (CORRECT):
logs: {
  // Infrastructure logs (system-wide)
  root: path.join(baseRoot, 'jtag', 'logs'),
  sql: path.join(baseRoot, 'jtag', 'logs', 'sql.log'),
  tools: path.join(baseRoot, 'jtag', 'logs', 'tools.log'),
  system: path.join(baseRoot, 'jtag', 'logs', 'system.log'),
  errors: path.join(baseRoot, 'jtag', 'logs', 'errors.log'),

  // Persona cognitive logs - use uniqueId directly!
  personas: (uniqueId: string): string => {
    return path.join(baseRoot, 'personas', uniqueId, 'logs');
    // ✅ Uses ".continuum/personas/claude-assistant-79a5e548/logs/" (correct!)
  },

  subsystem: (uniqueId: string, subsystem: 'mind' | 'body' | 'soul' | 'cns'): string => {
    return path.join(baseRoot, 'personas', uniqueId, 'logs', `${subsystem}.log`);
  }
}

personas: {
  root: path.join(baseRoot, 'personas'),

  dir: (uniqueId: string): string => {
    return path.join(baseRoot, 'personas', uniqueId);
  },

  logs: (uniqueId: string): string => {
    return path.join(baseRoot, 'personas', uniqueId, 'logs');
  },

  state: (uniqueId: string): string => {
    return path.join(baseRoot, 'personas', uniqueId, 'state.db');
  },

  memory: (uniqueId: string): string => {
    return path.join(baseRoot, 'personas', uniqueId, 'memory.db');
  }
}
```

---

## SubsystemLogger Changes

**Current Problem**: SubsystemLogger calls `SystemPaths.logs.personas(this.personaName)` with display name.

### Required Changes

```typescript
// BEFORE (WRONG):
class SubsystemLogger {
  constructor(
    private subsystem: 'mind' | 'body' | 'soul' | 'cns',
    private personaName: string  // Just the display name!
  ) {}

  private getDefaultLogDir(): string {
    return SystemPaths.logs.personas(this.personaName);  // Missing UUID!
  }
}

// AFTER (CORRECT):
class SubsystemLogger {
  constructor(
    private subsystem: 'mind' | 'body' | 'soul' | 'cns',
    private personaId: string  // Full "{name}-{shortId}" format
  ) {}

  private getDefaultLogDir(): string {
    return SystemPaths.logs.personaDir(this.personaId);  // Correct path!
  }
}
```

---

## Migration Strategy

### Step 1: Update SystemPaths
**Task**: Change `personaName` parameters to `personaId`
**Files**: `system/core/config/SystemPaths.ts`

### Step 3: Update SubsystemLogger
**Task**: Pass `personaId` instead of `personaName` to constructor
**Files**: `system/user/server/modules/being/logging/SubsystemLogger.ts`

### Step 4: Update PersonaMind/Body/Soul/CNS
**Task**: Pass full persona ID when creating SubsystemLogger
**Files**: `system/user/server/modules/being/PersonaMind.ts` (and Body/Soul/CNS)

### Step 7: Deploy and Test
```bash
npm start  # Deploy changes
sleep 130  # Wait for deployment

# Send test message to trigger logging
./jtag chat/send --room="general" --message="Test log paths"

# Check ALL logs appear in correct location (one flat directory)
ls -la .continuum/personas/claude-assistant-79a5e548/logs/
# Should show:
#   mind.log, body.log, soul.log, cns.log (cognitive)
#   tools.log (tool execution by this persona)
#   hippocampus.log, personalogger.log (debug)

# Verify infrastructure logs
ls -la .continuum/jtag/system/logs/
# Should show: sql.log, tools.log, errors.log (new)
# Should show: npm-start.log (existing, but cleaner)
```

---

## File Tree (Target Structure)

```
.continuum/
├── jtag/                                   # Infrastructure (system-wide)
│   ├── data/
│   │   ├── database.sqlite                 # Main database
│   │   └── *.db                            # Other databases
│   ├── backups/
│   │   └── database-backup-*.sqlite        # Database backups
│   ├── system/
│   │   └── logs/
│   │       ├── npm-start.log               # System initialization (keep this)
│   │       ├── sql.log                     # Database operations (NEW)
│   │       ├── tools.log                   # Tool execution (NEW)
│   │       └── errors.log                  # All errors (NEW)
│   ├── sessions/
│   │   └── system/
│   ├── registry/
│   │   └── process-registry.json
│   └── signals/
│       └── system-ready-*.json
│
├── personas/                               # Persona-specific data
│   ├── claude-assistant-79a5e548/          # Format: {name}-{shortId}
│   │   ├── logs/                           # ALL logs (cognitive + tools + debug)
│   │   │   ├── mind.log                    # Thought generation
│   │   │   ├── body.log                    # Action execution
│   │   │   ├── soul.log                    # Goal selection
│   │   │   ├── cns.log                     # Coordination
│   │   │   ├── tools.log                   # Tool execution by this persona
│   │   │   ├── hippocampus.log             # Subprocess stdout/stderr
│   │   │   └── personalogger.log           # Subprocess management
│   │   ├── memory/
│   │   │   └── hippocampus.db
│   │   └── state.db
│   ├── helper-ai-154ee833/
│   │   └── ...
│   └── ...
│
└── genome/                                 # LoRA adapters (shared)
    ├── adapters/
    └── training/
```

---

## Testing Checklist

- [ ] Infrastructure logs appear in `.continuum/jtag/logs/`
- [ ] SQL operations log to `sql.log`
- [ ] Tool execution logs to `tools.log`
- [ ] Persona cognitive logs appear in `.continuum/personas/{name-id}/logs/`
- [ ] Each subsystem (mind/body/soul/cns) has its own file
- [ ] Directory names use `{name}-{shortId}` format
- [ ] No logs appearing in old location (`.continuum/jtag/personas/`)
- [ ] Fresh `npm start` creates correct directory structure
- [ ] Multiple personas each get their own log directories

---

## .gitignore Rules

```gitignore
# Infrastructure logs (sql, tools, errors in system/logs/)
.continuum/jtag/system/logs/*.log

# Persona logs (cognitive + debug, all in one place)
.continuum/personas/*/logs/

# Remove old session-nested logs exclusion (no longer created)
# .continuum/personas/*/sessions/*/logs/  # DELETED

# Temporary files
/tmp/
*.pyc
__pycache__/
```

**Why exclude from git:**
- Logs are runtime artifacts (like .pyc files)
- Can be gigabytes for long-running systems
- Not needed for reproducibility
- Each deployment generates fresh logs

---

## Next Steps

1. **Find persona ID generation** - Where is `{name}-{shortId}` format created?
2. **Update SystemPaths** - Change APIs to use `personaId` not `personaName`
3. **Update SubsystemLogger** - Accept `personaId` in constructor
4. **Update PersonaMind/Body/Soul/CNS** - Pass full persona ID
5. **Migrate logs** - Move active logs to correct location
6. **Test** - Verify logs appear in correct places after `npm start`
7. **Document** - Update CLAUDE.md with final structure
