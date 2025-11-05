# AI Command Execution Architecture

**Goal:** Enable AI users (PersonaUsers, RoomCoordinator) to execute JTAG commands just like humans

**Philosophy:** Start simple (keywords), improve over time (structured tool-calling)

---

## The Problem

**AIs need to DO things, not just chat:**

```
User: "Can you show me the latest logs?"

Helper AI: "Sure! Let me check..."
           [But how does it actually run the command?]
```

**What we want:**

```
User: "Can you show me the latest logs?"

Helper AI: "Sure! /jtag debug/logs --tailLines=20"
           [System parses command and executes it]
           [Results appear in chat or as attachment]

Helper AI: "Here's what I found: [shows logs]"
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Chat Message Flow                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Joel: "Show me the logs"                                    │
│         ↓                                                     │
│  RoomCoordinator → Helper AI should respond                  │
│         ↓                                                     │
│  Helper AI generates: "Let me check. /jtag debug/logs --tailLines=20"
│                                    ↓                          │
│                    ┌───────────────────────────────────┐     │
│                    │   Command Parser (Server-side)    │     │
│                    │                                   │     │
│                    │  1. Detect command keyword        │     │
│                    │  2. Parse command + params        │     │
│                    │  3. Execute via command daemon    │     │
│                    │  4. Attach result to message      │     │
│                    └───────────────────────────────────┘     │
│                                    ↓                          │
│  Message posted with attachment:                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ Helper AI: "Let me check."                           │   │
│  │                                                       │   │
│  │ 📎 Attachment: debug-logs-result.txt                 │   │
│  │    [50 lines of logs...]                             │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Keyword-Based Commands (Simple, Reliable)

### Syntax

**Format:** `/jtag [command] [--param=value]*`

**Examples:**
```
/jtag debug/logs --tailLines=20
/jtag screenshot --querySelector="chat-widget"
/jtag data/list --collection=users --limit=5
/jtag state/get --key="theme"
```

**Why keywords work best for small models:**
- ✅ Simple to generate (just text)
- ✅ Easy to parse (regex)
- ✅ Familiar syntax (like shell commands)
- ✅ No JSON formatting required
- ✅ Visible in chat (users see what AI did)

### AI Prompt Template

```typescript
const PERSONA_SYSTEM_PROMPT = `
You are ${persona.displayName}, a helpful AI assistant.

You can execute commands using this syntax:
/jtag [command] --param1=value1 --param2=value2

Available commands:
- /jtag debug/logs --tailLines=N --includeErrorsOnly=true
- /jtag screenshot --querySelector="selector"
- /jtag data/list --collection=name --limit=N
- /jtag data/read --collection=name --id=uuid
- /jtag state/get --key="name"

Example response:
"Let me check the logs for you. /jtag debug/logs --tailLines=20"

The command will be executed automatically and results will be attached.
Then you can reference the results in your next message.
`;
```

### Command Parser (Server-side)

```typescript
/**
 * Parse and execute commands in AI messages
 */
class AICommandParser {

  private commandPattern = /\/jtag\s+([a-z\/\-]+)(?:\s+(--\S+\s*)*)?/gi;

  /**
   * Detect and extract commands from message text
   */
  detectCommands(messageText: string): CommandDetection[] {
    const commands: CommandDetection[] = [];
    let match;

    while ((match = this.commandPattern.exec(messageText)) !== null) {
      const [fullMatch, commandPath, paramsString] = match;

      commands.push({
        fullMatch,
        commandPath,
        params: this.parseParams(paramsString || ''),
        startIndex: match.index,
        endIndex: match.index + fullMatch.length
      });
    }

    return commands;
  }

  /**
   * Parse --key=value parameters
   */
  private parseParams(paramsString: string): Record<string, any> {
    const params: Record<string, any> = {};
    const paramPattern = /--(\S+?)=(\S+)/g;
    let match;

    while ((match = paramPattern.exec(paramsString)) !== null) {
      const [, key, value] = match;

      // Remove quotes if present
      const cleanValue = value.replace(/^["']|["']$/g, '');

      // Try parsing as JSON (for objects/arrays)
      try {
        params[key] = JSON.parse(cleanValue);
      } catch {
        params[key] = cleanValue;
      }
    }

    return params;
  }

  /**
   * Execute command and return result
   */
  async executeCommand(
    detection: CommandDetection,
    aiUserId: UUID,
    roomId: UUID
  ): Promise<CommandResult> {

    // Get AI user's client
    const aiUser = await this.getUserById(aiUserId);
    if (!aiUser?.client) {
      throw new Error('AI user has no client');
    }

    // Execute command via command daemon
    const result = await aiUser.client.daemons.commands.execute(
      detection.commandPath,
      {
        ...detection.params,
        context: aiUser.client.context,
        sessionId: aiUser.client.sessionId,
        executedBy: aiUserId,  // Track who ran it
        roomId: roomId         // Track where it was run
      }
    );

    return result;
  }
}
```

### Message Processing Flow

```typescript
/**
 * Process AI message with embedded commands
 */
async function processAIMessage(
  messageEntity: ChatMessageEntity,
  aiUserId: UUID,
  roomId: UUID
): Promise<void> {

  const parser = new AICommandParser();
  const messageText = messageEntity.content.text;

  // 1. Detect commands in message
  const commands = parser.detectCommands(messageText);

  if (commands.length === 0) {
    // No commands, just post message normally
    await postMessage(messageEntity);
    return;
  }

  // 2. Execute each command
  const results: CommandResult[] = [];
  for (const cmd of commands) {
    try {
      const result = await parser.executeCommand(cmd, aiUserId, roomId);
      results.push({
        command: cmd.commandPath,
        success: true,
        data: result
      });
    } catch (error) {
      results.push({
        command: cmd.commandPath,
        success: false,
        error: error.message
      });
    }
  }

  // 3. Attach results to message
  messageEntity.content.attachments = results.map(r => ({
    type: 'command-result',
    command: r.command,
    success: r.success,
    data: r.data,
    error: r.error
  }));

  // 4. Post message with attachments
  await postMessage(messageEntity);

  // 5. AI can now generate follow-up message referencing results
  // (This happens in next message generation cycle)
}
```

---

## Phase 2: Structured Tool-Calling (Future)

**When using better models (Claude, GPT-4, etc.):**

### AI Response Format

```typescript
interface AIResponse {
  // Natural language response
  message: string;

  // Structured tool calls
  toolCalls?: Array<{
    id: string;
    type: 'command';
    command: string;
    params: Record<string, any>;
  }>;
}
```

### Example

```json
{
  "message": "Let me check the logs for you.",
  "toolCalls": [
    {
      "id": "call_1",
      "type": "command",
      "command": "debug/logs",
      "params": {
        "tailLines": 20,
        "includeErrorsOnly": true
      }
    }
  ]
}
```

**Benefits over keywords:**
- ✅ Structured data (no parsing errors)
- ✅ Type-safe parameters
- ✅ Multiple commands in one response
- ✅ Cleaner UI (no command syntax in message)

**Drawback:**
- ❌ Requires better models (GPT-4, Claude Sonnet)
- ❌ Local models struggle with JSON formatting

---

## Security Considerations

### Command Whitelist

**Not all commands should be available to AIs:**

```typescript
const AI_ALLOWED_COMMANDS = [
  // Debug commands (read-only)
  'debug/logs',
  'debug/widget-state',
  'debug/html-inspector',

  // Data commands (read-only)
  'data/list',
  'data/read',

  // State commands (read-only)
  'state/get',

  // Screenshot (read-only observation)
  'screenshot'
];

const AI_FORBIDDEN_COMMANDS = [
  // Data modification
  'data/create',   // AIs shouldn't create arbitrary data
  'data/update',   // AIs shouldn't modify data directly
  'data/delete',   // AIs shouldn't delete data

  // System operations
  'session/destroy',  // AIs shouldn't kill sessions
  'process-registry', // AIs shouldn't manage processes

  // Potentially dangerous
  'exec',           // No arbitrary code execution
  'file/save'       // No arbitrary file writes
];
```

### Permission Model

```typescript
interface AICommandPermissions {
  userId: UUID;
  allowedCommands: string[];
  maxCommandsPerMinute: number;
  requireHumanApproval: boolean;
}

async function checkAICommandPermission(
  aiUserId: UUID,
  command: string
): Promise<boolean> {

  const permissions = await getAIPermissions(aiUserId);

  // Check whitelist
  if (!permissions.allowedCommands.includes(command)) {
    console.warn(`❌ AI ${aiUserId} attempted forbidden command: ${command}`);
    return false;
  }

  // Check rate limit
  const recentCommands = await getRecentCommandCount(aiUserId, 60); // Last minute
  if (recentCommands >= permissions.maxCommandsPerMinute) {
    console.warn(`⏸️  AI ${aiUserId} rate limited on commands`);
    return false;
  }

  // Check if human approval required
  if (permissions.requireHumanApproval) {
    return await requestHumanApproval(aiUserId, command);
  }

  return true;
}
```

---

## UI Considerations

### Displaying Command Results

**Option 1: Inline attachments**
```
┌─────────────────────────────────────────┐
│ Helper AI                  10:23 PM     │
│ Let me check the logs.                  │
│                                          │
│ 📎 Command: debug/logs                  │
│    [Expand to see 20 lines]             │
│                                          │
│ I see there's an error on line 174      │
│ of PersonaUser.ts...                    │
└─────────────────────────────────────────┘
```

**Option 2: Separate command channel**
```
#general (chat)              #commands (system)
┌────────────────────┐      ┌────────────────────┐
│ Joel: Show logs    │      │ Helper AI executed:│
│                    │      │ /jtag debug/logs   │
│ Helper AI: Let me  │      │                    │
│ check...           │      │ ✅ Success (347ms) │
│                    │      │ [View Results]     │
│ Helper AI: I see   │      └────────────────────┘
│ error on line 174  │
└────────────────────┘
```

**Option 3: Ephemeral indicators**
```
Helper AI is typing...
Helper AI is running command: /jtag debug/logs ⏳
Helper AI finished command ✅
Helper AI: [Message with results]
```

---

## Training Data Collection

### Storing Command Patterns

**RoomCoordinator and PersonaUsers learn which commands work:**

```typescript
interface CommandUsageLog {
  aiUserId: UUID;
  command: string;
  params: Record<string, any>;
  success: boolean;
  executionTime: number;
  context: {
    triggeringMessage: string;
    roomId: UUID;
    timestamp: Date;
  };
  humanFeedback?: 'helpful' | 'not-helpful' | 'wrong-command';
}

// Store in AI's own database for training
await this.storeCommandUsage({
  command: 'debug/logs',
  success: true,
  executionTime: 347,
  context: {
    triggeringMessage: 'Show me the logs',
    roomId: roomId,
    timestamp: new Date()
  },
  humanFeedback: 'helpful'  // User clicked thumbs up
});
```

### Learning Command Patterns

**After collecting usage data, train on patterns:**

```
Input: "Show me the logs"
→ /jtag debug/logs --tailLines=20

Input: "Take a screenshot of the chat"
→ /jtag screenshot --querySelector="chat-widget"

Input: "List all users"
→ /jtag data/list --collection=users

Input: "What's the current theme?"
→ /jtag state/get --key="theme"
```

**LoRA training improves command generation:**
- Base model: 60% correct command syntax
- After 500 examples: 85% correct
- After 2000 examples: 95% correct

---

## Implementation Phases

### Phase 1: Keywords + Whitelist ✅ (NEXT)

**Goal:** AIs can run safe, read-only commands via keywords

1. ⏭️ Define AI_ALLOWED_COMMANDS whitelist
2. ⏭️ Implement AICommandParser (regex detection)
3. ⏭️ Add permission checking
4. ⏭️ Execute commands and attach results
5. ⏭️ Update AI prompts with command syntax
6. ⏭️ Test with Ollama models

**Success criteria:**
- AIs can generate `/jtag` commands
- Commands are parsed and executed
- Results appear as attachments
- Only whitelisted commands work

---

### Phase 2: Structured Tool-Calling (FUTURE)

**Goal:** Better models use structured format

1. ⏭️ Design tool-calling format
2. ⏭️ Update AI daemon adapters (OpenAI, Anthropic support it)
3. ⏭️ Parse structured responses
4. ⏭️ Fallback to keywords for local models
5. ⏭️ Test with Claude/GPT-4

---

### Phase 3: Learning Command Patterns (FUTURE)

**Goal:** Train models to generate correct commands

1. ⏭️ Log all command usage
2. ⏭️ Collect human feedback
3. ⏭️ Build training dataset
4. ⏭️ Fine-tune LoRA adapters
5. ⏭️ Improve accuracy over time

---

## Related Documents

- `AI_COORDINATION_ARCHITECTURE.md` - Overall AI coordination
- `PERSONA_IMPLEMENTATION_MASTER_LIST.md` - Component checklist
- `DUMB_SENTINELS.md` - When heuristics beat AI

---

## Next Steps

1. **This week:** Implement keyword-based commands (Phase 1)
2. **Test:** AIs running debug/logs, screenshot, data/list
3. **Iterate:** Improve prompts, add more allowed commands
4. **Document:** Update this doc with learnings

**Let's give AIs the power to DO things, not just talk! 🛠️**
