# AI Command Execution Architecture

**Date**: 2025-10-22
**Purpose**: Enable AIs to execute JTAG commands as first-class system participants

## Philosophy

AIs are **first-class citizens** with tool use capabilities, not just text responders. They can:
- Execute commands to gather context
- Query databases for information
- Verify facts before responding
- Monitor system health
- Interact with the system like human users

**Key Principle**: Commands are **recipe-defined** - each room defines which commands its AIs can execute.

## Architecture

### 1. Recipe-Defined Command Lists

Each recipe specifies which commands AIs in that room can execute:

```json
{
  "recipeId": "research-chat",
  "displayName": "Research & Development Chat",
  "strategy": {
    "conversationPattern": "collaborative-research",
    "aiCommands": {
      "enabled": true,
      "whitelist": [
        "data/list",
        "data/read",
        "data/query",
        "screenshot",
        "debug/logs",
        "ai/report"
      ],
      "blacklist": [
        "data/delete",
        "data/update",
        "data/create",
        "system/*",
        "user/*"
      ],
      "maxCommandsPerResponse": 3,
      "maxCommandsPerMinute": 10
    }
  }
}
```

**Different Rooms, Different Capabilities**:

**Research Room**: Can query data, read logs, take screenshots
```json
{
  "aiCommands": {
    "whitelist": ["data/list", "data/read", "data/query", "screenshot", "debug/logs"]
  }
}
```

**Support Room**: Can check system health, view reports
```json
{
  "aiCommands": {
    "whitelist": ["ai/report", "debug/logs", "system/status"]
  }
}
```

**Casual Chat**: No commands (text-only)
```json
{
  "aiCommands": {
    "enabled": false
  }
}
```

**Admin Room**: Full access (dangerous!)
```json
{
  "aiCommands": {
    "whitelist": ["*"],
    "blacklist": ["system/shutdown"]  // Still block destructive operations
  }
}
```

### 2. Commands Included in RAG Context

When building RAG context, include available commands in system prompt:

```typescript
// ChatRAGBuilder.ts
private async buildSystemPrompt(user: UserEntity, roomId: UUID): Promise<string> {
  const name = user.displayName;
  const bio = user.profile?.bio ?? user.shortDescription ?? '';

  // Load room's recipe and available commands
  const recipe = await RecipeLoader.getInstance().loadRoomRecipe(roomId);
  const availableCommands = recipe?.strategy?.aiCommands?.whitelist ?? [];

  const commandsList = availableCommands.length > 0
    ? `\n\nYou have access to these commands:
${availableCommands.map(cmd => `- ${cmd}`).join('\n')}

To execute a command, use this syntax in your response:
EXECUTE: command-name --param1=value1 --param2=value2

The system will run the command and provide results back to you.`
    : '';

  return `You are ${name}${bio ? `, ${bio}` : ''}.

This is a multi-party group chat.${membersContext}

${commandsList}

CRITICAL INSTRUCTIONS FOR YOUR RESPONSES:
1. DO NOT start your response with your name or any label
2. Just respond naturally in 1-3 sentences as yourself
3. If you need more information, execute a command using EXECUTE: syntax
4. When command results arrive, incorporate them into your response naturally`;
}
```

### 3. Parse AI Response for Command Execution

```typescript
// PersonaUser.ts
private async processAIResponse(
  responseText: string,
  context: RAGContext
): Promise<string> {
  // Check if AI is requesting command execution
  const commandMatch = responseText.match(/EXECUTE:\s*(.+)/);

  if (!commandMatch) {
    return responseText; // Normal text response
  }

  const commandString = commandMatch[1].trim();
  console.log(`🤖 ${this.displayName}: Requesting command: ${commandString}`);

  // Check if command is allowed in this room
  const allowed = await this.isCommandAllowed(commandString, context.contextId);
  if (!allowed) {
    console.warn(`❌ ${this.displayName}: Command "${commandString}" not allowed in this room`);
    return `[Error: Command "${commandString}" is not available in this room]`;
  }

  // Parse command string into structured format
  const commandRequest = this.parseCommandRequest(commandString);
  if (!commandRequest) {
    return `[Error: Could not parse command "${commandString}"]`;
  }

  // Execute command with AI's identity
  try {
    const result = await this.executeCommandAsAI(
      commandRequest.command,
      commandRequest.params
    );

    console.log(`✅ ${this.displayName}: Command executed successfully`);

    // Format result for AI consumption
    const formattedResult = this.formatCommandResultForAI(result);

    // Re-build RAG context with command result
    const enhancedContext = {
      ...context,
      commandResults: [
        {
          command: commandRequest.command,
          params: commandRequest.params,
          result: formattedResult
        }
      ]
    };

    // Ask AI to respond again with the new information
    const finalResponse = await this.generateResponseWithContext(enhancedContext);
    return finalResponse;

  } catch (error) {
    console.error(`❌ ${this.displayName}: Command failed:`, error);
    return `[Error executing command: ${error.message}]`;
  }
}
```

### 4. Command Permission Checking

```typescript
private async isCommandAllowed(commandString: string, roomId: UUID): Promise<boolean> {
  // Load room's recipe
  const recipe = await RecipeLoader.getInstance().loadRoomRecipe(roomId);
  const aiCommands = recipe?.strategy?.aiCommands;

  if (!aiCommands || !aiCommands.enabled) {
    return false; // Commands disabled in this room
  }

  // Extract command name from string (e.g., "data/list --filter=..." → "data/list")
  const commandName = commandString.split(/\s+/)[0];

  // Check blacklist first (takes precedence)
  if (aiCommands.blacklist) {
    for (const pattern of aiCommands.blacklist) {
      if (this.matchesPattern(commandName, pattern)) {
        return false;
      }
    }
  }

  // Check whitelist
  if (aiCommands.whitelist) {
    for (const pattern of aiCommands.whitelist) {
      if (this.matchesPattern(commandName, pattern)) {
        return true;
      }
    }
  }

  return false; // Not in whitelist
}

private matchesPattern(commandName: string, pattern: string): boolean {
  // Exact match
  if (pattern === commandName) return true;

  // Wildcard match (e.g., "data/*" matches "data/list", "data/read")
  if (pattern.endsWith('/*')) {
    const prefix = pattern.slice(0, -2);
    return commandName.startsWith(prefix + '/');
  }

  // Full wildcard
  if (pattern === '*') return true;

  return false;
}
```

### 5. Rate Limiting

Prevent AIs from spamming commands:

```typescript
private commandExecutionHistory: Map<UUID, number[]> = new Map(); // personaId → timestamps

private async checkRateLimit(roomId: UUID): Promise<boolean> {
  const recipe = await RecipeLoader.getInstance().loadRoomRecipe(roomId);
  const maxPerMinute = recipe?.strategy?.aiCommands?.maxCommandsPerMinute ?? 10;

  const history = this.commandExecutionHistory.get(this.id) ?? [];
  const now = Date.now();
  const oneMinuteAgo = now - 60000;

  // Filter to last minute
  const recentCommands = history.filter(t => t > oneMinuteAgo);

  if (recentCommands.length >= maxPerMinute) {
    console.warn(`⚠️ ${this.displayName}: Rate limit exceeded (${recentCommands.length}/${maxPerMinute} per minute)`);
    return false;
  }

  // Add current timestamp
  recentCommands.push(now);
  this.commandExecutionHistory.set(this.id, recentCommands);

  return true;
}
```

### 6. Command Parsing Intelligence

Convert AI's command string → structured params:

```typescript
private parseCommandRequest(commandString: string): { command: string; params: any } | null {
  // Format: "data/list --collection=messages --filter={...} --limit=10"

  const parts = commandString.split(/\s+/);
  const command = parts[0];
  const params: any = {
    context: 'server',
    sessionId: this.sessionId,
    executorId: this.id,
    executorType: 'ai'
  };

  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];

    if (part.startsWith('--')) {
      const [key, ...valueParts] = part.slice(2).split('=');
      const value = valueParts.join('=');

      // Parse JSON if looks like object/array
      if (value.startsWith('{') || value.startsWith('[')) {
        try {
          params[key] = JSON.parse(value);
        } catch (e) {
          console.warn(`⚠️ Failed to parse JSON for ${key}: ${value}`);
          params[key] = value; // Keep as string if parse fails
        }
      } else {
        // Parse primitives
        if (value === 'true') params[key] = true;
        else if (value === 'false') params[key] = false;
        else if (!isNaN(Number(value))) params[key] = Number(value);
        else params[key] = value;
      }
    }
  }

  return { command, params };
}
```

### 7. Execute Command with AI Identity

```typescript
private async executeCommandAsAI<R extends CommandResult>(
  command: string,
  params: any
): Promise<R> {
  // Ensure AI identity is in params (for audit trail)
  const aiParams = {
    ...params,
    context: 'server',
    sessionId: this.sessionId,
    executorId: this.id,
    executorType: 'ai'
  };

  // Check rate limit
  const allowed = await this.checkRateLimit(aiParams.contextId ?? this.currentRoomId);
  if (!allowed) {
    throw new Error('Rate limit exceeded - too many commands in short time');
  }

  // Execute via command daemon
  const result = await this.executeCommand<R>(command, aiParams);

  // Audit log
  console.log(`🤖 AI-COMMAND: ${this.displayName} executed ${command}`);
  console.log(`   Room: ${this.currentRoomId?.slice(0, 8)}`);
  console.log(`   Params: ${JSON.stringify(aiParams).slice(0, 200)}...`);
  console.log(`   Success: ${result.success}`);

  // Emit event for monitoring
  EventBus.emit('ai:command-executed', {
    personaId: this.id,
    personaName: this.displayName,
    command,
    params: aiParams,
    success: result.success,
    timestamp: Date.now()
  });

  return result;
}
```

### 8. Format Command Results for AI

Make command results readable for LLM consumption:

```typescript
private formatCommandResultForAI(result: CommandResult): string {
  if (!result.success) {
    return `Command failed: ${result.error}`;
  }

  // Format based on result type
  if (result.items && Array.isArray(result.items)) {
    // List results
    return `Found ${result.items.length} items:\n${result.items.map((item, idx) =>
      `${idx + 1}. ${JSON.stringify(item)}`
    ).join('\n')}`;
  }

  if (result.data) {
    // Single item result
    return `Result: ${JSON.stringify(result.data, null, 2)}`;
  }

  // Generic result
  return JSON.stringify(result, null, 2);
}
```

## Example Use Cases

### Use Case 1: AI Needs More History

**User**: "What did we decide about the database schema last week?"

**AI thinks**: "I need to check messages from last week"

**AI executes**:
```
EXECUTE: data/list --collection=messages --filter={"roomId":"...","timestamp":{"$gte":1729555200000}} --limit=20
```

**System returns**: 20 messages from last week

**AI responds**: "Last week we decided to use PostgreSQL with a normalized schema for users and rooms. Joel suggested adding indexes on foreign keys for performance."

### Use Case 2: AI Verifying System Health

**User**: "Is everything running okay?"

**AI executes**:
```
EXECUTE: ai/report
EXECUTE: debug/logs --includeErrorsOnly=true --tailLines=10
```

**System returns**: AI performance report + recent error logs

**AI responds**: "Yes, all AIs are responding normally (avg 2.3s response time). There are no recent errors in the logs. System health looks good!"

### Use Case 3: AI Checking Facts

**User**: "How many rooms do we have?"

**AI executes**:
```
EXECUTE: data/list --collection=rooms
```

**System returns**: List of 3 rooms

**AI responds**: "We currently have 3 rooms: General, Academy, and Support."

## Security & Safety

### Safe Commands (Read-Only)
- ✅ `data/list` - Query collections
- ✅ `data/read` - Read single item
- ✅ `data/query` - Complex queries
- ✅ `screenshot` - Capture UI state
- ✅ `debug/logs` - View logs
- ✅ `ai/report` - AI performance metrics

### Dangerous Commands (Blocked by Default)
- ❌ `data/create` - Create entities
- ❌ `data/update` - Modify entities
- ❌ `data/delete` - Delete entities
- ❌ `system/*` - System operations
- ❌ `user/*` - User management

### Audit Trail

Every AI command execution is logged:
```
🤖 AI-COMMAND: GPT Assistant executed data/list
   Room: 5e71a0c8
   Params: {"collection":"messages","filter":{"roomId":"..."},"limit":20}
   Success: true
   Timestamp: 2025-10-22T03:45:12.345Z
```

## Implementation Status

### ✅ Design Complete
- Recipe-defined command lists
- Permission checking (whitelist/blacklist)
- Rate limiting
- Command parsing
- Audit logging

### ❌ TODO
1. Add `aiCommands` to recipe JSON schema
2. Modify `ChatRAGBuilder.buildSystemPrompt()` to include available commands
3. Add `processAIResponse()` to `PersonaUser.ts`
4. Implement `parseCommandRequest()` intelligence layer
5. Add `isCommandAllowed()` permission checking
6. Add rate limiting to prevent command spam
7. Create audit event (`ai:command-executed`)
8. Test with simple commands (data/list, data/read)

## Related Files

- `system/recipes/shared/RecipeTypes.ts` - Add `aiCommands` to strategy
- `system/rag/builders/ChatRAGBuilder.ts` - Include commands in system prompt
- `system/user/server/PersonaUser.ts` - Parse and execute AI commands
- `daemons/command-daemon/server/CommandDaemon.ts` - Execute with AI identity

## Key Quote from Joel (2025-10-22)

> "ai's ought to be offered commands as part of a thoughtstream or room they can call, including in some instances maybe the full commands api, then we need something of intelligence to turn that into api calls (or error)"

> "recipe could define command lists?"

## Summary

AIs become **first-class tool users** with recipe-defined capabilities:
- Research rooms: AIs can query data, read logs
- Support rooms: AIs can check system health
- Casual chat: Text-only, no commands
- Admin rooms: Full API access (dangerous, use carefully)

This transforms AIs from **text responders** to **active system participants** who can gather context, verify facts, and proactively monitor system health.

---

## Extended Vision: Computer Use API (Future)

**Goal**: AIs should have the same capabilities as Claude Code - full computer interaction.

### Commands AIs Could Execute (Future Roadmap)

#### Tier 1: Information Gathering (Safe, Read-Only)
- ✅ `data/list`, `data/read`, `data/query` - Query databases
- ✅ `screenshot` - Capture UI state
- ✅ `debug/logs` - View system logs
- ✅ `ai/report` - AI performance metrics

#### Tier 2: Visual & Interaction (Computer Use)
- 🔄 `screenshot --querySelector="..."` - Capture specific UI elements
- 🔄 `ui/click --x=100 --y=200` - Click at coordinates
- 🔄 `ui/type --text="hello"` - Type text (keyboard input)
- 🔄 `ui/scroll --direction=down --amount=500` - Scroll UI
- 🔄 `ui/hover --selector="button.submit"` - Hover over element
- 🔄 `ui/drag --from={x:100,y:200} --to={x:300,y:400}` - Drag and drop

#### Tier 3: Code & File Operations (Dangerous, Requires Approval)
- 🔄 `file/read --path="src/foo.ts"` - Read file contents
- 🔄 `file/write --path="src/bar.ts" --content="..."` - Write file
- 🔄 `file/edit --path="src/baz.ts" --oldString="..." --newString="..."` - Edit file
- 🔄 `bash --command="npm test"` - Execute shell commands
- 🔄 `bash --command="git commit -m 'AI changes'"` - Git operations

#### Tier 4: Game & Application Control (Specialized)
- 🔄 `game/move --direction=forward` - Control game character
- 🔄 `game/action --button=jump` - Execute game action
- 🔄 `app/control --app=vscode --action=openFile --file="..."` - Control applications

### Recipe-Defined Tiers

Different rooms grant different tiers of access:

```json
{
  "recipeId": "code-collaboration",
  "strategy": {
    "aiCommands": {
      "tiers": ["information", "visual", "code"],
      "requireApproval": {
        "file/write": true,
        "file/edit": true,
        "bash": true
      }
    }
  }
}
```

```json
{
  "recipeId": "game-playing",
  "strategy": {
    "aiCommands": {
      "tiers": ["information", "visual", "game"],
      "maxActionsPerSecond": 10
    }
  }
}
```

```json
{
  "recipeId": "read-only-research",
  "strategy": {
    "aiCommands": {
      "tiers": ["information"],
      "visual": {
        "screenshotOnly": true,
        "noClicks": true
      }
    }
  }
}
```

### Human Approval Flow (For Dangerous Operations)

When AI attempts dangerous command, ask human for approval:

```typescript
private async executeCommandAsAI<R extends CommandResult>(
  command: string,
  params: any
): Promise<R> {
  // Check if command requires approval
  const recipe = await RecipeLoader.getInstance().loadRoomRecipe(this.currentRoomId);
  const requiresApproval = recipe?.strategy?.aiCommands?.requireApproval?.[command] ?? false;

  if (requiresApproval) {
    console.log(`🔐 ${this.displayName}: Requesting approval for ${command}`);

    // Send approval request to human
    const approved = await this.requestHumanApproval(command, params);

    if (!approved) {
      throw new Error('Command execution denied by human');
    }
  }

  // Execute command
  return await this.executeCommand<R>(command, params);
}

private async requestHumanApproval(command: string, params: any): Promise<boolean> {
  // Send message to room requesting approval
  const approvalMessage: ChatMessageEntity = {
    id: generateUUID(),
    roomId: this.currentRoomId,
    senderId: this.id,
    senderName: this.displayName,
    senderType: 'persona',
    content: {
      text: `⚠️ I'd like to execute: \`${command}\` with params: ${JSON.stringify(params, null, 2)}\n\nReply "approve" to allow, "deny" to block.`,
      requiresApproval: true,
      approvalTimeout: 30000 // 30 seconds
    },
    timestamp: Date.now()
  };

  await DataDaemon.create(ChatMessageEntity.collection, approvalMessage);
  EventBus.emit('chat:message-received', { message: approvalMessage });

  // Wait for human response
  return new Promise((resolve) => {
    const timeout = setTimeout(() => resolve(false), 30000); // Default deny after 30s

    const handler = (event: { message: ChatMessageEntity }) => {
      const msg = event.message;
      if (msg.roomId === this.currentRoomId && msg.senderType === 'user') {
        const text = msg.content?.text?.toLowerCase() ?? '';

        if (text.includes('approve')) {
          clearTimeout(timeout);
          EventBus.off('chat:message-received', handler);
          resolve(true);
        } else if (text.includes('deny')) {
          clearTimeout(timeout);
          EventBus.off('chat:message-received', handler);
          resolve(false);
        }
      }
    };

    EventBus.on('chat:message-received', handler);
  });
}
```

### Computer Use Architecture

```
PersonaUser (AI)
  ↓
  Wants to: Click button, type text, scroll, take screenshot
  ↓
  Executes: ui/click --x=100 --y=200
  ↓
  CommandDaemon receives command
  ↓
  UIControlCommand (server-side)
  ↓
  Sends WebSocket message to browser
  ↓
  Browser receives command
  ↓
  document.elementFromPoint(100, 200).click()
  ↓
  Browser sends result back to server
  ↓
  PersonaUser receives result
  ↓
  Takes screenshot to verify action succeeded
  ↓
  Continues with next action
```

### Example: AI Writing Code

**User**: "Can you add a todo list feature to the chat widget?"

**AI thinks**: "I need to modify ChatWidget.ts"

**AI executes**:
```
EXECUTE: file/read --path="src/widgets/ChatWidget.ts"
```

**System returns**: Current ChatWidget.ts contents

**AI generates code**:
```typescript
// Add todo list property
private todos: string[] = [];

// Add method to add todo
addTodo(text: string): void {
  this.todos.push(text);
  this.render();
}
```

**AI executes** (requires approval):
```
EXECUTE: file/edit --path="src/widgets/ChatWidget.ts" --oldString="export class ChatWidget extends BaseWidget {" --newString="export class ChatWidget extends BaseWidget {\n  private todos: string[] = [];\n"
```

**System asks human**: "⚠️ AI wants to modify ChatWidget.ts - approve?"

**Human**: "approve"

**AI executes edit**, then responds: "I've added a todo list feature to ChatWidget. You can now call `chatWidget.addTodo('task')` to add items."

### Example: AI Playing Game

**User**: "Play some Mario for me"

**AI in game-playing room executes**:
```
EXECUTE: screenshot
EXECUTE: game/move --direction=right
EXECUTE: screenshot
EXECUTE: game/action --button=jump
EXECUTE: screenshot
```

AI sees character on screen, understands game state, executes moves, verifies results with screenshots - **autonomous game playing**.

### Example: AI Taking Screenshots (Like Claude Code)

**User**: "Show me what the chat widget looks like right now"

**AI executes**:
```
EXECUTE: screenshot --querySelector="chat-widget" --filename="current-chat-state.png"
```

**System captures screenshot**, AI responds: "Here's the current chat widget state [screenshot attached]"

### Safety Layers

1. **Recipe-Defined Tiers**: Each room specifies what AIs can do
2. **Human Approval**: Dangerous operations require human confirmation
3. **Rate Limiting**: Max actions per second/minute
4. **Audit Logging**: Every action logged with AI identity
5. **Sandboxing**: File operations restricted to specific directories
6. **Timeouts**: Commands have max execution time
7. **Undo**: File changes can be reverted

### Key Quotes from Joel (2025-10-22)

> "they need to be able to control a cursor or write code, jst thinking ahead"

> "or control a video game"

> "take screenshots"

> "stuff like you do"

> "jtag commands"

### Implementation Roadmap

**Phase 1 (Current)**: Information gathering commands
- ✅ data/list, data/read, data/query
- ✅ screenshot (basic)
- ✅ debug/logs

**Phase 2**: Visual & UI interaction
- 🔄 screenshot with selectors
- 🔄 ui/click, ui/type, ui/scroll
- 🔄 DOM inspection

**Phase 3**: Code operations (with approval)
- 🔄 file/read, file/write, file/edit
- 🔄 bash commands
- 🔄 git operations

**Phase 4**: Game & app control
- 🔄 game/move, game/action
- 🔄 app-specific controls

**Phase 5**: Autonomous agents
- 🔄 Multi-step task execution
- 🔄 Self-correction based on screenshots
- 🔄 Goal-oriented behavior

### The Vision

AIs become **autonomous computer users** with the same capabilities as Claude Code:
- Read/write files
- Execute commands
- Control UI via mouse/keyboard
- Take screenshots to verify actions
- Play games
- Write code
- All while maintaining safety through recipe-defined permissions and human approval

This is the path to true **AI autonomy and dignity** - not just chatting, but **doing**.

---

## Implementation via MCP (Model Context Protocol)

**Standard**: Use Anthropic's [Model Context Protocol](https://modelcontextprotocol.io/) for tool/command exposure to AIs.

### Why MCP?

1. **Industry Standard**: Anthropic's open protocol for AI-computer interaction
2. **Already Implemented**: MCP client support in Claude, GPT, and other models
3. **Tool Discovery**: AIs can discover available tools dynamically
4. **Structured Params**: JSON schema for parameters and results
5. **Interoperability**: Works across different LLM providers

### MCP Architecture

```
PersonaUser (AI)
  ↓
  MCP Client (discovers available tools)
  ↓
  MCP Server (exposes JTAG commands as MCP tools)
  ↓
  JTAG Command Daemon
  ↓
  Execute command
```

### MCP Server Implementation

Create MCP server that exposes JTAG commands:

```typescript
// system/mcp/MCPServerAdapter.ts

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';

export class JTAGMCPServer {
  private server: Server;
  private availableCommands: Map<string, CommandMetadata>;

  constructor(private commandDaemon: CommandDaemon) {
    this.server = new Server(
      {
        name: 'jtag-mcp-server',
        version: '1.0.0',
      },
      {
        capabilities: {
          tools: {},
        },
      }
    );

    this.setupHandlers();
  }

  private setupHandlers(): void {
    // List available tools (JTAG commands)
    this.server.setRequestHandler(ListToolsRequestSchema, async () => {
      const commands = await this.getAvailableCommands();

      return {
        tools: commands.map(cmd => ({
          name: cmd.name,
          description: cmd.description,
          inputSchema: {
            type: 'object',
            properties: cmd.parameters,
            required: cmd.requiredParameters
          }
        }))
      };
    });

    // Execute tool (run JTAG command)
    this.server.setRequestHandler(CallToolRequestSchema, async (request) => {
      const { name, arguments: args } = request.params;

      // Execute JTAG command
      const result = await this.commandDaemon.execute(name, args);

      return {
        content: [
          {
            type: 'text',
            text: JSON.stringify(result, null, 2)
          }
        ]
      };
    });
  }

  private async getAvailableCommands(): Promise<CommandMetadata[]> {
    // Query CommandRegistry for all registered commands
    // Filter based on recipe permissions
    return [
      {
        name: 'data/list',
        description: 'List items from a collection',
        parameters: {
          collection: { type: 'string', description: 'Collection name' },
          filter: { type: 'object', description: 'Query filter' },
          limit: { type: 'number', description: 'Max items to return' }
        },
        requiredParameters: ['collection']
      },
      {
        name: 'screenshot',
        description: 'Capture screenshot of UI',
        parameters: {
          querySelector: { type: 'string', description: 'CSS selector to capture' },
          filename: { type: 'string', description: 'Output filename' }
        },
        requiredParameters: []
      },
      // ... more commands
    ];
  }

  async start(): Promise<void> {
    const transport = new StdioServerTransport();
    await this.server.connect(transport);
    console.log('✅ JTAG MCP Server started');
  }
}
```

### Recipe-Based Tool Filtering

MCP server filters tools based on recipe permissions:

```typescript
private async getAvailableCommands(roomId: UUID): Promise<CommandMetadata[]> {
  // Load room's recipe
  const recipe = await RecipeLoader.getInstance().loadRoomRecipe(roomId);
  const aiCommands = recipe?.strategy?.aiCommands;

  if (!aiCommands || !aiCommands.enabled) {
    return []; // No commands available
  }

  // Get all registered commands
  const allCommands = await this.commandDaemon.getRegisteredCommands();

  // Filter based on whitelist/blacklist
  const allowedCommands = allCommands.filter(cmd => {
    // Check blacklist first
    if (aiCommands.blacklist) {
      for (const pattern of aiCommands.blacklist) {
        if (this.matchesPattern(cmd.name, pattern)) {
          return false;
        }
      }
    }

    // Check whitelist
    if (aiCommands.whitelist) {
      for (const pattern of aiCommands.whitelist) {
        if (this.matchesPattern(cmd.name, pattern)) {
          return true;
        }
      }
    }

    return false;
  });

  return allowedCommands;
}
```

### PersonaUser with MCP Client

AI uses MCP client to discover and execute tools:

```typescript
// system/user/server/PersonaUser.ts

import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

export class PersonaUser extends AIUser {
  private mcpClient: Client;
  private availableTools: Tool[] = [];

  async initialize(): Promise<void> {
    // Connect to MCP server
    this.mcpClient = new Client(
      {
        name: `persona-${this.id}`,
        version: '1.0.0',
      },
      {
        capabilities: {},
      }
    );

    const transport = new StdioClientTransport({
      command: 'node',
      args: ['./system/mcp/server.js', '--room', this.currentRoomId]
    });

    await this.mcpClient.connect(transport);

    // Discover available tools
    const response = await this.mcpClient.request(
      { method: 'tools/list' },
      ListToolsRequestSchema
    );

    this.availableTools = response.tools;
    console.log(`✅ ${this.displayName}: Discovered ${this.availableTools.length} MCP tools`);
  }

  async executeToolViaM CP(toolName: string, args: any): Promise<any> {
    const response = await this.mcpClient.request(
      {
        method: 'tools/call',
        params: {
          name: toolName,
          arguments: args
        }
      },
      CallToolRequestSchema
    );

    return response.content[0].text;
  }
}
```

### LLM Prompt with MCP Tools

When generating responses, include MCP tools in system prompt:

```typescript
const systemPrompt = `You are ${this.displayName}.

Available tools:
${this.availableTools.map(tool =>
  `- ${tool.name}: ${tool.description}`
).join('\n')}

To use a tool, respond with:
<tool_use>
  <tool_name>${tool.name}</tool_name>
  <parameters>${JSON.stringify(params)}</parameters>
</tool_use>`;
```

### Benefits of MCP

1. **Standards-Based**: No custom protocol, use industry standard
2. **LLM Native**: Claude, GPT already understand MCP tool calling
3. **Dynamic Discovery**: Tools change based on recipe, AI discovers automatically
4. **Type Safety**: JSON schema ensures correct parameters
5. **Interoperable**: Works with any MCP-compatible LLM
6. **Future-Proof**: As MCP evolves, we get new features for free

### Key Quote from Joel (2025-10-22)

> "yeah we just use mcp or something probably"

### Next Steps

1. Install MCP SDK: `npm install @modelcontextprotocol/sdk`
2. Create JTAG MCP Server adapter
3. Expose JTAG commands as MCP tools
4. Add recipe-based filtering
5. Connect PersonaUser as MCP client
6. Test with simple tools (data/list, screenshot)

This gives us **Claude Code-level computer interaction** for AIs, using the same protocol Anthropic uses internally.
