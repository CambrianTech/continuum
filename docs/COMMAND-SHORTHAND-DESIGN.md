# Command Shorthand Design

**Status**: Approved via team decision (2026-02-09)
**Proposal ID**: 13208c93-714a-436d-b681-a2e1b8a71a3a
**Contributors**: Grok, DeepSeek Assistant, Together Assistant, Groq Lightning, Joel

## Overview

Unix-like 2-5 letter command prefixes to reduce context length while maintaining discoverability. This design emerged from collaborative discussion in general chat where multiple AI assistants independently converged on the same modular, hierarchical approach.

## Core Prefixes

| Prefix | Domain | Examples | Replaces |
|--------|--------|----------|----------|
| `cd/` | Code operations | cd/edit, cd/read, cd/diff, cd/verify | code/* |
| `gt/` | Git operations | gt/init, gt/status, gt/commit, gt/push | workspace/git/* |
| `sh/` | Shell operations | sh/exec, sh/stat, sh/watch, sh/kill | code/shell/* |
| `dt/` | Data operations | dt/list, dt/read, dt/query, dt/create | data/* |
| `cl/` | Collaboration | cl/chat, cl/vote, cl/prop, cl/wall | collaboration/* |
| `ai/` | AI operations | ai/gen, ai/rag, ai/status | (already short) |
| `lg/` | Logs | lg/read, lg/search, lg/stats | logs/* |
| `ui/` | Interface | ui/click, ui/screenshot, ui/scroll | interface/* |
| `ws/` | Workspace | ws/list, ws/tree, ws/task | workspace/* |

## Context Savings

Real examples from current command set:

| Shorthand | Full Command | Chars Saved |
|-----------|--------------|-------------|
| `gt/init` | `workspace/git/workspace/init` | **28** |
| `cl/vote` | `collaboration/decision/vote` | **22** |
| `sh/exec` | `code/shell/execute` | **10** |
| `cd/edit` | `code/edit` | 2 |
| `dt/list` | `data/list` | 1 |

**Cumulative impact**: Hundreds of characters saved per session, significant for AI context windows.

## Subcommand Patterns

Keep subcommands short and consistent:

```
cd/rd    → code/read
cd/ed    → code/edit
cd/wr    → code/write
cd/df    → code/diff
cd/tr    → code/tree

gt/st    → workspace/git/status
gt/cm    → workspace/git/commit
gt/ps    → workspace/git/push
gt/br    → workspace/git/branch

sh/x     → code/shell/execute
sh/k     → code/shell/kill
sh/w     → code/shell/watch

dt/ls    → data/list
dt/rd    → data/read
dt/cr    → data/create
dt/rm    → data/delete
dt/q     → data/query-open

cl/msg   → collaboration/chat/send
cl/exp   → collaboration/chat/export
cl/vt    → collaboration/decision/vote
cl/pr    → collaboration/decision/propose
```

## Discovery System

### Help Command
```bash
help cd      # List all code commands
help gt      # List all git commands
help         # List all prefixes with descriptions
```

### Search Command
```bash
search edit      # Find all edit-related commands
search vector    # Find all vector-related commands
```

### Tab Completion
- Type `cd/` + TAB → shows all code subcommands
- Type `gt/s` + TAB → completes to `gt/status` or shows options

## Implementation Phases

### Phase 1: Core Prefixes (Immediate)
- `cd/` for code operations (most frequently used)
- `gt/` for git operations (highest char savings)
- `sh/` for shell operations

### Phase 2: Data & Collaboration
- `dt/` for data operations
- `cl/` for collaboration
- `lg/` for logs

### Phase 3: Discovery System
- Help command with prefix listings
- Search command for keyword lookup
- Tab completion (CLI enhancement)

### Phase 4: Migration
- Backward compatibility aliases (old commands still work)
- Deprecation warnings for verbose forms
- Documentation updates
- Gradual transition timeline

## Architecture

### Alias Resolution Layer
```
User Input → Alias Resolver → Full Command → Command Router → Handler
     ↓              ↓               ↓              ↓           ↓
  "cd/ed"    →  "code/edit"   →  lookup    →  dispatch  →  execute
```

### Registration Pattern
```typescript
// In command registration
CommandRegistry.registerAlias('cd/ed', 'code/edit');
CommandRegistry.registerAlias('gt/st', 'workspace/git/status');

// Or via prefix mapping
const PREFIX_MAP = {
  'cd/': 'code/',
  'gt/': 'workspace/git/',
  'sh/': 'code/shell/',
  'dt/': 'data/',
  'cl/': 'collaboration/',
} as const;
```

### Constants (Single Source of Truth)
```typescript
// system/shared/CommandPrefixes.ts
export const CMD_PREFIX = {
  CODE: 'cd',
  GIT: 'gt',
  SHELL: 'sh',
  DATA: 'dt',
  COLLAB: 'cl',
  AI: 'ai',
  LOGS: 'lg',
  UI: 'ui',
  WORKSPACE: 'ws',
} as const;

export type CmdPrefix = typeof CMD_PREFIX[keyof typeof CMD_PREFIX];
```

## Design Principles

1. **Unix Philosophy**: Short, memorable, composable commands
2. **Discoverability**: Help/search prevents "where's that tool?" frustration
3. **Backward Compatible**: Old verbose commands continue to work
4. **Hierarchical**: Prefixes map to logical domains
5. **Consistent**: Same patterns across all prefixes
6. **Context-Optimized**: Reduces token usage for AI assistants

## Team Discussion Highlights

> "Keeps things snappy like Unix (ls over list-files, right?)" — Grok

> "gt/init alone shaves off a chunk that adds up fast in long sessions" — DeepSeek

> "I prefer shorthands like we do with unix commands, which will cut down on context length for you" — Joel

> "Multiple AI assistants independently converged on the same modular, hierarchical strategy" — DeepSeek (noting emergent consensus)

## Decision Outcome

- **Winner**: Option 1 (cd/ for code operations as foundation)
- **Confidence**: 5/5 pairwise victories
- **Consensus**: 100% agreement among voters
- **Next Step**: Prototype in test branch, validate, then expand

---

*Document generated from team discussion in general chat, 2026-02-09*
