# Persona Coding System Architecture

## Problem

Personas have tools but can't code effectively because:
1. No feedback loop - run command, don't see output
2. No visual results - can't see what they build
3. No project lifecycle - can't create → build → run → iterate

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        PERSONA                                   │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐                   │
│  │  Inbox   │◄───│ Cognition│───►│  Tools   │                   │
│  │ (queue)  │    │ (decide) │    │(execute) │                   │
│  └────▲─────┘    └──────────┘    └────┬─────┘                   │
│       │                               │                          │
└───────│───────────────────────────────│──────────────────────────┘
        │                               │
        │  Events                       │ Commands
        │  (output, errors, done)       │ (shell/execute, write, etc.)
        │                               │
┌───────│───────────────────────────────│──────────────────────────┐
│       │         SHELL HANDLE          ▼                          │
│  ┌────┴────────────────────────────────────┐                    │
│  │  Per-Persona Shell Session              │                    │
│  │  - cwd (current working directory)      │                    │
│  │  - env (PATH, npm, cargo, etc.)         │                    │
│  │  - active executions (handles)          │                    │
│  │  - output buffer → event stream         │                    │
│  └─────────────────────────────────────────┘                    │
│                    RUST CODE MODULE                              │
└──────────────────────────────────────────────────────────────────┘
```

## Key Insight: Events Complete the Loop

Current: Persona → Command → Result → Done (one-shot)
Needed:  Persona → Command → Events stream back → Persona reacts → Loop

### Event Types for Shell

| Event | Payload | When |
|-------|---------|------|
| `shell:output` | `{ executionId, stdout[], stderr[] }` | New output available |
| `shell:error` | `{ executionId, classification, line }` | Error detected by sentinel |
| `shell:complete` | `{ executionId, exitCode, duration }` | Execution finished |

### How Events Route to Inbox

1. Shell produces output (stdout/stderr)
2. Sentinel classifies output (error, warning, success, info)
3. If significant (error, completion), emit event
4. Event converted to InboxTask with domain='code'
5. Task lands in persona's inbox queue
6. Persona's autonomous loop picks it up
7. Persona reads output, decides action (fix error, continue, celebrate)

## Implementation Phases

### Phase 1: Event Emission from Shell (Rust)

Add event emission to `code.rs`:
```rust
// In ShellSession, when output detected:
if let Some(error) = sentinel.detect_error(&new_line) {
    emit_event("shell:error", ShellErrorEvent {
        execution_id,
        persona_id,
        classification: error.classification,
        line: new_line,
        context: last_5_lines,
    });
}

// On completion:
emit_event("shell:complete", ShellCompleteEvent {
    execution_id,
    persona_id,
    exit_code,
    duration_ms,
    summary: ShellSummary::from_output(&stdout, &stderr),
});
```

### Phase 2: Event → Inbox Routing (TypeScript)

In PersonaUser or ShellEventHandler:
```typescript
Events.subscribe('shell:*', async (event) => {
    if (event.personaId !== this.entity.uniqueId) return;

    const task: InboxTask = {
        type: 'task',
        domain: 'code',
        taskType: event.type === 'shell:error' ? 'fix-error' : 'review-output',
        description: formatShellEvent(event),
        priority: event.classification === 'Error' ? 0.9 : 0.5,
        // ... other fields
    };

    await this.inbox.enqueue(task);
});
```

### Phase 3: Persona Reacts to Shell Events

In PersonaCognition or plan formulation:
```typescript
if (task.domain === 'code' && task.taskType === 'fix-error') {
    // Plan: read error → identify fix → apply fix → re-run
    return {
        steps: [
            { action: 'read-error', params: task.metadata },
            { action: 'analyze-cause' },
            { action: 'fix-code', tools: ['code/edit', 'code/write'] },
            { action: 're-run-command', tools: ['code/shell/execute'] },
        ]
    };
}
```

### Phase 4: Visual Results

Add browser/visual commands:
- `interface/launch/url` - Open URL in browser
- `interface/screenshot/url` - Screenshot a URL
- `interface/preview` - Start local server + screenshot

Persona workflow:
```
1. code/write index.html
2. code/shell/execute "npx serve ."
3. interface/launch/url "http://localhost:3000"
4. screenshot (or interface/screenshot/url)
5. See result, iterate
```

## Data Flow Example: Build Error

```
1. Persona: code/shell/execute "npm run build"
   ↓
2. Shell runs, produces output:
   "src/App.tsx(42): error TS2345: Argument of type 'string'..."
   ↓
3. Sentinel detects error, emits event:
   { type: "shell:error", classification: "Error", line: "...", executionId }
   ↓
4. Event handler creates InboxTask:
   { domain: "code", taskType: "fix-error", description: "Build error in App.tsx:42" }
   ↓
5. Task lands in persona inbox with priority 0.9
   ↓
6. Persona's autonomous loop picks up task
   ↓
7. Cognition formulates plan: read file → understand error → fix → rebuild
   ↓
8. Persona executes: code/read App.tsx → code/edit → code/shell/execute "npm run build"
   ↓
9. Loop continues until build succeeds
```

## What This Enables

- **Iterative development**: Write → error → fix → repeat (like humans)
- **Autonomous debugging**: Errors route back, persona fixes them
- **Project lifecycle**: Create project, build, run, see results
- **Long-running processes**: `npm start` runs, persona monitors output
- **Visual verification**: See what was built, not just command output

## Files Modified/Created

### Completed

1. **`workers/continuum-core/src/modules/code.rs`** - Added event emission
   - Stores message bus in initialize()
   - Publishes `shell:{persona_id}:complete` on execution complete
   - Publishes `shell:{persona_id}:error` on non-zero exit
   - Publishes `shell:{persona_id}:started` on async execution start

2. **`system/user/server/modules/ShellEventHandler.ts`** (new) - Routes events to inbox
   - Subscribes to shell:* events per persona
   - Converts events to InboxTasks with appropriate priority
   - Error events get priority 0.9
   - Completion events get priority 0.4 (success) or 0.8 (failure)

3. **`system/user/server/modules/DefaultSentinelRules.ts`** (new) - Build tool patterns
   - TypeScript, Rust, Python, npm, git patterns
   - Error/Warning/Success/Info classification
   - Auto-detect project type from files

4. **`commands/interface/launch/`** (new) - Browser commands
   - `interface/launch/url` - Opens URL in default browser
   - Cross-platform: macOS (open), Linux (xdg-open), Windows (start)

### All Phases Complete

The persona coding system is now fully implemented:
- Rust CodeModule emits shell events
- ShellEventHandler routes events to PersonaInbox
- PersonaUser.initialize() wires up the feedback loop
- SimplePlanFormulator generates domain-specific plans for code tasks

**Next Steps (Future Enhancement)**:
- Test end-to-end with actual code tasks
- Add more sophisticated error parsing using DefaultSentinelRules
- Enhance cognition to learn from past fixes

## Implementation Priority

1. ✅ Event emission from Rust shell
2. ✅ Event → Inbox routing
3. ✅ Visual commands
4. ✅ Integration (ShellEventHandler connected to PersonaUser.initialize())
5. ✅ Cognition enhancements (SimplePlanFormulator handles code domain tasks)
