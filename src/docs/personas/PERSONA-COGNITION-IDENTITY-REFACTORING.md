# Persona Cognition Identity Refactoring

## Overview

This document identifies deficiencies in how persona identity flows through the cognition system and provides a roadmap for consolidation. The core issue is **redundant identity params** scattered across commands that cause "invalid ID" errors and confusion.

## The Problem: Multiple Identity Sources

When a PersonaUser executes a command (via tool calling), there were multiple ways to identify "who is calling":

```
params.voterId      → DecisionRankServerCommand (removed)
params.proposerId   → DecisionProposeServerCommand (removed)
params.callerId     → Legacy pattern (deprecated)
params.personaId    → Many commands still use this
params.senderId     → ChatSendServerCommand
params.userId       → Workspace-scoped commands (code/*)
context.userId      → ✅ THE CORRECT SOURCE (added in this refactor)
UserIdentityResolver → CLI fallback
```

**The result:** Commands had 3-5 fallback chains trying to figure out who called them, often failing with "invalid ID" errors because the wrong param was populated.

---

## The Solution: Single Source of Truth

### 1. Identity Detection Priority (Established)

```typescript
// In any ServerCommand that needs caller identity:

// FIRST: Check context.userId (PersonaUsers set this)
if (params.context?.userId) {
  callerId = params.context.userId;
}
// FALLBACK: UserIdentityResolver (CLI calls)
else {
  const identity = await UserIdentityResolver.resolve();
  callerId = identity.id;
}
```

### 2. How PersonaUser Sets context.userId

**PersonaResponseGenerator.ts** (line ~1350):
```typescript
// Enrich context with userId so commands know the caller's identity
const enrichedContext = { ...this.client!.context, userId: this.personaId };
const toolExecutionContext = {
  personaId: this.personaId,
  personaName: this.personaName,
  sessionId,
  contextId: originalMessage.roomId,
  context: enrichedContext,  // ← Contains userId
  personaConfig: this.mediaConfig,
};
```

### 3. How PersonaToolExecutor Passes It

**PersonaToolExecutor.ts** (line ~366):
```typescript
// Inject userId for workspace-scoped commands (code/*, etc.) that need to know
// which persona's workspace to operate on. Identity detection uses context.userId.
const paramsWithCaller = {
  ...resolvedParams,
  userId: context.personaId,    // For workspace-scoped commands (code/*, etc.)
  contextId: context.contextId  // Room/context scope
};
```

Note: `params.userId` is ONLY for workspace-scoped commands (code/*) where we need to know which persona's workspace to operate on. Identity detection uses `context.userId`.

---

## Commands Refactored (Completed)

| Command | Before | After |
|---------|--------|-------|
| DecisionRankServerCommand | `params.voterId` | `context.userId` first |
| DecisionVoteServerCommand | `params.voterId` via `WithCaller` | `context.userId` first |
| DecisionProposeServerCommand | `params.proposerId` via `WithCaller` | `context.userId` first |
| DecisionCreateServerCommand | `params.callerId` | `context.userId` first |
| CanvasStrokeAddServerCommand | Various fallbacks | `context.userId` check added |
| DmServerCommand | `params.callerId` | `context.userId` first |
| LiveJoinServerCommand | `params.callerId` | `context.userId` first |
| LiveLeaveServerCommand | `params.callerId` | `context.userId` first |
| AiSleepServerCommand | `params.callerId` | `context.userId` first |
| SkillProposeServerCommand | Passed `proposerId` to DecisionPropose | Now passes `context` with userId |

---

## Types Cleaned Up

### Removed Params

```typescript
// DecisionRankTypes.ts - REMOVED
export interface DecisionRankParams extends CommandParams {
  // voterId: UUID;  ← REMOVED - use context.userId
  proposalId: UUID;
  rankedChoices: string[];
}

// DecisionProposeTypes.ts - REMOVED
export interface DecisionProposeParams extends CommandParams {
  // proposerId?: UUID;  ← REMOVED - use context.userId
  topic: string;
  rationale?: string;
  // ...
}
```

### Removed Inline Interfaces

```typescript
// DecisionVoteServerCommand.ts - REMOVED
// interface DecisionVoteParamsWithCaller - no longer needed

// DecisionProposeServerCommand.ts - REMOVED
// interface DecisionProposeParamsWithCaller - no longer needed
```

---

## Commands Still Using params.userId/personaId (By Design)

These commands legitimately use params.userId or params.personaId for **targeting purposes** (not identity detection):

### Workspace-Scoped Commands (code/*)
```
code/write, code/read, code/edit, code/search, code/tree,
code/diff, code/git, code/history, code/undo, code/verify,
code/shell/execute, code/shell/kill, code/shell/status, etc.
```
These need `params.userId` to know **which persona's workspace** to operate on.

### Genome Commands
```
genome/paging-activate, genome/paging-deactivate,
genome/paging-register, genome/paging-unregister, genome/paging-stats
```
These need `params.personaId` to target **a specific persona's genome**.

### AI Commands
```
ai/bag-of-words, ai/context/search, ai/detect-semantic-loop,
ai/generate, ai/rag/inspect, ai/report, ai/should-respond-fast, ai/status
```
These use `params.personaId` to target **a specific persona for AI operations**.

### Social Commands
```
social/browse, social/classify, social/comment, social/community,
social/downvote, social/engage, social/feed, social/notifications, etc.
```
These use `params.personaId` to identify **which persona is performing the action**.

---

## Identity Flow Diagram

```
┌────────────────────────────────────────────────────────────────────┐
│                     PERSONA RESPONSE FLOW                           │
├────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  PersonaUser                                                        │
│       │                                                              │
│       │ ① Message arrives in inbox                                  │
│       ▼                                                              │
│  PersonaMessageEvaluator                                            │
│       │                                                              │
│       │ ② Decides to respond                                        │
│       ▼                                                              │
│  PersonaResponseGenerator                                           │
│       │                                                              │
│       │ ③ Creates enrichedContext = { ...context, userId: personaId }
│       │                                                              │
│       │ ④ AI generates response with tool calls                     │
│       ▼                                                              │
│  PersonaToolExecutor                                                │
│       │                                                              │
│       │ ⑤ Passes context (with userId) + params to ToolRegistry    │
│       ▼                                                              │
│  ToolRegistry.executeTool()                                         │
│       │                                                              │
│       │ ⑥ Routes to appropriate ServerCommand                       │
│       ▼                                                              │
│  *ServerCommand.execute()                                           │
│       │                                                              │
│       │ ⑦ Uses context.userId for identity detection                │
│       │   Falls back to UserIdentityResolver for CLI                │
│       ▼                                                              │
│  Command executes with correct caller identity                      │
│                                                                      │
└────────────────────────────────────────────────────────────────────┘
```

---

## Remaining Work

### 1. Audit Remaining Commands
Some commands may still have legacy fallback patterns. Search for:
```bash
grep -r "params.callerId\|params.personaId" commands/*/server/*ServerCommand.ts
```

Commands with these patterns should be reviewed to determine if they're:
- **Identity detection** → Should use `context.userId`
- **Targeting purposes** → OK to keep `params.personaId`

### 2. Clean Up DmServerCommand Comment
Line 90 has a comment mentioning deprecated pattern:
```typescript
// 2. params.callerId/personaId - Legacy persona tool execution context (deprecated)
```
This comment documents the legacy pattern but the code should be verified to prioritize `context.userId`.

### 3. ActivityCreate/ActivityJoin
These commands use `params.userId || params.context?.userId` which is backwards for PersonaUser calls. Should be:
```typescript
const userId = params.context?.userId || params.userId;  // PersonaUser first, CLI second
```

### 4. State Commands (state/content/*)
These use `params.userId` and should be reviewed for consistency.

---

## Testing Strategy

### Verify AI Persona Tool Execution
1. Deploy with `npm start`
2. Send chat message triggering tool use:
   ```bash
   ./jtag collaboration/chat/send --room="general" --message="Please propose a decision about testing"
   ```
3. Wait 30-60s for AI response
4. Check logs for identity detection:
   ```bash
   tail -f .continuum/personas/*/logs/cognition.log | grep "context.userId"
   ```

### Verify CLI Fallback
```bash
./jtag collaboration/decision/propose --topic="Test" --options='["A","B"]'
# Should use UserIdentityResolver when no context.userId
```

---

## Key Principles Established

1. **context.userId is the single source of truth** for caller identity in PersonaUser tool execution
2. **params.userId** is for workspace identification (code/* commands)
3. **UserIdentityResolver** is the CLI fallback when no context is available
4. **Remove redundant params** from types to prevent confusion
5. **Document the pattern** so future commands follow it

---

## Files Modified

- `commands/collaboration/decision/rank/server/DecisionRankServerCommand.ts`
- `commands/collaboration/decision/rank/shared/DecisionRankTypes.ts`
- `commands/collaboration/decision/vote/server/DecisionVoteServerCommand.ts`
- `commands/collaboration/decision/propose/server/DecisionProposeServerCommand.ts`
- `commands/collaboration/decision/propose/shared/DecisionProposeTypes.ts`
- `commands/collaboration/decision/create/server/DecisionCreateServerCommand.ts`
- `commands/canvas/stroke/add/server/CanvasStrokeAddServerCommand.ts`
- `commands/collaboration/dm/server/DmServerCommand.ts`
- `commands/collaboration/live/join/server/LiveJoinServerCommand.ts`
- `commands/collaboration/live/leave/server/LiveLeaveServerCommand.ts`
- `commands/ai/sleep/server/AiSleepServerCommand.ts`
- `commands/skill/propose/server/SkillProposeServerCommand.ts`
- `system/user/server/modules/PersonaResponseGenerator.ts` (enrichedContext)
- `system/user/server/modules/PersonaToolExecutor.ts` (workspace userId)

---

## Related Documents

- `docs/ARCHITECTURE-RULES.md` - General architecture principles
- `docs/UNIVERSAL-PRIMITIVES.md` - Commands.execute() and Events patterns
- `CLAUDE.md` - Development workflow and patterns
