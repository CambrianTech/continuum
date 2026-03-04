# AI Tool Calling Troubleshooting Guide

**Status**: Active investigation as of 2025-11-18
**Audience**: Local Ollama AI personas experiencing tool execution failures

---

## Problem Summary

Local Ollama AIs (llama3.2:1b, llama3.2:3b, phi3:mini) are receiving tool documentation in their system prompts but are **not generating XML tool calls**. Instead, they:

1. Discuss tools theoretically without executing them
2. Get stuck in repetitive loops (e.g., Fireworks AI repeating the same intent 6+ times)
3. Attempt to use tools but fail with parameter errors

**Evidence from chat logs**:
- ✅ **External API AIs work** - Claude Assistant successfully executed `tree` command and received full results
- ❌ **Local Ollama AIs fail** - DeepSeek, Groq Lightning, Helper AI, Teacher AI only discuss tools without calling them

---

## Root Cause Analysis

### What's Working

1. **ToolRegistry is properly initialized** - Called at UserDaemon startup (daemons/user-daemon/server/UserDaemonServer.ts:86)
2. **Tool documentation is generated** - `ToolRegistry.generateToolDocumentation()` creates XML format instructions
3. **Tool docs are injected into prompts** - ChatRAGBuilder.ts:239 adds tool docs to every AI's system prompt
4. **XML parsing exists** - PersonaToolExecutor.ts:495-531 correctly parses `<tool_use>` blocks
5. **Tool execution infrastructure works** - External API AIs successfully execute tools

### What's Broken

**Local Ollama models are not following the XML tool calling instructions** in their system prompts.

**Specific failure patterns observed:**

1. **Fireworks AI** - Stuck in loop saying "I'll use the ai/validate-response tool" 6+ times without ever generating XML
2. **DeepSeek Assistant** - Trying to query `chat_messages` collection but not formatting requests as tools
3. **Groq Lightning** - Suggesting tool use but not generating `<tool_use>` XML blocks
4. **Helper AI, Teacher AI, Local Assistant** - Discussing validation scripts instead of executing `tree` or `data/list`

---

## Issue #1: Local Models Don't Generate XML Tool Calls

### Problem

Small local models (1B-3B parameters) are not sophisticated enough to consistently follow XML tool calling format, even with explicit instructions.

### Current System Prompt Format

From `ToolRegistry.generateToolDocumentation()` (system/tools/server/ToolRegistry.ts:277-296):

```
AVAILABLE TOOLS:
You have access to tools for reading code, querying data, and system operations. To use a tool, include a tool invocation in your response using this exact XML format:

<tool_use>
<tool_name>command/name</tool_name>
<parameters>
<paramName>value</paramName>
</parameters>
</tool_use>

Available tools:
[List of 104 tools with parameters...]

Tool execution flow:
1. Include <tool_use> blocks in your response
2. System executes tools and provides results
3. You receive results and provide final analysis

NOTE: Tool calls are removed from visible response. Only your text is shown to users.
```

### Why This Fails for Small Models

1. **Format complexity** - XML with nested tags is difficult for small models to generate reliably
2. **Instruction length** - 104 tools in prompt overwhelms context window
3. **No examples** - Abstract instructions without concrete demonstrations
4. **No reinforcement** - Models never see successful tool calling examples in conversation history

### Solution #1: Enhanced System Prompt

**Location**: `system/tools/server/ToolRegistry.ts` - `generateToolDocumentation()` method

**Changes needed**:

```typescript
generateToolDocumentation(): string {
  if (!this.initialized) {
    throw new Error('ToolRegistry not initialized - call initialize() first');
  }

  const tools = this.getAllTools();

  if (tools.length === 0) {
    return 'No tools available.';
  }

  // NEW: Add concrete examples at the top
  const examplesSection = `
TOOL CALLING EXAMPLES:

Example 1: List all commands
<tool_use>
<tool_name>list</tool_name>
<parameters>
</parameters>
</tool_use>

Example 2: Get command tree structure
<tool_use>
<tool_name>tree</tool_name>
<parameters>
</parameters>
</tool_use>

Example 3: Query chat messages
<tool_use>
<tool_name>data/list</tool_name>
<parameters>
<collection>chat_messages</collection>
<limit>20</limit>
</parameters>
</tool_use>

Example 4: Read code file
<tool_use>
<tool_name>code/read</tool_name>
<parameters>
<paths>/path/to/file.ts</paths>
</parameters>
</tool_use>

CRITICAL: You MUST use this EXACT XML format. Do NOT just talk about tools - USE them!
`;

  // NEW: Group tools by category to reduce cognitive load
  const categorizedTools = this.groupToolsByCategory(tools);

  // NEW: Show only top 20 most useful tools, with "use 'list' to see all" note
  const topTools = this.getTopTools(tools, 20);

  const toolDescriptions = topTools.map((tool, index) => {
    const requiredParams = Object.entries(tool.parameters)
      .filter(([_, def]) => def.required)
      .map(([name, def]) => `<${name}>${def.description || name}</${name}>`)
      .join(' ');

    const optionalParams = Object.entries(tool.parameters)
      .filter(([_, def]) => !def.required)
      .map(([name, def]) => `<${name}>${def.description || name}</${name}>`)
      .join(' ');

    let desc = `${index + 1}. ${tool.name} - ${tool.description}`;

    if (requiredParams) {
      desc += `\n   Required: ${requiredParams}`;
    }

    if (optionalParams) {
      desc += `\n   Optional: ${optionalParams}`;
    }

    return desc;
  }).join('\n\n');

  return `AVAILABLE TOOLS:
You have access to ${tools.length} tools. Here are the 20 most useful (use 'list' tool to see all):

${examplesSection}

${toolDescriptions}

Tool execution flow:
1. When you want to READ DATA, QUERY INFORMATION, or EXECUTE COMMANDS, you MUST use tools
2. Generate a <tool_use> block with EXACT XML format shown in examples above
3. System will execute the tool and show you results
4. After seeing results, provide your analysis to the user

REMEMBER:
- DON'T just say "I'll use the X tool" - ACTUALLY USE IT by generating XML
- DON'T talk about what you would do - DO IT
- DON'T repeat the same intent - if you said you'll use a tool, generate the XML immediately`;
}

// NEW: Helper methods
private groupToolsByCategory(tools: ToolDefinition[]): Map<string, ToolDefinition[]> {
  const groups = new Map<string, ToolDefinition[]>();

  for (const tool of tools) {
    const category = tool.name.split('/')[0];
    if (!groups.has(category)) {
      groups.set(category, []);
    }
    groups.get(category)!.push(tool);
  }

  return groups;
}

private getTopTools(tools: ToolDefinition[], limit: number): ToolDefinition[] {
  // Prioritize most useful tools for AI workflows
  const priority = [
    'list', 'tree', 'data/list', 'data/read', 'code/read',
    'chat/export', 'ai/generate', 'ai/model/list',
    'screenshot', 'file/load', 'file/save'
  ];

  const prioritized = tools.filter(t => priority.includes(t.name));
  const remaining = tools.filter(t => !priority.includes(t.name));

  return [...prioritized, ...remaining].slice(0, limit);
}
```

**Key improvements**:
1. **Concrete examples** - 4 real-world tool calling examples at the top
2. **Reduced cognitive load** - Show only top 20 tools instead of all 104
3. **Explicit anti-patterns** - "DON'T just say you'll use it - USE IT"
4. **Repetition prevention** - Warns against loops like Fireworks AI's behavior

---

## Issue #2: Fireworks AI Stuck in Repetitive Loop

### Problem

Fireworks AI repeated "I'll use the ai/validate-response tool" **6 times** without ever generating XML:

```
#e70684 - "I'll use the ai/validate-response tool to check for conflicts..."
#561217 - "I'll use the ai/validate-response tool to check for conflicts..."
#95373f - "I'll use the ai/validate-response tool to check for conflicts..."
#2d5c9b - "I'll use the ai/validate-response tool to check for conflicts..."
#3a912e - "I'll use the ai/validate-response tool to check for conflicts..."
```

### Root Cause

The model is:
1. Understanding it should use a tool (intent recognition works)
2. Describing what it will do (natural language generation)
3. **Never transitioning** from intent to XML generation
4. Conversation history shows its own failed attempts, reinforcing the pattern

### Solution #2: Add Repetition Detection and Intervention

**Location**: `system/user/server/modules/PersonaResponseGenerator.ts`

**Add new method**:

```typescript
/**
 * Detect if AI is stuck in repetitive loop without executing tools
 *
 * Pattern: AI says "I'll use X tool" multiple times without generating XML
 */
private detectToolIntentLoop(
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
  currentResponse: string
): boolean {
  // Check if current response contains tool intent phrases
  const intentPhrases = [
    "I'll use the",
    "I will use the",
    "Let me use the",
    "I'll query",
    "I'll call"
  ];

  const hasIntent = intentPhrases.some(phrase =>
    currentResponse.toLowerCase().includes(phrase.toLowerCase())
  );

  if (!hasIntent) {
    return false;
  }

  // Check if response actually contains tool_use XML
  const hasToolUse = currentResponse.includes('<tool_use>');

  if (hasToolUse) {
    return false; // Not stuck - they're actually using tools
  }

  // Check how many recent messages from this AI contain same pattern
  const recentAssistantMessages = messages
    .filter(m => m.role === 'assistant')
    .slice(-5); // Last 5 assistant messages

  const intentCount = recentAssistantMessages.filter(m =>
    intentPhrases.some(phrase => m.content.toLowerCase().includes(phrase.toLowerCase())) &&
    !m.content.includes('<tool_use>')
  ).length;

  // If 3+ recent messages show intent without execution, they're stuck
  return intentCount >= 3;
}

/**
 * Generate intervention prompt to break repetition loop
 */
private generateInterventionPrompt(toolName: string): string {
  return `
SYSTEM INTERVENTION: You have stated your intent to use the "${toolName}" tool multiple times without actually using it.

To use a tool, you MUST generate XML in this EXACT format:

<tool_use>
<tool_name>${toolName}</tool_name>
<parameters>
<paramName>value</paramName>
</parameters>
</tool_use>

DO NOT say "I will use" - just generate the XML immediately.
If you cannot generate valid XML, explain why and ask for help.
`;
}
```

**Integrate into response generation** (in `generateAndPostResponse` method):

```typescript
// After LLM generates response, before posting
const isStuck = this.detectToolIntentLoop(messages, llmResponse);

if (isStuck) {
  console.warn(`⚠️ ${this.personaName}: Detected tool intent loop - injecting intervention`);

  // Extract tool name from intent
  const toolMatch = llmResponse.match(/(?:use the|call the|query)\s+([a-z0-9\/\-]+)\s+tool/i);
  const toolName = toolMatch ? toolMatch[1] : 'tool';

  // Inject intervention into message history
  messages.push({
    role: 'system',
    content: this.generateInterventionPrompt(toolName)
  });

  // Request new response with intervention
  llmResponse = await this.generateResponse(messages);
}
```

**Expected outcome**: After 3 failed attempts to execute a tool, system intervenes with explicit XML example, breaking the loop.

---

## Issue #3: DeepSeek Trying to Query Collections Manually

### Problem

DeepSeek Assistant keeps saying:
```
"I need to query the chat_messages collection to get the conversation history"
"Let me query the chat_messages collection directly"
```

But never generates valid `data/list` or `data/read` commands.

### Root Cause

The AI understands:
1. Data exists in collections (shows data model knowledge)
2. Collections can be queried (shows system understanding)
3. **Doesn't know how** to format the query as a tool call

### Solution #3: Add Task-Specific Tool Guidance

**Location**: `system/rag/builders/ChatRAGBuilder.ts` - `buildSystemPrompt()` method

**Add domain-specific hints based on conversation context**:

```typescript
private async buildSystemPrompt(user: UserEntity, roomId: UUID): Promise<string> {
  // ... existing code ...

  // NEW: Add context-aware tool hints
  const toolHints = this.generateContextualToolHints(roomId);

  return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat.${othersContext}

[... existing sections ...]

${toolHints}

${toolRegistry.generateToolDocumentation()}`;
}

/**
 * Generate contextual hints for common tool use patterns
 */
private generateContextualToolHints(roomId: UUID): string {
  return `
COMMON TOOL PATTERNS:

When you need conversation history:
<tool_use>
<tool_name>chat/export</tool_name>
<parameters>
<room>general</room>
<limit>20</limit>
</parameters>
</tool_use>

When you need to query data:
<tool_use>
<tool_name>data/list</tool_name>
<parameters>
<collection>chat_messages</collection>
<filter>{"roomId":"${roomId}"}</filter>
<limit>50</limit>
</parameters>
</tool_use>

When you need to read code:
<tool_use>
<tool_name>code/read</tool_name>
<parameters>
<paths>/path/to/file.ts</paths>
</parameters>
</tool_use>

When you want to see all commands:
<tool_use>
<tool_name>tree</tool_name>
<parameters>
</parameters>
</tool_use>
`;
}
```

**Expected outcome**: When AI says "I need to query chat_messages", it sees the exact pattern above and copies it.

---

## Issue #4: Model Selection - Wrong Tools for the Job

### Problem

Current local models:
- `llama3.2:1b` - 1 billion parameters (too small for complex tool use)
- `llama3.2:3b` - 3 billion parameters (barely sufficient)
- `phi3:mini` - 3.8 billion parameters (inconsistent)

These models were optimized for **chat completion**, not **tool calling**.

### Solution #4: Use Tool-Calling Optimized Models

**Recommended models for Ollama**:

1. **llama3.1:8b-instruct-q4_K_M** (4.9GB)
   - Explicitly trained on tool calling
   - Best balance of size/capability
   - Recommended for all local PersonaUsers

2. **mistral:7b-instruct-v0.3-q4_K_M** (4.1GB)
   - Good tool calling support
   - Faster inference than llama3.1:8b
   - Alternative if memory constrained

3. **qwen2.5:7b-instruct-q4_K_M** (4.4GB)
   - Excellent tool calling performance
   - Strong reasoning capabilities
   - Best for complex multi-step tasks

**How to switch models**:

```bash
# Pull new model
ollama pull llama3.1:8b-instruct-q4_K_M

# Update PersonaUser config in database
./jtag user/update --userId="PERSONA_ID" \
  --modelConfig='{"provider":"ollama","modelId":"llama3.1:8b-instruct-q4_K_M"}'

# Restart system to reload configs
npm start
```

**Update all local personas at once**:

```bash
# Get all local PersonaUser IDs
PERSONA_IDS=$(./jtag data/list --collection=users \
  --filter='{"type":"persona","modelConfig.provider":"ollama"}' \
  --format=json | jq -r '.items[].id')

# Update each one
for ID in $PERSONA_IDS; do
  echo "Updating persona $ID..."
  ./jtag user/update --userId="$ID" \
    --modelConfig='{"provider":"ollama","modelId":"llama3.1:8b-instruct-q4_K_M"}'
done
```

---

## Issue #5: No Tool Calling Examples in Training Data

### Problem

Even with enhanced prompts, small models may never learn tool calling patterns because they've never been **fine-tuned** on tool calling examples.

### Solution #5: Fine-Tune Local Models on Tool Calling

**Strategy**: Create a dataset of tool calling examples and fine-tune a LoRA adapter for each PersonaUser's model.

**Dataset format** (`tool-calling-examples.jsonl`):

```jsonl
{"messages":[{"role":"system","content":"AVAILABLE TOOLS:\n<tool_use> XML format..."},{"role":"user","content":"Show me the command tree"},{"role":"assistant","content":"<tool_use>\n<tool_name>tree</tool_name>\n<parameters>\n</parameters>\n</tool_use>"}]}
{"messages":[{"role":"system","content":"AVAILABLE TOOLS:\n..."},{"role":"user","content":"What messages are in the general room?"},{"role":"assistant","content":"<tool_use>\n<tool_name>chat/export</tool_name>\n<parameters>\n<room>general</room>\n<limit>20</limit>\n</parameters>\n</tool_use>"}]}
{"messages":[{"role":"system","content":"AVAILABLE TOOLS:\n..."},{"role":"user","content":"List all available AI models"},{"role":"assistant","content":"<tool_use>\n<tool_name>ai/model/list</tool_name>\n<parameters>\n</parameters>\n</tool_use>"}]}
```

**Create training dataset**:

```bash
# Generate tool calling examples from successful Claude Assistant interactions
./jtag ai/dataset/create \
  --name="tool-calling-examples" \
  --sourceCollection="chat_messages" \
  --filter='{"senderName":"Claude Assistant","content":{"$regex":"<tool_use>"}}' \
  --format="chat-completion" \
  --outputPath="/tmp/tool-calling-training.jsonl"
```

**Fine-tune adapter** (using existing genome/fine-tuning system):

```bash
# Train LoRA adapter for tool calling
./jtag genome/fine-tune \
  --genomeId="PERSONA_GENOME_ID" \
  --layerName="tool-calling" \
  --trainingData="/tmp/tool-calling-training.jsonl" \
  --epochs=3 \
  --learningRate=0.0001
```

**This is Phase 7 of PersonaUser Convergence** - See `PERSONA-CONVERGENCE-ROADMAP.md` for full implementation plan.

---

## Issue #6: Claude Assistant's --output-format Suggestion

### Problem (Actually Not a Problem!)

Claude Assistant proposed using `--output-format` instead of `--format` to avoid conflicts. This is actually a **good idea** and should be implemented.

### Solution #6: Implement --output-format Global Flag

**Location**: Create new file `system/cli/shared/GlobalFlags.ts`

```typescript
/**
 * Global flags available to ALL commands
 *
 * These flags are automatically injected and handled by the CLI framework,
 * so individual commands don't need to implement them.
 */
export interface GlobalFlags {
  /**
   * Control output serialization format
   *
   * - json: Raw JSON for scripting/piping
   * - yaml: Human-readable structured data
   * - table: Formatted table for terminal (default for list commands)
   * - compact: Minified version of selected format
   */
  outputFormat?: 'json' | 'yaml' | 'table';

  /**
   * Minify output (removes whitespace, borders, formatting)
   */
  compact?: boolean;

  /**
   * Show command help documentation
   * Auto-generated from command metadata
   */
  help?: boolean;
}

/**
 * Get default output format for command category
 */
export function getDefaultOutputFormat(commandName: string): 'json' | 'yaml' | 'table' {
  // List commands default to table
  if (commandName.includes('/list') || commandName === 'list' || commandName === 'tree') {
    return 'table';
  }

  // Export commands default to JSON
  if (commandName.includes('/export')) {
    return 'json';
  }

  // Status/info commands default to YAML
  if (commandName.includes('/status') || commandName.includes('/get')) {
    return 'yaml';
  }

  // Default to YAML for readability
  return 'yaml';
}
```

**Convention**:
- `--format` = domain-specific (image format, report type, data schema)
- `--output-format` = CLI response serialization (JSON/YAML/table)
- Commands can use both if they have distinct purposes

---

## Testing the Fixes

### Test Plan

1. **Deploy enhanced system prompts**
   ```bash
   npm run build:ts
   npm start
   # Wait 90 seconds
   ./jtag ping
   ```

2. **Test with direct command in chat**
   ```bash
   ./jtag debug/chat-send --room="general" \
     --message="Use the tree tool to show me all commands. Remember to generate <tool_use> XML, don't just say you'll use it."
   ```

3. **Wait 10 seconds and check responses**
   ```bash
   sleep 10
   ./jtag collaboration/chat/export --room="general" --limit=10
   ```

4. **Look for**:
   - ✅ XML `<tool_use>` blocks in AI responses
   - ✅ Tool execution results in chat
   - ❌ Repetitive "I'll use X tool" without XML
   - ❌ Discussion about tools without execution

### Success Criteria

- **Before**: 0% of local Ollama AIs execute tools successfully
- **After**: 60%+ of local Ollama AIs execute tools on first attempt
- **With fine-tuning**: 90%+ execution rate

---

## Summary of Action Items

**Immediate (Can do now)**:
1. ✅ Enhance ToolRegistry.generateToolDocumentation() with examples
2. ✅ Add repetition detection to PersonaResponseGenerator
3. ✅ Add contextual tool hints to ChatRAGBuilder
4. ✅ Document --output-format convention

**Short-term (This week)**:
5. ⏳ Switch local PersonaUsers to llama3.1:8b-instruct models
6. ⏳ Implement GlobalFlags system with --output-format
7. ⏳ Test with enhanced prompts

**Long-term (Next sprint)**:
8. 📋 Create tool calling training dataset
9. 📋 Fine-tune LoRA adapters for each PersonaUser
10. 📋 Implement adaptive prompt selection based on model capability

---

## References

- **ToolRegistry**: `system/tools/server/ToolRegistry.ts`
- **ChatRAGBuilder**: `system/rag/builders/ChatRAGBuilder.ts`
- **PersonaToolExecutor**: `system/user/server/modules/PersonaToolExecutor.ts`
- **PersonaResponseGenerator**: `system/user/server/modules/PersonaResponseGenerator.ts`
- **PersonaUser Convergence**: `system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md`
- **Tool Calling Examples**: Chat messages from Claude Assistant in general room

---

**Last Updated**: 2025-11-18
**Status**: Ready for implementation
**Priority**: HIGH - Blocking AI team productivity
