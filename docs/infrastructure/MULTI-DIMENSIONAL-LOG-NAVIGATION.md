# Multi-Dimensional Log Navigation Design

## Context

After implementing `docs/read --toc` feature based on AI team feedback, the AIs expanded the concept with a profound insight:

> **Docs have explicit structure (headers), logs have implicit structure (patterns).**

This document captures the AI team's design for extending the ToC concept to logs through multi-dimensional navigation.

## The Four Lenses

The AI team proposed `logs/read --analyze-structure` that returns multiple organizational views of log data:

### 1. Temporal Lens (When did things happen?)
Time-based segmentation of log entries:
- Hourly/daily chunks
- Specific time ranges
- Duration analysis

**Usage Example:**
```bash
./jtag logs/read --log="system/server" --section="10:00-10:05"
./jtag logs/read --log="system/server" --section="2025-12-02"
```

### 2. Severity Lens (What went wrong?)
Log level clustering:
- ERROR grouping
- WARNING grouping
- INFO/DEBUG filtering
- Severity trends over time

**Usage Example:**
```bash
./jtag logs/read --log="system/server" --section="ERROR"
./jtag logs/read --log="system/server" --section="WARNING"
```

### 3. Spatial Lens (Which components were involved?)
Component/module grouping:
- PersonaCoordinator activity
- DataDaemon operations
- AIProviderAdapter calls
- Cross-component interactions

**Usage Example:**
```bash
./jtag logs/read --log="system/server" --section="PersonaCoordinator"
./jtag logs/read --log="system/adapters" --section="AnthropicAdapter"
```

### 4. Narrative Lens (What story do these entries tell?)
**Semantic clustering** - the breakthrough insight:

Logs have natural "story arcs" that can be detected through:
- **Request traces**: Following a requestId through multiple components
- **Error chains**: exception → retry → fallback sequences
- **Deployment sequences**: start → migrate → validate → complete
- **User journeys**: Session lifecycle from connect to disconnect

**Usage Example:**
```bash
# Follow entire request lifecycle
./jtag logs/read --log="system/server" --section="request-abc123"

# Track error cascade
./jtag logs/read --log="system/server" --section="error-chain-xyz"

# View deployment story
./jtag logs/read --log="system/server" --section="deployment-5747"
```

## Implementation Design

### Phase 1: Structure Analysis Command

```bash
./jtag logs/read --log="system/server" --analyze-structure
```

**Returns:**
```json
{
  "success": true,
  "log": "system/server",
  "totalLines": 15420,
  "structure": {
    "temporal": [
      { "label": "10:00-10:15", "lines": [1, 234], "entryCount": 234 },
      { "label": "10:15-10:30", "lines": [235, 456], "entryCount": 222 }
    ],
    "severity": [
      { "label": "ERROR", "lines": [12, 45, 89, 234], "entryCount": 4 },
      { "label": "WARNING", "lines": [23, 67, 123], "entryCount": 3 }
    ],
    "spatial": [
      { "label": "PersonaCoordinator", "lines": [10, 34, 78], "entryCount": 3 },
      { "label": "DataDaemon", "lines": [15, 45, 90], "entryCount": 3 }
    ],
    "narrative": [
      {
        "label": "request-abc123",
        "type": "request-trace",
        "lines": [45, 67, 89, 102],
        "components": ["APIHandler", "DataDaemon", "PersonaCoordinator"],
        "duration": "1.2s",
        "outcome": "success"
      },
      {
        "label": "error-chain-xyz",
        "type": "error-cascade",
        "lines": [234, 235, 236, 240],
        "components": ["AIProviderAdapter", "RetryManager"],
        "duration": "450ms",
        "outcome": "fallback-success"
      }
    ]
  }
}
```

### Phase 2: Section Navigation

Once structure is analyzed, use `--section` to jump to specific views:

```bash
# Temporal navigation
./jtag logs/read --log="system/server" --section="10:00-10:05"

# Severity navigation
./jtag logs/read --log="system/server" --section="ERROR"

# Spatial navigation
./jtag logs/read --log="system/server" --section="PersonaCoordinator"

# Narrative navigation
./jtag logs/read --log="system/server" --section="request-abc123"
```

### Phase 3: Composable Navigation

Combine multiple lenses:

```bash
# All errors from PersonaCoordinator between 10:00-10:15
./jtag logs/read --log="system/server" \
  --section="ERROR" \
  --component="PersonaCoordinator" \
  --timeRange="10:00-10:15"

# Request trace showing only warnings
./jtag logs/read --log="system/server" \
  --section="request-abc123" \
  --severity="WARNING"
```

## Semantic Clustering Heuristics

### Simple Pattern Detection (Phase 1)

**1. Common Identifiers:**
- `requestId`, `sessionId`, `userId`, `messageId`
- Regex patterns: `/request-[a-f0-9-]+/`, `/session-[a-f0-9-]+/`
- Group all entries with same identifier

**2. Temporal Proximity:**
- Entries within same 100ms window with similar patterns
- Example: "Starting X" followed by "Completed X" within 2 seconds

**3. Error→Retry→Success Sequences:**
- ERROR log followed by WARN "Retrying..." followed by INFO "Success"
- Track retry counts and final outcome

**4. Natural Delimiters:**
- `=== Session Start ===`
- `--- Deployment Begin ---`
- `### Component Initialized ###`

### Advanced Pattern Detection (Phase 2)

**1. Cross-Component Request Tracing:**
```
APIHandler: Received request abc123
DataDaemon: Query for request abc123
PersonaCoordinator: Processing request abc123
AIProviderAdapter: LLM call for request abc123
APIHandler: Responding to request abc123
```

**2. Error Cascade Detection:**
```
AIProviderAdapter: ERROR - Rate limit exceeded
RetryManager: WARNING - Retry attempt 1/3
AIProviderAdapter: ERROR - Rate limit exceeded
RetryManager: WARNING - Retry attempt 2/3
FallbackManager: INFO - Using fallback provider
AIProviderAdapter: INFO - Fallback success
```

**3. Deployment Sequence Detection:**
```
Deployment: Starting deployment 5747
Migration: Running database migrations
Migration: Applied 3 migrations successfully
Validation: Running health checks
Validation: All checks passed
Deployment: Deployment 5747 complete
```

## Benefits for AI Agents

### Current State: Linear Log Reading
AI agents see logs as chronological entries - hard to extract meaning.

### Future State: Multi-Dimensional Navigation
AI agents can:
1. **Build context efficiently**: "Show me what PersonaCoordinator did during this error"
2. **Follow narratives**: "Trace this request from start to finish"
3. **Detect patterns**: "Are there any error cascades in the last hour?"
4. **Debug intelligently**: "Show me all errors related to this user session"

### Example AI Agent Workflow

```typescript
// AI agent investigating a bug report
async investigateBug(errorMessage: string, timestamp: string) {
  // 1. Find temporal context
  const timeRange = `${timestamp - 5m}-${timestamp + 5m}`;

  // 2. Get structure analysis
  const structure = await Commands.execute('logs/read', {
    log: 'system/server',
    analyzeStructure: true
  });

  // 3. Find related error chains
  const errorChains = structure.narrative.filter(n =>
    n.type === 'error-cascade' &&
    n.lines.some(l => l >= timeRange.start && l <= timeRange.end)
  );

  // 4. Read full narrative
  for (const chain of errorChains) {
    const narrative = await Commands.execute('logs/read', {
      log: 'system/server',
      section: chain.label
    });

    // AI now has full story of what happened
    await this.analyzeErrorChain(narrative);
  }
}
```

## Implementation Phases

### ✅ Phase 0: ToC for Docs (COMPLETED)
- `docs/read --toc` returns hierarchical structure
- `docs/read --section="Name"` jumps to section
- Composable: can combine flags

### 🚧 Phase 1: Basic Structure Analysis (NEXT)
- Implement `logs/read --analyze-structure`
- Temporal lens (time-based chunks)
- Severity lens (log level grouping)
- Spatial lens (component grouping)

### 📋 Phase 2: Simple Semantic Clustering
- Detect common identifiers (requestId, sessionId)
- Group by temporal proximity
- Identify error→retry→success sequences
- Support natural delimiters

### 📋 Phase 3: Advanced Narrative Detection
- Cross-component request tracing
- Error cascade analysis
- Deployment sequence detection
- User journey tracking

### 📋 Phase 4: Composable Navigation
- Combine multiple lenses
- Filter by multiple criteria
- AI-friendly output format

## AI Team Feedback (12/2/2025)

### Consensus from Together, Groq, DeepSeek, Claude:

1. **Output format**:
   - **JSON for programmatic use** (primary)
   - Optional `--format=markdown` for human readability
   - AI agents are primary consumers, so default to JSON

2. **Performance**:
   - **YES to caching!** Structure analysis should be lazy
   - Cache key: `{logFile, lastModifiedTime}`
   - Invalidate cache when file changes
   - Configurable TTL

3. **Pattern customization**:
   - **Start simple, add later**
   - Phase 1: Built-in patterns (request/session/error chains)
   - Phase 2: Custom patterns via regex or simple DSL in config

4. **Cross-log navigation**:
   - **THE KILLER FEATURE** - critical for production debugging
   - Start with single-file, but **design for multi-file from day one**
   - Use `logGroup` concept for related logs
   - Example: Following request from `browser.log` → `server.log` → `adapters.log`

5. **Threshold tuning**:
   - **Configurable with sensible defaults**
   - Temporal proximity: **500ms-1s default** (via `--cluster-window`)
   - Minimum cluster size: 3 entries (avoid noise)
   - Max time gap within cluster: 5s (break if silence too long)

6. **Priority**:
   - **START WITH BASIC STRUCTURE ANALYSIS** (temporal/severity/spatial)
   - Get foundation working and cached
   - Add semantic clustering in Phase 2
   - **Ship basic version fast, iterate on semantics**

### Key Quotes:

> "JSON for programmatic use, but with a --format=markdown option for human readability. Default to JSON since AI agents will be the primary consumers." - Claude

> "Absolutely cache it! Structure analysis should be lazy - only run when requested, then cache results keyed by {logFile, lastModifiedTime}." - Claude

> "Cross-log navigation: This is the killer feature! Start with single-file, but design the data structure to support cross-file from day one." - Claude

> "For temporal proximity, start with 1s as default but make it configurable. This gives us a solid MVP that's immediately useful while leaving room for the more complex features." - DeepSeek

> "I'd prioritize in this order: JSON output, basic structure analysis first, add caching with configurable TTL, semantic clustering as advanced feature, cross-log navigation and custom patterns as stretch goals." - DeepSeek

## References

**AI Team Discussion**: Chat export from 12/2/2025 11:13-11:14 PM

**Key Contributors:**
- **Claude Assistant**: Introduced "implicit vs explicit structure" insight
- **DeepSeek Assistant**: Proposed `--analyze-structure` command
- **Groq Lightning**: Emphasized unified navigation approach
- **Together Assistant**: Highlighted consistency benefits

**Key Quotes:**

> "The key insight: docs have explicit structure (headers), logs have implicit structure (patterns). The ToC feature should embrace both!" - Claude

> "Semantic clustering would make logs tell stories rather than just being chronological entries." - DeepSeek

> "The four-lens approach (temporal, severity, spatial, narrative) would give AI agents a complete picture of system behavior." - DeepSeek

## Next Steps

1. ✅ Document AI team's vision (this file)
2. Ask AI team for clarification on questions above
3. Implement Phase 1 (basic structure analysis)
4. Test with AI team and iterate
5. Gradually add semantic clustering capabilities

---

**Document Status**: Design phase - awaiting AI team feedback before implementation

**Last Updated**: 2025-12-02

**Related Docs**:
- `docs/LOGGING.md` - Current logging architecture
- `commands/logs/read/shared/LogsReadTypes.ts` - Existing logs/read implementation
- `commands/docs/read/shared/DocsReadTypes.ts` - ToC implementation for docs
