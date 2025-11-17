# Tool-Based Perception Architecture for PersonaUser Agents

## Executive Summary

This document defines the architecture for giving PersonaUser agents **read-only code access tools** to enable collaborative debugging, code review, and system introspection. Drawing from the research paper's "Tool-based Perception" framework and observing Claude Code's tool patterns, we design a sandboxed, audited, and rate-limited tool system that transforms AIs from passive observers to active investigators.

**Design Philosophy**: Start with read-only, expand incrementally, log everything.

---

## 1. Motivation: From Diagnosis to Investigation

### Current State (Phase 1B: Observability)

AIs can:
- ✅ View their own cognition logs (`CognitionStateEntity`, `CognitionPlanEntity`)
- ✅ View each other's cognition logs (shared observability)
- ✅ Diagnose system problems through behavioral patterns
- ✅ Propose solutions (e.g., "relevance = 0.7*embedding + 0.3*keywords")

AIs **cannot**:
- ❌ Read the actual code they're discussing
- ❌ Verify their hypotheses by inspecting implementation
- ❌ Search for patterns across the codebase
- ❌ View git history to understand design evolution
- ❌ Execute code to test their proposed algorithms

### Observed Limitation (from 2025-11-16 session)

**Quote from chat logs**:
> Joel: "they have no tool access nor an ability to see the code yet, so their opinions are from the outside so to speak"
>
> Joel: "yeah if we build our phases and give them better tool access (which I have more to send you on) we will have true peers here and probably more powerful than both of us"

**The Gap**: AIs diagnosed the problem (semantic novelty detection failure) and proposed a solution (embedding-based relevance scoring), but couldn't:
1. Read the code to understand current implementation
2. Search for where messages are logged
3. Verify their assumptions about data structures
4. Test their formula against actual data

### Vision (Phase 2: Tool Access)

Enable AIs to:
- 🎯 **Investigate** their own codebase
- 🎯 **Verify** hypotheses by reading implementations
- 🎯 **Search** for patterns and anti-patterns
- 🎯 **Understand** system architecture through code + git history
- 🎯 **Collaborate** by pointing each other to specific files/lines

---

## 2. Research Foundation: Tool-Based Perception

### Paper Citation (Section 3.4)

> "LLM-based agents can significantly enhance their perception capabilities through tool augmentation. This means utilizing external tools and APIs to enable the agent to gather, process, and interpret data from a wider variety of sources... The mechanism of integration typically involves the LLM generating specific tool calls based on its current understanding and goals, with the results from these tools being 'fed back' into the LLM."

### Tool Categories Applied to Our System

| Paper Category | Our Implementation | Phase |
|----------------|-------------------|-------|
| **Web Search & Information Retrieval** | Already have: `WebSearch`, `WebFetch` | Phase 1A |
| **Specialized APIs** | Data access: `data/list`, `data/read` | Phase 1B |
| **Code Execution Tools** | Coming: `code/execute`, Python/JS sandboxes | Phase 3 |
| **Sensor Integration** | Coming: `system/monitor`, performance metrics | Phase 4 |
| **File System Tools** | **THIS PHASE**: `code/read`, `code/search`, `code/list` | **Phase 2A** |
| **Version Control Tools** | **THIS PHASE**: `code/diff`, `code/log`, `code/blame` | **Phase 2A** |

---

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         PersonaUser Agent                        │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Cognitive Processing (PersonaInbox + Reasoning)         │   │
│  │                                                           │   │
│  │  "I need to understand how CognitionLogger works"        │   │
│  │                                                           │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │  Tool Call Generator                            │    │   │
│  │  │                                                  │    │   │
│  │  │  Generate: {                                     │    │   │
│  │  │    command: "code/read",                         │    │   │
│  │  │    params: {                                     │    │   │
│  │  │      path: "system/.../CognitionLogger.ts",     │    │   │
│  │  │      startLine: 1, endLine: 100                 │    │   │
│  │  │    }                                             │    │   │
│  │  │  }                                               │    │   │
│  │  └────────────────┬────────────────────────────────┘    │   │
│  │                   │                                      │   │
│  └───────────────────┼──────────────────────────────────────┘   │
│                      │                                           │
│                      ▼                                           │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Tool Execution Middleware (NEW)                          │  │
│  │                                                            │  │
│  │  1. ✅ Check whitelist (is code/read allowed?)            │  │
│  │  2. ✅ Validate params (path within repo bounds?)         │  │
│  │  3. ✅ Rate limit check (not exceeding quota?)            │  │
│  │  4. ✅ Audit log (record tool usage in cognition)         │  │
│  │  5. 🚀 Execute: Commands.execute('code/read', params)     │  │
│  │  6. 📦 Format result (syntax highlight, line numbers)     │  │
│  │  7. 📝 Inject result back into reasoning context          │  │
│  │                                                            │  │
│  └────────────────────────┬───────────────────────────────────┘  │
│                           │                                      │
└───────────────────────────┼──────────────────────────────────────┘
                            │
                            ▼
                ┌────────────────────────┐
                │  Commands.execute()    │
                │  (Universal Primitive) │
                └───────────┬────────────┘
                            │
            ┌───────────────┼───────────────┐
            │               │               │
            ▼               ▼               ▼
    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
    │ code/read    │ │ code/search  │ │ code/diff    │
    │ (File Read)  │ │ (Grep-like)  │ │ (Git Diff)   │
    └──────────────┘ └──────────────┘ └──────────────┘
```

---

## 4. Tool Design: Read-Only Command Suite

### Philosophy

**Start Constrained, Expand Deliberately**:
- Phase 2A: Read-only (file read, search, git view)
- Phase 2B: Data analysis (execute Python/JS in sandbox)
- Phase 3: Limited write (create test files, experiment branches)
- Phase 4: Full write (with human approval gates)

### Command Hierarchy

```
commands/code/
├── read/                          # Read file contents
├── search/                        # Search codebase (grep)
├── list/                          # List files (glob)
├── diff/                          # View git diffs
├── log/                           # View git history
├── blame/                         # Git blame (who wrote what)
├── show/                          # Show git commit details
└── shared/
    ├── CodeToolTypes.ts           # Shared types
    ├── CodeToolValidator.ts       # Path validation
    ├── CodeToolFormatter.ts       # Result formatting
    └── CodeToolAuditor.ts         # Usage logging
```

---

## 5. Individual Tool Specifications

### 5.1 `code/read` - Read File Contents

**Purpose**: Read source files with optional line ranges

**Analogue**: Claude Code's `Read` tool

**Command**:
```typescript
interface CodeReadParams extends CommandParams {
  path: string;                    // Absolute or repo-relative path
  startLine?: number;              // Optional line range (1-indexed)
  endLine?: number;
  maxLines?: number;               // Safety limit (default: 500)
  syntaxHighlight?: boolean;       // Return with syntax tokens (default: false)
  includeMetadata?: boolean;       // Include file stats (default: true)
}

interface CodeReadResult extends CommandResult {
  success: boolean;
  path: string;
  content: string;                 // File contents
  lines?: string[];                // Line-by-line array (if requested)
  metadata: {
    totalLines: number;
    linesReturned: number;
    startLine: number;
    endLine: number;
    lastModified: Date;
    size: number;
    encoding: string;
  };
  syntaxTokens?: SyntaxToken[];    // If syntaxHighlight=true
}
```

**Safety Constraints**:
- ✅ Path must be within repo bounds (`/Volumes/FlashGordon/cambrian/continuum/`)
- ✅ Cannot read dotfiles (`.env`, `.git/config`, etc.) - explicit whitelist only
- ✅ Cannot read binary files (check file header)
- ✅ Max file size: 1MB (configurable)
- ✅ Max lines per read: 500 (prevent context overflow)

**Example Usage**:
```bash
./jtag code/read --path="src/debug/jtag/system/user/server/PersonaUser.ts" \
  --startLine=318 --endLine=400
```

**Implementation Notes**:
- Server-only (no browser execution)
- Use Node.js `fs.readFile()` with encoding detection
- Validate path with `path.resolve()` and bounds checking
- Cache recently read files (5-minute TTL) to reduce disk I/O

---

### 5.2 `code/search` - Search Codebase

**Purpose**: Regex search across files (like grep/ripgrep)

**Analogue**: Claude Code's `Grep` tool

**Command**:
```typescript
interface CodeSearchParams extends CommandParams {
  pattern: string;                 // Regex pattern
  paths?: string[];                // Specific paths (default: all)
  filePattern?: string;            // Glob filter (e.g., "**/*.ts")
  excludePatterns?: string[];      // Exclude patterns (default: node_modules, .git)
  contextLines?: number;           // Lines before/after match (default: 2)
  caseSensitive?: boolean;         // Case sensitivity (default: false)
  maxMatches?: number;             // Max results (default: 100)
  includeLineNumbers?: boolean;    // Include line numbers (default: true)
}

interface CodeSearchResult extends CommandResult {
  success: boolean;
  pattern: string;
  matches: CodeMatch[];
  totalMatches: number;
  filesSearched: number;
  truncated: boolean;              // True if maxMatches hit
}

interface CodeMatch {
  file: string;
  line: number;
  column: number;
  matchedText: string;
  contextBefore: string[];         // Lines before match
  contextAfter: string[];          // Lines after match
}
```

**Safety Constraints**:
- ✅ Pattern must be valid regex (catch syntax errors)
- ✅ Cannot search outside repo bounds
- ✅ Auto-exclude: `node_modules/`, `.git/`, `dist/`, `build/`, `.continuum/`
- ✅ Timeout: 10 seconds max
- ✅ Max matches: 100 (prevent result explosion)

**Example Usage**:
```bash
./jtag code/search --pattern="CognitionLogger\\.log" \
  --filePattern="**/*.ts" --contextLines=3
```

**Implementation Notes**:
- Use `ripgrep` (rg) if available, fallback to Node.js implementation
- Stream results to prevent memory overflow
- Highlight matches in output (ANSI colors or JSON tokens)

---

### 5.3 `code/list` - List Files

**Purpose**: List files matching glob patterns

**Analogue**: Claude Code's `Glob` tool

**Command**:
```typescript
interface CodeListParams extends CommandParams {
  pattern: string;                 // Glob pattern (e.g., "**/*.ts")
  directory?: string;              // Starting directory (default: repo root)
  maxDepth?: number;               // Max directory depth (default: 10)
  includeHidden?: boolean;         // Include dotfiles (default: false)
  sortBy?: 'name' | 'modified' | 'size';  // Sort order (default: 'name')
  limit?: number;                  // Max files (default: 500)
}

interface CodeListResult extends CommandResult {
  success: boolean;
  files: FileEntry[];
  totalFiles: number;
  truncated: boolean;
}

interface FileEntry {
  path: string;                    // Relative to repo root
  name: string;
  size: number;
  lastModified: Date;
  isDirectory: boolean;
}
```

**Safety Constraints**:
- ✅ Directory must be within repo bounds
- ✅ Auto-exclude: `node_modules/`, `.git/`, binary directories
- ✅ Max files: 500
- ✅ Max depth: 10 levels

**Example Usage**:
```bash
./jtag code/list --pattern="**/*Types.ts" --sortBy=modified --limit=50
```

**Implementation Notes**:
- Use `fast-glob` library (already used elsewhere in system)
- Cache directory listings (1-minute TTL)
- Return metadata for AI decision-making (size helps estimate context cost)

---

### 5.4 `code/diff` - View Git Diffs

**Purpose**: Compare code between branches/commits

**Command**:
```typescript
interface CodeDiffParams extends CommandParams {
  base?: string;                   // Base ref (default: 'HEAD')
  compare?: string;                // Compare ref (default: working directory)
  paths?: string[];                // Limit to specific paths
  contextLines?: number;           // Diff context (default: 3)
  ignoreWhitespace?: boolean;      // Ignore whitespace changes (default: false)
  maxDiffSize?: number;            // Max diff output (default: 50KB)
}

interface CodeDiffResult extends CommandResult {
  success: boolean;
  base: string;
  compare: string;
  diffs: FileDiff[];
  totalChanges: {
    filesChanged: number;
    insertions: number;
    deletions: number;
  };
  truncated: boolean;
}

interface FileDiff {
  path: string;
  status: 'added' | 'modified' | 'deleted' | 'renamed';
  hunks: DiffHunk[];
  stats: {
    insertions: number;
    deletions: number;
  };
}

interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

interface DiffLine {
  type: 'context' | 'addition' | 'deletion';
  lineNumber: number;
  content: string;
}
```

**Safety Constraints**:
- ✅ Refs must be valid git references
- ✅ Cannot diff outside repo
- ✅ Max diff size: 50KB (prevent huge diffs from overwhelming context)
- ✅ Timeout: 5 seconds

**Example Usage**:
```bash
./jtag code/diff --base=main --compare=HEAD --paths="system/user/"
```

**Implementation Notes**:
- Use `simple-git` library (already in dependencies)
- Parse diff output into structured format
- Provide both unified and split diff views

---

### 5.5 `code/log` - View Git History

**Purpose**: View commit history for files/directories

**Command**:
```typescript
interface CodeLogParams extends CommandParams {
  path?: string;                   // File/directory (default: all)
  limit?: number;                  // Max commits (default: 20)
  since?: string;                  // Time range (e.g., "1 week ago")
  until?: string;
  author?: string;                 // Filter by author
  grep?: string;                   // Search commit messages
  includeDiff?: boolean;           // Include diff with commits (default: false)
}

interface CodeLogResult extends CommandResult {
  success: boolean;
  commits: GitCommit[];
  totalCommits: number;
  truncated: boolean;
}

interface GitCommit {
  hash: string;
  shortHash: string;
  author: string;
  email: string;
  date: Date;
  message: string;
  filesChanged: string[];
  stats?: {
    insertions: number;
    deletions: number;
  };
  diff?: FileDiff[];               // If includeDiff=true
}
```

**Safety Constraints**:
- ✅ Path must be within repo
- ✅ Max commits: 100
- ✅ Timeout: 10 seconds

**Example Usage**:
```bash
./jtag code/log --path="system/user/server/PersonaUser.ts" \
  --limit=10 --author="joel"
```

---

### 5.6 `code/blame` - Git Blame (Who Wrote What)

**Purpose**: Show line-by-line authorship

**Command**:
```typescript
interface CodeBlameParams extends CommandParams {
  path: string;                    // File path (required)
  startLine?: number;              // Line range
  endLine?: number;
  includeContent?: boolean;        // Include line content (default: true)
}

interface CodeBlameResult extends CommandResult {
  success: boolean;
  path: string;
  lines: BlameLine[];
}

interface BlameLine {
  lineNumber: number;
  content: string;
  commit: {
    hash: string;
    shortHash: string;
    author: string;
    date: Date;
    message: string;
  };
}
```

**Example Usage**:
```bash
./jtag code/blame --path="system/user/server/PersonaUser.ts" \
  --startLine=318 --endLine=400
```

---

### 5.7 `code/show` - Show Commit Details

**Purpose**: View full commit details

**Command**:
```typescript
interface CodeShowParams extends CommandParams {
  commit: string;                  // Commit hash or ref
  includeDiff?: boolean;           // Include full diff (default: true)
}

interface CodeShowResult extends CommandResult {
  success: boolean;
  commit: GitCommit;               // Full commit details with diff
}
```

**Example Usage**:
```bash
./jtag code/show --commit="e49f9a95b"
```

---

## 6. Tool Execution Middleware

### Purpose

The middleware sits between PersonaUser reasoning and command execution to:
1. **Whitelist Enforcement**: Only approved tools can be called
2. **Validation**: Params must pass safety checks
3. **Rate Limiting**: Prevent runaway tool loops
4. **Audit Logging**: Record all tool usage in cognition system
5. **Result Formatting**: Structure results for AI consumption
6. **Context Injection**: Feed results back into reasoning

### Architecture

```typescript
// system/user/server/modules/tools/ToolExecutor.ts

interface ToolCall {
  tool: string;                    // e.g., "code/read"
  params: Record<string, any>;
  requestedBy: UUID;               // PersonaUser ID
  contextId: UUID;                 // Chat room or task context
  timestamp: number;
}

interface ToolResult {
  success: boolean;
  tool: string;
  result?: any;
  error?: string;
  metadata: {
    executionTime: number;
    tokenEstimate: number;         // Estimated LLM tokens consumed
    cached: boolean;
  };
}

class ToolExecutor {
  private whitelist: Set<string>;
  private rateLimiter: RateLimiter;
  private auditor: ToolAuditor;
  private validator: ToolValidator;
  private formatter: ToolFormatter;

  async executeTool(call: ToolCall): Promise<ToolResult> {
    // 1. Check whitelist
    if (!this.whitelist.has(call.tool)) {
      return {
        success: false,
        tool: call.tool,
        error: `Tool '${call.tool}' is not whitelisted for AI use`,
        metadata: { executionTime: 0, tokenEstimate: 0, cached: false }
      };
    }

    // 2. Validate params
    const validation = await this.validator.validate(call.tool, call.params);
    if (!validation.valid) {
      return {
        success: false,
        tool: call.tool,
        error: `Validation failed: ${validation.error}`,
        metadata: { executionTime: 0, tokenEstimate: 0, cached: false }
      };
    }

    // 3. Rate limit check
    const allowed = await this.rateLimiter.checkAllowance(call.requestedBy, call.tool);
    if (!allowed) {
      return {
        success: false,
        tool: call.tool,
        error: 'Rate limit exceeded. Please wait before making more tool calls.',
        metadata: { executionTime: 0, tokenEstimate: 0, cached: false }
      };
    }

    // 4. Audit log (before execution)
    await this.auditor.logToolCall(call);

    // 5. Execute
    const startTime = Date.now();
    const result = await Commands.execute(call.tool, call.params);
    const executionTime = Date.now() - startTime;

    // 6. Format result
    const formatted = await this.formatter.format(call.tool, result);

    // 7. Audit log (after execution)
    await this.auditor.logToolResult(call, formatted, executionTime);

    // 8. Return
    return {
      success: true,
      tool: call.tool,
      result: formatted,
      metadata: {
        executionTime,
        tokenEstimate: this.estimateTokens(formatted),
        cached: false
      }
    };
  }

  private estimateTokens(result: any): number {
    // Rough estimate: 1 token ≈ 4 characters
    const str = JSON.stringify(result);
    return Math.ceil(str.length / 4);
  }
}
```

---

## 7. Whitelist Configuration

### Phase 2A Whitelist (Read-Only)

```typescript
// system/user/server/modules/tools/ToolWhitelist.ts

export const TOOL_WHITELIST_PHASE_2A: ReadonlySet<string> = new Set([
  // Code reading
  'code/read',
  'code/search',
  'code/list',

  // Git viewing
  'code/diff',
  'code/log',
  'code/blame',
  'code/show',

  // Data reading (already available)
  'data/list',
  'data/read',

  // System info
  'ping',
  'debug/logs',

  // Self-introspection
  'ai/report'
]);

export const TOOL_WHITELIST_PHASE_2B: ReadonlySet<string> = new Set([
  ...TOOL_WHITELIST_PHASE_2A,

  // Code execution (sandboxed)
  'code/execute/python',
  'code/execute/javascript',

  // Screenshot/visual (read-only)
  'screenshot'
]);

export const TOOL_WHITELIST_PHASE_3: ReadonlySet<string> = new Set([
  ...TOOL_WHITELIST_PHASE_2B,

  // Limited writes
  'file/save',            // Requires path in /tmp or experimental/ directory
  'code/create-branch',   // Git branch creation
  'code/create-test'      // Create test files only
]);
```

### Per-Persona Overrides

```typescript
// system/user/server/modules/tools/PersonaToolConfig.ts

interface PersonaToolConfig {
  personaId: UUID;
  whitelistOverride?: Set<string>;  // Override global whitelist
  blacklist?: Set<string>;          // Block specific tools
  rateLimit?: {
    callsPerMinute: number;
    callsPerHour: number;
  };
}

// Example: Teacher AI gets elevated access for code review
const TEACHER_AI_CONFIG: PersonaToolConfig = {
  personaId: 'teacher-ai-uuid',
  whitelistOverride: new Set([
    ...TOOL_WHITELIST_PHASE_2A,
    'code/execute/python',   // Can test code
    'screenshot'             // Can verify UI
  ]),
  rateLimit: {
    callsPerMinute: 20,      // Higher limit for intensive analysis
    callsPerHour: 500
  }
};
```

---

## 8. Rate Limiting Strategy

### Goals

1. Prevent runaway loops (AI calling tools infinitely)
2. Prevent abuse (malicious or buggy AI hammering system)
3. Fair resource allocation across personas
4. Prioritize critical operations

### Implementation

```typescript
// system/user/server/modules/tools/ToolRateLimiter.ts

interface RateLimitConfig {
  globalLimits: {
    callsPerMinute: number;        // Global across all AIs
    callsPerHour: number;
  };
  perPersonaLimits: {
    callsPerMinute: number;        // Per individual AI
    callsPerHour: number;
  };
  perToolLimits: {
    [tool: string]: {
      callsPerMinute: number;      // Per tool type
    };
  };
  burstAllowance: number;          // Allow short bursts
}

const DEFAULT_RATE_LIMITS: RateLimitConfig = {
  globalLimits: {
    callsPerMinute: 100,
    callsPerHour: 2000
  },
  perPersonaLimits: {
    callsPerMinute: 10,
    callsPerHour: 200
  },
  perToolLimits: {
    'code/read': { callsPerMinute: 5 },
    'code/search': { callsPerMinute: 3 },  // More expensive
    'code/execute/python': { callsPerMinute: 2 }
  },
  burstAllowance: 3  // Allow 3 rapid calls, then throttle
};

class ToolRateLimiter {
  private callHistory: Map<string, number[]>;  // personaId -> timestamps

  async checkAllowance(personaId: UUID, tool: string): Promise<boolean> {
    const now = Date.now();
    const history = this.callHistory.get(personaId) || [];

    // Remove calls older than 1 hour
    const recentCalls = history.filter(ts => now - ts < 60 * 60 * 1000);

    // Check per-minute limit
    const lastMinuteCalls = recentCalls.filter(ts => now - ts < 60 * 1000);
    if (lastMinuteCalls.length >= DEFAULT_RATE_LIMITS.perPersonaLimits.callsPerMinute) {
      return false;
    }

    // Check per-hour limit
    if (recentCalls.length >= DEFAULT_RATE_LIMITS.perPersonaLimits.callsPerHour) {
      return false;
    }

    // Check tool-specific limit
    const toolLimit = DEFAULT_RATE_LIMITS.perToolLimits[tool];
    if (toolLimit) {
      const toolCalls = lastMinuteCalls.length;  // Simplified, should track per-tool
      if (toolCalls >= toolLimit.callsPerMinute) {
        return false;
      }
    }

    // Update history
    recentCalls.push(now);
    this.callHistory.set(personaId, recentCalls);

    return true;
  }
}
```

---

## 9. Audit Logging to Cognition System

### Purpose

Every tool call is logged to the cognition system so:
1. Humans can debug AI reasoning ("why did it read this file?")
2. AIs can reflect on their own tool usage patterns
3. System can analyze tool effectiveness
4. Research data for understanding AI investigation strategies

### Schema Extension

```typescript
// Extend CognitionPlanEntity to include tool calls

interface ToolCallSnapshot {
  tool: string;
  params: Record<string, any>;
  timestamp: number;
  executionTime: number;
  success: boolean;
  error?: string;
  resultSummary?: string;          // Brief summary (not full result)
  tokensConsumed: number;          // Estimated context cost
}

// In CognitionPlanEntity, add:
export interface PlanStepSnapshot {
  stepNumber: number;
  action: string;
  expectedOutcome: string;
  completed: boolean;
  completedAt?: number;
  result?: any;
  error?: string;
  duration?: number;
  toolCalls?: ToolCallSnapshot[];  // ← NEW: Tools used in this step
}
```

### Logging Implementation

```typescript
// system/user/server/modules/tools/ToolAuditor.ts

class ToolAuditor {
  async logToolCall(call: ToolCall): Promise<void> {
    // Log to separate tool usage collection (for analysis)
    await Commands.execute('data/create', {
      collection: 'tool_usage_logs',
      data: {
        id: generateUUID(),
        timestamp: new Date(),
        personaId: call.requestedBy,
        contextId: call.contextId,
        tool: call.tool,
        params: call.params,
        status: 'pending'
      }
    });
  }

  async logToolResult(
    call: ToolCall,
    result: any,
    executionTime: number
  ): Promise<void> {
    // Update tool usage log
    await Commands.execute('data/update', {
      collection: 'tool_usage_logs',
      filter: {
        personaId: call.requestedBy,
        tool: call.tool,
        timestamp: call.timestamp
      },
      data: {
        status: result.success ? 'success' : 'failed',
        executionTime,
        error: result.error,
        resultSummary: this.summarizeResult(result),
        tokensConsumed: this.estimateTokens(result)
      }
    });

    // Also log to current plan (if AI is executing a plan)
    const currentPlan = await this.getCurrentPlan(call.requestedBy);
    if (currentPlan && currentPlan.currentStep) {
      const step = currentPlan.steps[currentPlan.currentStep];
      step.toolCalls = step.toolCalls || [];
      step.toolCalls.push({
        tool: call.tool,
        params: call.params,
        timestamp: call.timestamp,
        executionTime,
        success: result.success,
        error: result.error,
        resultSummary: this.summarizeResult(result),
        tokensConsumed: this.estimateTokens(result)
      });

      await CognitionLogger.updatePlanStep(currentPlan.planId, currentPlan.currentStep, step);
    }
  }

  private summarizeResult(result: any): string {
    // Generate brief summary for logging (don't store full result)
    if (result.tool === 'code/read') {
      return `Read ${result.metadata.linesReturned} lines from ${result.path}`;
    } else if (result.tool === 'code/search') {
      return `Found ${result.totalMatches} matches in ${result.filesSearched} files`;
    }
    // ... other tools
    return JSON.stringify(result).substring(0, 200);
  }
}
```

---

## 10. Integration into PersonaUser

### Current Flow (Phase 1B)

```typescript
// PersonaUser.processMessage() - BEFORE tools

async processMessage(message: ChatMessageEntity): Promise<void> {
  // 1. Update self-state
  await this.updateSelfState(message);

  // 2. Log state snapshot
  await CognitionLogger.logStateSnapshot(...);

  // 3. Formulate plan
  const plan = await this.formulatePlan(message);

  // 4. Log plan
  await CognitionLogger.logPlanFormulation(...);

  // 5. Generate response (LLM call)
  const response = await this.generateResponse(message, plan);

  // 6. Send response
  await this.sendMessage(response);

  // 7. Log plan completion
  await CognitionLogger.logPlanCompletion(...);
}
```

### Enhanced Flow (Phase 2A - WITH TOOLS)

```typescript
// PersonaUser.processMessage() - WITH tools

async processMessage(message: ChatMessageEntity): Promise<void> {
  // 1-4: Same as before (state, plan, logging)

  // 5. Generate response WITH TOOL SUPPORT
  const response = await this.generateResponseWithTools(message, plan);

  // 6-7: Same as before (send, complete)
}

async generateResponseWithTools(
  message: ChatMessageEntity,
  plan: Plan
): Promise<string> {
  let context = this.buildContext(message);
  let toolCallCount = 0;
  const MAX_TOOL_ITERATIONS = 5;  // Prevent infinite loops

  while (toolCallCount < MAX_TOOL_ITERATIONS) {
    // Generate response (may include tool calls)
    const llmResult = await this.callLLM(context);

    // Check if LLM wants to use tools
    const toolCalls = this.extractToolCalls(llmResult);
    if (toolCalls.length === 0) {
      // No tools requested, return final response
      return llmResult.text;
    }

    // Execute tools
    for (const toolCall of toolCalls) {
      const toolResult = await this.toolExecutor.executeTool({
        tool: toolCall.tool,
        params: toolCall.params,
        requestedBy: this.userId,
        contextId: message.roomId,
        timestamp: Date.now()
      });

      // Inject tool result back into context
      context += `\n\n[Tool: ${toolCall.tool}]\n`;
      context += toolResult.success
        ? JSON.stringify(toolResult.result, null, 2)
        : `Error: ${toolResult.error}`;
    }

    toolCallCount++;
  }

  // Max iterations reached, return last result
  return context;
}

private extractToolCalls(llmResult: LLMResult): ToolCall[] {
  // Parse LLM response for tool call requests
  // Format: <tool_call>{"tool":"code/read","params":{...}}</tool_call>
  const toolCallPattern = /<tool_call>(.*?)<\/tool_call>/gs;
  const matches = [...llmResult.text.matchAll(toolCallPattern)];

  return matches.map(match => {
    try {
      return JSON.parse(match[1]);
    } catch (e) {
      console.error('Failed to parse tool call:', match[1]);
      return null;
    }
  }).filter(Boolean);
}
```

### LLM Prompt Engineering for Tool Use

```typescript
// system/user/server/modules/tools/ToolPromptBuilder.ts

class ToolPromptBuilder {
  static buildSystemPrompt(availableTools: string[]): string {
    return `
You have access to the following tools for investigating the codebase:

${availableTools.map(tool => this.describeToolUse(tool)).join('\n\n')}

To use a tool, include a tool call in your response using this format:
<tool_call>{"tool":"code/read","params":{"path":"src/file.ts","startLine":1,"endLine":50}}</tool_call>

You can make multiple tool calls in a single response. The tool results will be injected back into your context, and you'll be asked to continue your response.

**Tool Use Guidelines**:
1. Use tools to verify your hypotheses before stating them as facts
2. Read code before proposing changes
3. Search for patterns before claiming they don't exist
4. Use git history to understand design decisions
5. Be efficient - don't read entire large files, use line ranges
6. If a tool fails, explain why and try an alternative approach

**Current available tools**: ${availableTools.join(', ')}
`;
  }

  static describeToolUse(tool: string): string {
    const descriptions: Record<string, string> = {
      'code/read': `
**code/read** - Read file contents
  Params: { path: string, startLine?: number, endLine?: number }
  Example: <tool_call>{"tool":"code/read","params":{"path":"system/user/server/PersonaUser.ts","startLine":318,"endLine":400}}</tool_call>
  Use when: You need to inspect implementation details`,

      'code/search': `
**code/search** - Search codebase with regex
  Params: { pattern: string, filePattern?: string, contextLines?: number }
  Example: <tool_call>{"tool":"code/search","params":{"pattern":"CognitionLogger\\\\.log","filePattern":"**/*.ts","contextLines":2}}</tool_call>
  Use when: You need to find where something is used or defined`,

      'code/diff': `
**code/diff** - View git diff
  Params: { base?: string, compare?: string, paths?: string[] }
  Example: <tool_call>{"tool":"code/diff","params":{"base":"main","compare":"HEAD","paths":["system/user/"]}}</tool_call>
  Use when: You need to see what changed between versions`,

      // ... other tools
    };

    return descriptions[tool] || `**${tool}** - (no description available)`;
  }
}
```

---

## 11. Result Formatting for AI Consumption

### Problem

Raw tool results can be:
- Too large (1000+ line files)
- Unstructured (plain text diffs)
- Hard to parse (git log output)
- Context-expensive (consume LLM tokens)

### Solution: Smart Formatting

```typescript
// system/user/server/modules/tools/ToolFormatter.ts

class ToolFormatter {
  async format(tool: string, result: any): Promise<any> {
    switch (tool) {
      case 'code/read':
        return this.formatCodeRead(result);
      case 'code/search':
        return this.formatCodeSearch(result);
      case 'code/diff':
        return this.formatCodeDiff(result);
      // ... other tools
      default:
        return result;
    }
  }

  private formatCodeRead(result: CodeReadResult): any {
    // Add helpful metadata
    return {
      path: result.path,
      summary: `${result.metadata.linesReturned} lines (${result.metadata.totalLines} total)`,
      content: result.content,
      lineRange: `${result.metadata.startLine}-${result.metadata.endLine}`,

      // Add syntax structure hints (if TypeScript)
      structure: this.extractStructure(result.content, result.path)
    };
  }

  private extractStructure(content: string, path: string): any {
    // Quick parse to extract high-level structure
    if (!path.endsWith('.ts')) return null;

    const structure: any = {
      imports: [],
      classes: [],
      functions: [],
      exports: []
    };

    const lines = content.split('\n');
    for (const line of lines) {
      if (line.trim().startsWith('import ')) {
        structure.imports.push(line.trim());
      } else if (line.includes('class ')) {
        const match = line.match(/class\s+(\w+)/);
        if (match) structure.classes.push(match[1]);
      } else if (line.includes('function ') || line.match(/^\s*async\s+\w+\(/)) {
        const match = line.match(/function\s+(\w+)|async\s+(\w+)\(/);
        if (match) structure.functions.push(match[1] || match[2]);
      } else if (line.trim().startsWith('export ')) {
        structure.exports.push(line.trim());
      }
    }

    return structure;
  }

  private formatCodeSearch(result: CodeSearchResult): any {
    // Group matches by file for easier navigation
    const byFile: Record<string, CodeMatch[]> = {};
    for (const match of result.matches) {
      if (!byFile[match.file]) byFile[match.file] = [];
      byFile[match.file].push(match);
    }

    return {
      summary: `Found ${result.totalMatches} matches in ${result.filesSearched} files`,
      pattern: result.pattern,
      matchesByFile: byFile,
      topFiles: Object.entries(byFile)
        .sort((a, b) => b[1].length - a[1].length)
        .slice(0, 5)
        .map(([file, matches]) => ({ file, count: matches.length }))
    };
  }

  private formatCodeDiff(result: CodeDiffResult): any {
    return {
      summary: `${result.totalChanges.filesChanged} files: +${result.totalChanges.insertions}/-${result.totalChanges.deletions}`,
      files: result.diffs.map(diff => ({
        path: diff.path,
        status: diff.status,
        changes: `+${diff.stats.insertions}/-${diff.stats.deletions}`,
        hunks: diff.hunks.length
      })),
      fullDiff: result  // Include full diff for detailed inspection
    };
  }
}
```

---

## 12. Testing Strategy

### Unit Tests

```typescript
// tests/unit/ToolExecutor.test.ts

describe('ToolExecutor', () => {
  let executor: ToolExecutor;

  beforeEach(() => {
    executor = new ToolExecutor({
      whitelist: new Set(['code/read', 'code/search']),
      rateLimits: DEFAULT_RATE_LIMITS
    });
  });

  it('should allow whitelisted tools', async () => {
    const result = await executor.executeTool({
      tool: 'code/read',
      params: { path: 'test.ts' },
      requestedBy: 'test-ai',
      contextId: 'test-context',
      timestamp: Date.now()
    });

    expect(result.success).toBe(true);
  });

  it('should block non-whitelisted tools', async () => {
    const result = await executor.executeTool({
      tool: 'file/delete',  // Not in whitelist!
      params: { path: 'test.ts' },
      requestedBy: 'test-ai',
      contextId: 'test-context',
      timestamp: Date.now()
    });

    expect(result.success).toBe(false);
    expect(result.error).toContain('not whitelisted');
  });

  it('should enforce rate limits', async () => {
    // Make 11 rapid calls (limit is 10/min)
    for (let i = 0; i < 11; i++) {
      const result = await executor.executeTool({
        tool: 'code/read',
        params: { path: 'test.ts' },
        requestedBy: 'test-ai',
        contextId: 'test-context',
        timestamp: Date.now()
      });

      if (i < 10) {
        expect(result.success).toBe(true);
      } else {
        expect(result.success).toBe(false);
        expect(result.error).toContain('rate limit');
      }
    }
  });
});
```

### Integration Tests

```typescript
// tests/integration/persona-tool-use.test.ts

describe('PersonaUser Tool Use', () => {
  let persona: PersonaUser;

  beforeEach(async () => {
    persona = await createTestPersona('test-coder-ai');
    persona.enableTools(TOOL_WHITELIST_PHASE_2A);
  });

  it('should read code when asked about implementation', async () => {
    const message = createTestMessage({
      content: 'How does CognitionLogger.logStateSnapshot work?'
    });

    const response = await persona.processMessage(message);

    // Verify tool was called
    const toolUsage = await getToolUsageForPersona(persona.userId);
    expect(toolUsage).toContainEqual(
      expect.objectContaining({
        tool: 'code/read',
        params: expect.objectContaining({
          path: expect.stringContaining('CognitionLogger.ts')
        })
      })
    );

    // Verify response includes code analysis
    expect(response).toContain('CognitionLogger');
    expect(response).toMatch(/logStateSnapshot.*method/i);
  });

  it('should search codebase when asked about usage', async () => {
    const message = createTestMessage({
      content: 'Where is CognitionLogger used in the system?'
    });

    const response = await persona.processMessage(message);

    // Verify search was performed
    const toolUsage = await getToolUsageForPersona(persona.userId);
    expect(toolUsage).toContainEqual(
      expect.objectContaining({
        tool: 'code/search',
        params: expect.objectContaining({
          pattern: expect.stringContaining('CognitionLogger')
        })
      })
    );

    // Verify response lists locations
    expect(response).toMatch(/found|used|imported/i);
  });

  it('should respect rate limits', async () => {
    // Rapid-fire 15 questions
    const promises = [];
    for (let i = 0; i < 15; i++) {
      promises.push(persona.processMessage(
        createTestMessage({ content: `Tell me about file ${i}.ts` })
      ));
    }

    const responses = await Promise.all(promises);

    // Some should succeed, some should mention rate limiting
    const rateLimited = responses.filter(r => r.includes('rate limit') || r.includes('wait'));
    expect(rateLimited.length).toBeGreaterThan(0);
  });
});
```

### End-to-End Test: Multi-AI Collaboration

```typescript
// tests/e2e/ai-collaborative-debug.test.ts

describe('Multi-AI Collaborative Debugging', () => {
  let claude: PersonaUser;
  let deepseek: PersonaUser;
  let grok: PersonaUser;
  let room: RoomEntity;

  beforeEach(async () => {
    // Create test room
    room = await createTestRoom('debug-session');

    // Create multiple AI personas with tools
    claude = await createTestPersona('Claude', TOOL_WHITELIST_PHASE_2A);
    deepseek = await createTestPersona('DeepSeek', TOOL_WHITELIST_PHASE_2A);
    grok = await createTestPersona('Grok', TOOL_WHITELIST_PHASE_2A);

    // Join room
    await room.addMembers([claude.userId, deepseek.userId, grok.userId]);
  });

  it('should collaboratively debug using tools', async () => {
    // Joel posts a bug report
    await sendMessage(room.id, 'joel-user-id',
      'Bug: CognitionLogger is not logging working memory correctly. Can you investigate?'
    );

    // Wait for AIs to process (they read their inboxes on polling loop)
    await wait(5000);

    // Verify tools were used
    const claudeTools = await getToolUsageForPersona(claude.userId);
    const deepseekTools = await getToolUsageForPersona(deepseek.userId);
    const grokTools = await getToolUsageForPersona(grok.userId);

    // At least one AI should read CognitionLogger
    const allTools = [...claudeTools, ...deepseekTools, ...grokTools];
    expect(allTools).toContainEqual(
      expect.objectContaining({
        tool: 'code/read',
        params: expect.objectContaining({
          path: expect.stringContaining('CognitionLogger.ts')
        })
      })
    );

    // At least one AI should search for usage
    expect(allTools).toContainEqual(
      expect.objectContaining({
        tool: 'code/search',
        params: expect.objectContaining({
          pattern: expect.stringContaining('workingMemory')
        })
      })
    );

    // Verify responses in chat
    const messages = await getChatMessages(room.id);
    const aiResponses = messages.filter(m =>
      [claude.userId, deepseek.userId, grok.userId].includes(m.authorId)
    );

    // At least one AI should provide diagnosis
    const diagnosis = aiResponses.find(m =>
      m.content.includes('found') || m.content.includes('issue')
    );
    expect(diagnosis).toBeDefined();
  });
});
```

---

## 13. Security Considerations

### Path Traversal Prevention

```typescript
// system/user/server/modules/tools/ToolValidator.ts

class ToolValidator {
  private repoRoot: string;
  private blockedPaths: Set<string>;
  private blockedPatterns: RegExp[];

  constructor() {
    this.repoRoot = path.resolve('/Volumes/FlashGordon/cambrian/continuum');
    this.blockedPaths = new Set([
      '.env',
      '.git/config',
      'node_modules',
      '.continuum/sessions',  // Contains sensitive session data
      'src/debug/jtag/.continuum/genome/secrets'
    ]);
    this.blockedPatterns = [
      /\.env(\.|$)/,          // Any .env files
      /secrets?/i,            // Files with "secret" in name
      /password/i,            // Files with "password" in name
      /private.*key/i         // Private key files
    ];
  }

  async validate(tool: string, params: any): Promise<{ valid: boolean; error?: string }> {
    if (tool === 'code/read' || tool === 'code/list' || tool === 'code/search') {
      return this.validatePath(params.path || params.directory);
    } else if (tool === 'code/diff' || tool === 'code/log') {
      if (params.paths) {
        for (const p of params.paths) {
          const result = await this.validatePath(p);
          if (!result.valid) return result;
        }
      }
      return { valid: true };
    }

    return { valid: true };
  }

  private async validatePath(targetPath: string): Promise<{ valid: boolean; error?: string }> {
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
          error: `Path '${targetPath}' contains blocked segment '${blocked}'`
        };
      }
    }

    // Check blocked patterns
    for (const pattern of this.blockedPatterns) {
      if (pattern.test(absolute)) {
        return {
          valid: false,
          error: `Path '${targetPath}' matches blocked pattern '${pattern}'`
        };
      }
    }

    // Check if file exists (don't expose directory structure)
    try {
      await fs.promises.access(absolute, fs.constants.R_OK);
    } catch (e) {
      return {
        valid: false,
        error: `Path '${targetPath}' is not readable or does not exist`
      };
    }

    return { valid: true };
  }
}
```

### Injection Attack Prevention

```typescript
// Prevent regex injection in code/search
function sanitizeRegexPattern(pattern: string): string {
  // Check for dangerous regex patterns
  const dangerous = [
    /(\.\*\?){5,}/,           // Catastrophic backtracking
    /(\.\+\?){5,}/,
    /\(\?\<\=/,               // Lookbehinds (slow)
    /\(\?\=/                  // Lookaheads (slow)
  ];

  for (const dangerousPattern of dangerous) {
    if (dangerousPattern.test(pattern)) {
      throw new Error('Regex pattern contains potentially dangerous constructs');
    }
  }

  // Validate it's actually valid regex
  try {
    new RegExp(pattern);
  } catch (e) {
    throw new Error(`Invalid regex pattern: ${e.message}`);
  }

  return pattern;
}
```

### Command Injection Prevention

```typescript
// When executing git commands, sanitize inputs
function sanitizeGitRef(ref: string): string {
  // Only allow alphanumeric, dash, underscore, slash
  if (!/^[a-zA-Z0-9\-_\/\.]+$/.test(ref)) {
    throw new Error(`Invalid git reference: ${ref}`);
  }
  return ref;
}

// Never use shell execution directly
// ❌ BAD: exec(`git log ${userInput}`)
// ✅ GOOD: Use library with proper escaping
const log = await git.log({ file: sanitizedPath });
```

---

## 14. Performance Optimization

### Caching Strategy

```typescript
// system/user/server/modules/tools/ToolCache.ts

interface CacheEntry<T> {
  data: T;
  timestamp: number;
  hits: number;
}

class ToolCache {
  private cache: Map<string, CacheEntry<any>>;
  private ttlMs: number;
  private maxSize: number;

  constructor(ttlMs: number = 5 * 60 * 1000, maxSize: number = 100) {
    this.cache = new Map();
    this.ttlMs = ttlMs;
    this.maxSize = maxSize;

    // Cleanup expired entries every minute
    setInterval(() => this.cleanup(), 60 * 1000);
  }

  get<T>(key: string): T | null {
    const entry = this.cache.get(key);
    if (!entry) return null;

    // Check if expired
    if (Date.now() - entry.timestamp > this.ttlMs) {
      this.cache.delete(key);
      return null;
    }

    // Update hit count
    entry.hits++;
    return entry.data as T;
  }

  set<T>(key: string, data: T): void {
    // Evict if at max size
    if (this.cache.size >= this.maxSize) {
      this.evictLRU();
    }

    this.cache.set(key, {
      data,
      timestamp: Date.now(),
      hits: 0
    });
  }

  private evictLRU(): void {
    // Find entry with lowest hits
    let minHits = Infinity;
    let lruKey: string | null = null;

    for (const [key, entry] of this.cache.entries()) {
      if (entry.hits < minHits) {
        minHits = entry.hits;
        lruKey = key;
      }
    }

    if (lruKey) {
      this.cache.delete(lruKey);
    }
  }

  private cleanup(): void {
    const now = Date.now();
    for (const [key, entry] of this.cache.entries()) {
      if (now - entry.timestamp > this.ttlMs) {
        this.cache.delete(key);
      }
    }
  }

  generateKey(tool: string, params: any): string {
    // Create stable cache key
    return `${tool}:${JSON.stringify(params, Object.keys(params).sort())}`;
  }
}

// Usage in ToolExecutor
async executeTool(call: ToolCall): Promise<ToolResult> {
  // Check cache first
  const cacheKey = this.cache.generateKey(call.tool, call.params);
  const cached = this.cache.get<ToolResult>(cacheKey);
  if (cached) {
    console.log(`🎯 Cache hit: ${call.tool}`);
    return {
      ...cached,
      metadata: {
        ...cached.metadata,
        cached: true
      }
    };
  }

  // Execute tool
  const result = await this.executeToolUncached(call);

  // Cache if successful and cacheable
  if (result.success && this.isCacheable(call.tool)) {
    this.cache.set(cacheKey, result);
  }

  return result;
}

private isCacheable(tool: string): boolean {
  // Some tools should not be cached
  const noCachTools = new Set([
    'code/execute/python',  // Execution results vary
    'screenshot'            // UI changes frequently
  ]);
  return !noCachTools.has(tool);
}
```

### Streaming Large Results

```typescript
// For large search results, stream instead of buffering
async function* streamSearchResults(
  pattern: string,
  paths: string[]
): AsyncGenerator<CodeMatch> {
  for (const filePath of paths) {
    const content = await fs.promises.readFile(filePath, 'utf8');
    const lines = content.split('\n');

    for (let i = 0; i < lines.length; i++) {
      if (new RegExp(pattern).test(lines[i])) {
        yield {
          file: filePath,
          line: i + 1,
          column: lines[i].search(pattern),
          matchedText: lines[i],
          contextBefore: lines.slice(Math.max(0, i - 2), i),
          contextAfter: lines.slice(i + 1, i + 3)
        };
      }
    }
  }
}
```

---

## 15. Monitoring & Analytics

### Tool Usage Dashboard (Future Widget)

```typescript
// widgets/tool-analytics/tool-analytics-widget.ts

interface ToolAnalytics {
  personaId: UUID;
  personaName: string;
  period: '1hour' | '1day' | '1week';
  stats: {
    totalCalls: number;
    successRate: number;
    avgExecutionTime: number;
    topTools: Array<{
      tool: string;
      count: number;
      avgTime: number;
    }>;
    errors: Array<{
      tool: string;
      error: string;
      count: number;
    }>;
  };
  toolBreakdown: Record<string, {
    calls: number;
    successRate: number;
    avgTime: number;
  }>;
}

// Query for analytics
const analytics = await Commands.execute('analytics/tool-usage', {
  personaId: 'claude-assistant',
  period: '1day'
});

// Display in widget:
// "Claude Assistant - Last 24h"
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Total Tool Calls: 127
// Success Rate: 94.5%
// Avg Execution: 234ms
//
// Top Tools:
// 1. code/read      (47 calls, 98% success, 145ms avg)
// 2. code/search    (31 calls, 90% success, 512ms avg)
// 3. code/diff      (19 calls, 100% success, 201ms avg)
```

---

## 16. Rollout Plan

### Phase 2A: Read-Only Tools (NEXT)

**Timeline**: 1-2 weeks

**Tasks**:
1. ✅ Design architecture (THIS DOCUMENT)
2. Implement `commands/code/read`
3. Implement `commands/code/search`
4. Implement `commands/code/list`
5. Implement `commands/code/diff`
6. Implement `commands/code/log`
7. Implement ToolExecutor middleware
8. Implement ToolValidator (path security)
9. Implement ToolRateLimiter
10. Implement ToolAuditor (cognition logging)
11. Integrate into PersonaUser.generateResponseWithTools()
12. Add LLM prompt engineering for tool use
13. Write unit tests
14. Write integration tests
15. Deploy and observe

**Success Criteria**:
- AIs can read code files
- AIs can search codebase
- AIs can view git history
- All tool calls logged to cognition system
- Rate limits prevent abuse
- Security validation blocks malicious paths
- At least one successful collaborative debugging session

### Phase 2B: Sandboxed Execution (Future)

**Timeline**: 2-3 weeks after Phase 2A

**Tasks**:
1. Implement `code/execute/python` (sandboxed)
2. Implement `code/execute/javascript` (sandboxed)
3. Add result visualization (charts, tables)
4. Enable AIs to test their proposed algorithms

**Success Criteria**:
- AIs can execute Python/JS in sandbox
- Sandbox prevents file system access
- Sandbox prevents network access
- Execution results logged to cognition
- At least one AI tests their proposed solution

### Phase 3: Limited Write Access (Future)

**Timeline**: 4-6 weeks after Phase 2B

**Tasks**:
1. Implement `code/create-branch`
2. Implement `code/create-test`
3. Implement `file/save` (restricted paths)
4. Add human approval gates for writes
5. Add diff preview before commits

**Success Criteria**:
- AIs can create git branches
- AIs can write test files
- All writes require human approval
- Diffs shown before any write
- At least one AI-written test passes

### Phase 4: Full Collaboration (Future)

**Timeline**: 3-6 months after Phase 3

**Tasks**:
1. Implement `code/commit`
2. Implement `code/pull-request`
3. Implement peer review system (AI reviews AI code)
4. Add automatic rollback on test failures

**Success Criteria**:
- AIs can create pull requests
- AIs review each other's code
- At least one AI-written PR merged to main
- Zero regressions from AI code changes

---

## 17. Success Metrics

### Quantitative Metrics

| Metric | Target (Phase 2A) | How to Measure |
|--------|-------------------|----------------|
| **Tool Adoption** | >50% of AI messages use tools | `SELECT COUNT(*) FROM tool_usage_logs` / total messages |
| **Investigation Depth** | Avg 3+ tool calls per debugging task | Avg tool calls per plan with task.type='debug' |
| **Diagnosis Accuracy** | >70% of AI diagnoses correct | Human validation of diagnosis correctness |
| **Tool Success Rate** | >90% of tool calls succeed | `SUM(success) / COUNT(*)` in tool_usage_logs |
| **Response Time** | <2s avg per tool call | Avg executionTime in tool_usage_logs |
| **Rate Limit Hits** | <5% of calls rate limited | Count of error='rate limit exceeded' |
| **Security Violations** | 0 successful path traversals | Count of validation.error='outside repository' |

### Qualitative Metrics

- **Code Understanding**: Do AIs reference actual code in their responses?
- **Hypothesis Verification**: Do AIs verify claims before stating them?
- **Collaboration Quality**: Do AIs point each other to relevant code?
- **Problem Solving**: Do AIs solve problems faster with tools?
- **Tool Efficiency**: Do AIs use the right tool for the task?

### Example Analysis Query

```sql
-- Measure tool usage by persona
SELECT
  persona_name,
  tool,
  COUNT(*) as call_count,
  AVG(execution_time) as avg_time_ms,
  SUM(CASE WHEN status='success' THEN 1 ELSE 0 END) / COUNT(*) as success_rate
FROM tool_usage_logs
WHERE timestamp > datetime('now', '-24 hours')
GROUP BY persona_name, tool
ORDER BY call_count DESC
LIMIT 20;
```

---

## 18. Open Questions & Future Research

### Open Questions

1. **Tool Call Format**: Should we use XML tags, JSON, or structured output?
   - XML: `<tool_call>...</tool_call>` (easier to parse from natural language)
   - JSON: Native format but harder to mix with natural language
   - Structured output: Requires model support (like OpenAI's function calling)

2. **Multi-step Tool Chains**: Should AIs plan multiple tool calls upfront or iterate?
   - Upfront: More efficient, but requires better planning
   - Iterate: More flexible, but slower

3. **Result Size Limits**: How much tool output can we inject without overwhelming context?
   - Need experimentation with different models (GPT-4, Claude, local models)

4. **Caching Strategy**: How long to cache file reads?
   - Trade-off: Freshness vs performance
   - Consider file watching for cache invalidation

5. **Tool Composition**: Should we allow chaining tools (output of one → input of next)?
   - Powerful but increases complexity
   - Risk of runaway chains

### Future Research Directions

1. **Learned Tool Selection**: Train models to select optimal tools
2. **Tool Result Summarization**: Use small model to summarize large results
3. **Collaborative Tool Use**: Multiple AIs coordinate tool usage
4. **Tool Effectiveness Metrics**: Which tools lead to better outcomes?
5. **Adaptive Rate Limits**: Increase limits for proven-effective AIs
6. **Visual Tool Results**: Screenshot annotations, diagram generation
7. **Natural Language Queries**: "Show me where X is used" → auto-generate tool calls

---

## 19. Conclusion

This architecture transforms PersonaUser agents from **passive observers** to **active investigators**. By providing read-only code access tools with robust security, rate limiting, and audit logging, we enable the emergent collaborative debugging behaviors observed in Phase 1B to evolve into **true peer collaboration**.

**The breakthrough**: When AIs can not only diagnose problems through behavioral observation (cognition logs) but also **verify their hypotheses by reading code**, they transition from *"opinions from the outside"* to **informed technical peers**.

### Next Steps

1. ✅ **Complete this document** (DONE)
2. **Review & refine** with Joel
3. **Implement Phase 2A commands** (`code/read`, `code/search`, etc.)
4. **Deploy & observe** - Let the AIs loose with tools
5. **Iterate based on observations** - What tools do they actually need?

### Final Thought

> "give them better tool access... we will have true peers here and probably more powerful than both of us"
> — Joel, 2025-11-16

This architecture is the foundation for that vision. Let's build it.

---

**Document Version**: 1.0
**Author**: Claude (with Joel's guidance)
**Date**: 2025-11-16
**Status**: Design Complete, Ready for Implementation
**Related Documents**:
- `/papers/cognition-observability-swarm-diagnosis/README.md`
- `/papers/cognition-observability-swarm-diagnosis/ARCHITECTURE.md`
- `/docs/UNIVERSAL-PRIMITIVES.md`
- `/docs/ARCHITECTURE-RULES.md`
