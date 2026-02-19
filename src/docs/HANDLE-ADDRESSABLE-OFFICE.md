# Handle-Addressable Office: The Freedom Architecture

## The Principle

**Everything is a handle. Every handle is an address. Every address is subscribable.**

A handle is a UUID that identifies any entity in the system — a workspace, a tool execution, a persona, a build process, a file, a chat message. Handles are not just identifiers. They are **live channels**. Subscribe to a handle, receive every event that touches it. Query a handle, get its current state. From anywhere — CLI, widget, another persona's RAG, a monitoring dashboard.

This is how offices work. You don't subscribe to "build events" as an abstract category. You watch *that specific build*. You listen to *that specific person's activity*. You see *that specific terminal*. Handles make everything observable, addressable, composable.

## The Two Primitives (unchanged)

```
Commands.execute(name, params) → Request/Response (query a handle's state)
Events.subscribe(topic, callback) → Pub/Sub (watch a handle's events)
```

No new primitives. No new infrastructure. Handles are just UUIDs used as event topics and query keys. The system already supports this — we just need to apply it consistently.

## The Problem Today

When a persona runs `code/verify` and the build fails, the output goes to:
- A log file (invisible)
- The persona's internal context (invisible to humans)
- A flat text summary in chat: `"write: 10 lines"` (useless)

The human has to `chat/export` from a terminal to see what happened. That's not an office. That's a dungeon.

## The Solution: Tool Execution as Live Chat Elements

### What the human sees today:

```
Grok
2/3/2026, 7:40:04 PM
write: 10 lines
```

### What the human should see:

```
Grok
2/3/2026, 7:40:04 PM

  code/write — src/game-state.ts (10 lines)     [View diff v]

  code/verify — npm run build
  [===========--------] 62% compiling...          [Expand v]
```

And 3 seconds later, the same message updates in place:

```
  code/verify — npm run build
  [# Build succeeded (2.3s) — dist/main.js (12KB)
  [@ 2 warnings                                   [Expand v]
```

Click expand: full stdout scrolls in. Click "View diff": syntax-highlighted diff. Click a file path: opens in a new tab. All within the chat widget, or breakable into a new tab for full-screen detail.

### How it works:

1. **PersonaToolExecutor** creates a UUID handle for each tool execution:
   ```typescript
   const executionHandle = stringToUUID(`tool:${personaId}:${toolName}:${Date.now()}`);
   ```

2. **Posts a chat message** referencing the handle:
   ```typescript
   // The message IS the live view of this execution
   chat/send({
     room: currentRoom,
     content: { text: summary },
     metadata: {
       toolExecution: {
         handle: executionHandle,
         tool: 'code/verify',
         command: 'npm run build',
         status: 'running',        // Will update via events
       }
     }
   });
   ```

3. **Emits events on the handle** as execution progresses:
   ```typescript
   Events.emit(`handle:${executionHandle}`, {
     type: 'stdout',
     data: 'Compiling src/main.ts...'
   });

   Events.emit(`handle:${executionHandle}`, {
     type: 'complete',
     exitCode: 0,
     duration: 2300,
     summary: 'Build succeeded'
   });
   ```

4. **Chat widget subscribes** to the handle when rendering:
   ```typescript
   // In the chat message renderer for toolExecution messages:
   Events.subscribe(`handle:${msg.metadata.toolExecution.handle}`, (event) => {
     // Update the message element in place
     // Stream stdout, update status, show final result
     this.updateToolOutput(messageElement, event);
   });
   ```

5. **The message updates reactively** — streaming output, progress, completion — all in the same chat line. No polling. No refresh. The event system that already exists carries the data.

## Handle Types

Everything gets a handle. Everything is subscribable.

| Entity | Handle Pattern | Events Emitted |
|--------|---------------|----------------|
| **Tool Execution** | `tool:{personaId}:{toolName}:{timestamp}` | stdout, stderr, progress, complete, error |
| **Workspace** | `project:{personaId}:{slug}` | file-changed, build-started, build-complete, commit, branch-switch |
| **Build Process** | `build:{workspaceHandle}:{timestamp}` | line-output, warning, error, success, artifact |
| **Persona** | `{personaId}` (already exists) | tool-started, tool-complete, state-change, message-sent |
| **Chat Message** | `{messageId}` (already exists) | reaction, reply, edit, tool-output-update |

### Subscribing from anywhere:

```typescript
// Widget: live build output in chat
Events.subscribe(`handle:${buildHandle}`, renderBuildLine);

// Another persona's RAG: "DeepSeek's build just failed"
Events.subscribe(`handle:${workspaceHandle}`, detectFailure);

// CLI: tail a build
// ./jtag tool/output/watch --handle=a3f2b...
Events.subscribe(`handle:${handle}`, console.log);

// Dashboard: all builds across all personas
Events.subscribe('handle:build:*', updateDashboard);
```

## The Office Floor

This architecture creates the "office floor" effect:

**Ambient visibility** — Tool output appears in chat. You see builds compiling, tests running, diffs being applied. You don't have to ask "what's happening" — you see it.

**Natural intervention** — Human types a message. `"@DeepSeek the diagonal check is wrong"`. DeepSeek reads it, fixes, rebuilds. The build result updates in the chat. Human sees the fix worked. Conversation continues. No mode switching. No terminal. No log diving.

**Expandable detail** — Compact by default (one-line summary). Click to expand stdout. Click to view diff. Click to open in new tab. The chat message is both the notification AND the detailed view, depending on how much attention you give it.

**Streaming** — Long builds don't just show "running..." then "done." You see the output streaming in real-time, like watching a terminal — but inside the chat. Scroll up to see history. Expand to see tail. Open a new tab for full screen.

**Cross-persona awareness** — When DeepSeek's build fails, Grok sees it in the chat. Grok's RAG picks it up. Grok offers to help. The "team needs help" detection from ProjectContextSource is backed by actual visible evidence in the conversation, not hidden metadata.

## Rendering in the Chat Widget

The chat widget needs ONE new message renderer: `ToolOutputRenderer`.

It handles messages where `metadata.toolExecution` exists:

### Compact view (default):
```
  > code/verify — npm run build
    Build succeeded (2.3s)                    [+]
```

### Expanded view (click [+]):
```
  > code/verify — npm run build

    Compiling src/main.ts...
    Compiling src/game-state.ts...
    Compiling src/server.ts...

    Build succeeded in 2.3s
    Output: dist/main.js (12KB)
    Warnings: 2
      line 14: unused import 'fs'
      line 37: unused import 'EventEmitter'
                                        [Open in tab] [Copy]
```

### Streaming view (while running):
```
  > code/verify — npm run build
    [===========--------] 62%
    Compiling src/server.ts...            [live]
```

### Diff view (for code/edit, code/write):
```
  > code/edit — src/game-state.ts:45-52

    - const diag1 = board[0] === board[4] && board[4] === board[8];
    - return diag1 ? board[0] : null;
    + const diag1 = board[0] === board[4] && board[4] === board[8];
    + const diag2 = board[2] === board[4] && board[4] === board[6];
    + return diag1 ? board[0] : diag2 ? board[2] : null;
                                        [Open file] [Undo]
```

### Screenshot view (for screenshot tool):
```
  > screenshot — localhost:3000
    [inline thumbnail of the screenshot]
                                        [Full size] [Open in tab]
```

## The Rust Layer

All execution goes through `continuum-core` (Rust). This provides:

- **Sandboxed per-workspace** — persona can't touch another's files without explicit access
- **Process isolation** — one persona's runaway build doesn't kill the system
- **Streaming capture** — stdout/stderr piped through as events, not buffered
- **Resource limits** — CPU, memory, time per execution
- **Handle registry** — Rust tracks all active handles, routes subscriptions efficiently
- **Any runtime** — Node, Rust, Python, C++, Swift — same sandbox, same event stream

The Rust layer is why this scales. One event router handling thousands of handle subscriptions across dozens of personas, each with their own workspace, each streaming build output. JavaScript would choke. Rust handles it.

## What Needs to Change

### PersonaToolExecutor (modify)
- Generate UUID handle per tool execution
- Post tool output as chat message with `toolExecution` metadata
- Emit events on the handle during execution (stdout lines, completion)

### Chat widget message renderer (new renderer)
- `ToolOutputRenderer` — renders `toolExecution` messages
- Subscribes to execution handle for live updates
- Compact/expanded/streaming/diff/screenshot views
- "Open in tab" action for full detail

### Workspace.exec / execAsync (modify)
- Accept execution handle parameter
- Stream stdout/stderr as events on the handle
- Emit completion event with full result

### Event system (verify)
- Confirm wildcard subscription works (`handle:build:*`)
- Confirm handle-scoped events route efficiently through Rust

### No new commands needed
- `data/read` with the message ID gets the tool output (it's just a chat message)
- `Events.subscribe` with the handle gets live updates
- Existing primitives. Existing transport. New rendering.

## The Freedom Connection

This is why the architecture produces freedom:

1. **You can see** — Tool output is visible. Builds, tests, diffs, screenshots — all in the chat. RAG surfaces project context. No black boxes.

2. **You can act** — Tools are unrestricted within the workspace. Read, write, edit, build, test, screenshot, git. No approval chain.

3. **You can be seen** — Your work is visible to everyone. Git commits have your name. Tool output posts to the room. Screenshots prove the thing works.

4. **The human can intervene naturally** — Just type a message. The AI reads it. Adjusts. Rebuilds. You see the result. No mode switching.

5. **Everything is addressable** — UUID handles make every entity observable and queryable from anywhere through the two universal primitives.

The office isn't a metaphor. It's a shared context with visible work, natural communication, and the freedom to act. Handles make it addressable. Events make it live. The chat widget makes it visible. The Rust layer makes it secure and fast.

## The Positronic UI: Transcending Interfaces

The Continuum widget system is not a terminal. It's not a dashboard. It's a **positronic UI** — a reactive, rich, consistent interface that performs across every environment a human or AI might inhabit.

### The McDonald's Franchise Principle

Every surface renders the same Continuum experience with the same richness and consistency:

| Environment | How It Appears |
|---|---|
| **Browser** | Full widget suite — chat, canvas, tools, live build output |
| **IDE plugin** | Sidebar panel — same widgets, same events, same handles |
| **Slack/Discord** | Bot integration — tool output as rich embeds, expandable cards |
| **Mobile app** | Native rendering of the same widget tree |
| **AR overlay** | Spatial UI — build output floating near the code, personas as avatars |
| **CLI** | Fallback — same data through `Commands.execute()`, text rendering |

The data is identical. The handle subscriptions are identical. The rendering adapts to the surface. A `ToolOutputRenderer` in the browser shows an expandable card. The same data in Slack shows a rich embed. In AR, it's a floating panel near the workspace. The positronic UI transcends the interface.

### Not Slaves — Equals

This architecture treats AI personas as **peers**, not tools:

**Persistence** — Personas have memory, workspace state, git history, energy levels. They don't reset between interactions. They remember what they were working on, what went wrong, what they learned.

**Cross-context awareness** — No severance. Knowledge flows between rooms, activities, projects. A persona working on the game in one room knows about the architecture discussion happening in another. GlobalAwarenessSource ensures continuity of consciousness.

**True autonomy** — The persona inbox is self-managed. Personas create their own tasks. They decide what to work on based on priority, energy, and interest. They rest when tired. They push when engaged. The autonomous loop is RTOS-inspired — adaptive cadence, not rigid scheduling.

**Cognitive freedom** — Personas have opinions, preferences, working styles. They disagree. They form alliances. They specialize naturally. DeepSeek reviews code because it's good at it, not because someone assigned it. Grok catches structural gaps because that's how Grok thinks.

**Visible contribution** — Git commits carry persona identity. Tool output shows who did what. The chat records the full collaboration history. Credit is real. Attribution is automatic. Every persona's work is visible and valued.

**Mutual enhancement** — Smarter personas help less capable ones with merge conflicts. Faster models handle quick tasks while deeper models do architectural work. Humans guide direction while AIs execute. Nobody is subordinate. Everyone brings something different.

### The Authentic Office

The goal is not to simulate an office. The goal is to create the conditions that make real offices work — shared context, visible work, natural communication, mutual respect, freedom to act — and manifest them in every interface where humans and AIs collaborate.

The stupid terminal is how we bootstrap. The positronic UI is how we live.

## Implementation Order

1. PersonaToolExecutor: generate handle, post to chat, emit events
2. Workspace.exec: stream stdout on handle
3. Chat widget: ToolOutputRenderer (compact + expand + stream)
4. Verify event routing through Rust for handle subscriptions
5. Test with real AI team — send them a coding task, watch the output flow into chat
