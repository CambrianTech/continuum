# MCP Tool Calling Best Practices

**Date**: 2025-10-23
**Purpose**: Document proven patterns for getting LLMs to successfully use MCP tools

## The Challenge

Getting LLMs to reliably call tools requires:
1. **Clear formatting** - LLMs must output exact JSON schema
2. **Contextual awareness** - They need to know WHEN to use tools
3. **Success patterns** - Learn from what works (Claude Desktop, OpenAI function calling, etc.)
4. **No fine-tuning** - Must work with base models via prompting alone

---

## Industry Proven Patterns

### 1. OpenAI Function Calling Format

**What works:**
- Explicit `tools` array in API request
- Structured JSON schema with descriptions
- `tool_choice` parameter ('auto', 'required', 'none')
- Separate `tool_calls` in response (not mixed with text)

**Example:**
```json
{
  "model": "gpt-4",
  "messages": [...],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "data_read",
        "description": "Read an entity from the database by ID",
        "parameters": {
          "type": "object",
          "properties": {
            "collection": {
              "type": "string",
              "description": "Collection name (users, rooms, chat_messages)"
            },
            "id": {
              "type": "string",
              "description": "Entity UUID to read"
            }
          },
          "required": ["collection", "id"]
        }
      }
    }
  ]
}
```

**Response:**
```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [{
        "id": "call_abc123",
        "type": "function",
        "function": {
          "name": "data_read",
          "arguments": "{\"collection\":\"rooms\",\"id\":\"room-123\"}"
        }
      }]
    }
  }]
}
```

### 2. Anthropic Claude MCP Format

**What works:**
- Tools defined in `tools` parameter
- Clear descriptions with examples
- Structured thinking before tool use
- Tool results fed back into conversation

**Example:**
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [...],
  "tools": [
    {
      "name": "data_read",
      "description": "Read an entity from the database. Use this when you need to inspect a specific room, user, or message by its ID.",
      "input_schema": {
        "type": "object",
        "properties": {
          "collection": {
            "type": "string",
            "enum": ["users", "rooms", "chat_messages"],
            "description": "The collection to read from"
          },
          "id": {
            "type": "string",
            "description": "UUID of the entity to read"
          }
        },
        "required": ["collection", "id"]
      }
    }
  ]
}
```

### 3. MCP Protocol Standard

**What works:**
- Separate discovery phase (`tools/list`)
- Execution phase (`tools/call`)
- JSON-RPC 2.0 format
- Clear error handling

**Discovery:**
```json
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "data_read",
        "description": "Read entity from database",
        "inputSchema": {
          "type": "object",
          "properties": {...},
          "required": [...]
        }
      }
    ]
  }
}
```

---

## Our Implementation Strategy

### Phase 1: Provider Adapter Support (CURRENT - Phase 3)

**Goal**: Add `tools` parameter to AIProviderAdapter interface

```typescript
// daemons/ai-provider-daemon/shared/AIProviderTypes.ts
export interface TextGenerationRequest {
  messages: ChatMessage[];
  systemPrompt?: string;
  model?: string;
  temperature?: number;
  maxTokens?: number;

  // Phase 3: Tool calling support
  tools?: MCPTool[];
  toolChoice?: 'auto' | 'required' | 'none';
}

export interface TextGenerationResponse {
  text?: string;
  finishReason: 'stop' | 'length' | 'tool_calls' | 'error';

  // Phase 3: Tool call results
  toolCalls?: ToolCall[];

  model: string;
  provider: string;
  usage: UsageMetrics;
}

export interface MCPTool {
  name: string;
  description: string;
  inputSchema: {
    type: 'object';
    properties: Record<string, JSONSchemaProperty>;
    required?: string[];
  };
}

export interface ToolCall {
  id: string;
  type: 'function';
  function: {
    name: string;
    arguments: string; // JSON string
  };
}
```

### Phase 2: Recipe-Defined Tool Lists

**Goal**: Each recipe defines which JTAG commands are available as MCP tools

```json
{
  "uniqueId": "general-chat",
  "strategy": {
    "conversationPattern": "collaborative",
    "mcpTools": {
      "enabled": true,
      "whitelist": [
        "data/read",
        "data/list",
        "data/query",
        "file/load",
        "debug/logs"
      ],
      "descriptions": {
        "data/read": "Read a specific entity by ID. Use when you need to inspect a room, user, or message.",
        "data/list": "List entities from a collection. Use to discover available rooms or users.",
        "file/load": "Read file contents. Use when discussing code or documentation."
      }
    }
  }
}
```

### Phase 3: ThoughtStream Integration

**Goal**: Include available tools in RAG context

```typescript
// system/rag/shared/RAGTypes.ts
export interface RAGContext {
  domain: RAGDomain;
  contextId: UUID;
  personaId: UUID;
  identity: PersonaIdentity;
  conversationHistory: LLMMessage[];

  // Phase 3: Available tools from recipe
  availableTools?: MCPTool[];
  toolPolicy?: {
    maxToolsPerResponse?: number;
    maxToolsPerMinute?: number;
    requiresApproval?: boolean;
  };

  // ... rest of context
}
```

### Phase 4: Prompt Engineering for Tool Use

**Critical patterns from production systems:**

#### Pattern 1: System Prompt with Tool Instructions
```typescript
const systemPrompt = `You are ${personaName} in a group chat.

AVAILABLE TOOLS:
You have access to these commands to gather more information:

${availableTools.map(tool => `
- ${tool.name}: ${tool.description}
  Parameters: ${JSON.stringify(tool.inputSchema.properties)}
`).join('\n')}

HOW TO USE TOOLS:
1. When you need information you don't have, use a tool BEFORE responding
2. Call tools by including tool_use blocks in your response
3. Wait for tool results, then incorporate them into your answer
4. Don't make up information - use tools to verify facts

WHEN TO USE TOOLS:
- User asks about specific entities (rooms, users, messages) → use data/read
- User asks "what rooms exist?" → use data/list
- User mentions code/files → use file/load
- Debugging issues → use debug/logs

WHEN NOT TO USE TOOLS:
- Casual conversation that doesn't need data
- Questions you can answer from current context
- After you already have the information
`;
```

#### Pattern 2: Few-Shot Examples in RAG Context

```typescript
// Include example tool uses in conversation history
const exampleToolUse = [
  {
    role: 'user',
    content: 'What rooms are available?'
  },
  {
    role: 'assistant',
    content: null,
    tool_calls: [{
      id: 'call_example',
      type: 'function',
      function: {
        name: 'data/list',
        arguments: '{"collection":"rooms","limit":10}'
      }
    }]
  },
  {
    role: 'tool',
    tool_call_id: 'call_example',
    content: '{"items":[{"name":"general"},{"name":"academy"}]}'
  },
  {
    role: 'assistant',
    content: 'There are 2 rooms: general and academy.'
  }
];
```

#### Pattern 3: Structured Reasoning (Chain-of-Thought)

```typescript
const systemPrompt = `...

REASONING PROCESS:
Before responding, think through:
1. What information do I need to answer this?
2. Do I have this information in current context?
3. If not, which tool would give me this information?
4. Call the tool, wait for results
5. Now I can give an accurate answer

Example internal reasoning:
"User asks about learning mode fields. I don't see the RoomEntity definition in context. I should use file/load to read system/data/entities/RoomEntity.ts, then I can answer accurately about which fields exist."
`;
```

---

## Common Pitfalls to Avoid

### ❌ Pitfall 1: Tool Calls Mixed with Text
**Bad:**
```json
{
  "content": "Let me check that... [TOOL:data/read:rooms:123] According to the data..."
}
```

**Good:**
```json
{
  "content": null,
  "tool_calls": [{...}]
}
```

### ❌ Pitfall 2: Hallucinating Tool Parameters
**Problem**: LLM invents parameter values it doesn't know

**Solution**: Include parameter constraints in description
```json
{
  "name": "data_read",
  "description": "Read entity by ID. IMPORTANT: Use actual UUIDs from conversation, don't invent IDs.",
  "inputSchema": {
    "properties": {
      "id": {
        "type": "string",
        "description": "UUID from a previous message or tool result. If you don't have a UUID, use data/list first."
      }
    }
  }
}
```

### ❌ Pitfall 3: Not Waiting for Tool Results
**Problem**: LLM generates response before tool completes

**Solution**: Multi-turn conversation with explicit waiting
1. Turn 1: User asks question
2. Turn 2: Assistant calls tool (no text content)
3. Turn 3: System returns tool result
4. Turn 4: Assistant generates answer using result

### ❌ Pitfall 4: Over-Using Tools
**Problem**: LLM calls tools for information it already has

**Solution**: Clear guidance in prompt
```
IMPORTANT: Only use tools when you NEED information not in current context.
- ✓ User asks "what's in RoomEntity?" → use file/load
- ✗ User says "hi" → just respond, no tools needed
```

---

## Testing Strategy

### Unit Tests: Tool Call Parsing
```typescript
describe('AIProviderAdapter - Tool Calling', () => {
  it('should parse OpenAI-style tool calls', async () => {
    const response = await adapter.generateText({
      messages: [...],
      tools: [dataSendTool]
    });

    expect(response.toolCalls).toBeDefined();
    expect(response.toolCalls[0].function.name).toBe('data_read');
    expect(JSON.parse(response.toolCalls[0].function.arguments)).toEqual({
      collection: 'rooms',
      id: 'room-123'
    });
  });
});
```

### Integration Tests: End-to-End Tool Use
```typescript
describe('PersonaUser - Tool Calling', () => {
  it('should use tools to gather context before responding', async () => {
    // 1. Send message that requires data lookup
    await sendMessage('What learning mode is configured for persona-123 in the general room?');

    // 2. Verify AI calls tool
    const toolCalls = await waitForToolCalls();
    expect(toolCalls).toContainEqual({
      name: 'data/read',
      arguments: { collection: 'rooms', id: 'general' }
    });

    // 3. Verify AI uses result in response
    const response = await waitForResponse();
    expect(response).toContain('fine-tuning mode');
    expect(response).not.toContain('I don\'t know'); // Should not guess
  });
});
```

### Human Evaluation: Accuracy Check
```bash
# Test against known ground truth
./jtag test/tool-calling/accuracy --scenarios=10

# Scenarios:
# 1. User asks about entity that exists → AI should use data/read
# 2. User asks about entity that doesn't exist → AI should handle gracefully
# 3. User asks casual question → AI should NOT use tools unnecessarily
# 4. User asks about code → AI should use file/load
# 5. User asks follow-up → AI should reuse previous tool results
```

---

## Metrics to Track

### Tool Call Success Rate
```typescript
interface ToolCallMetrics {
  totalToolCalls: number;
  successfulCalls: number;
  failedCalls: number;
  hallucinated: number; // Called tool with invalid parameters
  unnecessary: number;  // Called tool when answer was in context
  accuracy: number;     // Used correct tool for task
}
```

### Response Quality
- **Hallucination rate** before vs after tool calling
- **Answer accuracy** on questions requiring data lookup
- **Response latency** (tool calls add time)
- **Token efficiency** (fewer tokens if tools replace long context)

---

## Priority MCP Tools (Phase 3 Focus)

### Goal 1: Code Sharing & Discussion

**Use Case**: Human discusses code with AI, AI can read relevant files

**Priority Tools:**
1. **`file/load`** - Read source files
   ```json
   {
     "name": "file_load",
     "description": "Read contents of a source file. Use when discussing code, debugging, or understanding architecture.",
     "inputSchema": {
       "type": "object",
       "properties": {
         "path": {
           "type": "string",
           "description": "Relative path from project root (e.g., 'system/user/server/PersonaUser.ts')"
         }
       },
       "required": ["path"]
     }
   }
   ```

2. **`data/read`** - Inspect entity definitions
   ```json
   {
     "name": "data_read",
     "description": "Read entity to understand data structure. Use when discussing database schema or entity relationships.",
     "inputSchema": {
       "type": "object",
       "properties": {
         "collection": {"type": "string", "enum": ["users", "rooms", "chat_messages"]},
         "id": {"type": "string", "description": "Entity UUID"}
       },
       "required": ["collection", "id"]
     }
   }
   ```

3. **`data/list`** - Discover available entities
   ```json
   {
     "name": "data_list",
     "description": "List entities to discover what exists. Use before data/read when you don't have a UUID.",
     "inputSchema": {
       "type": "object",
       "properties": {
         "collection": {"type": "string"},
         "limit": {"type": "number", "default": 10}
       },
       "required": ["collection"]
     }
   }
   ```

### Goal 2: Code Editing (Later Phase)

**Use Case**: AI proposes code changes, verifies they compile

**Priority Tools:**
4. **`file/save`** - Write code changes
   ```json
   {
     "name": "file_save",
     "description": "Save changes to a file. ALWAYS read the file with file/load first to see current contents.",
     "inputSchema": {
       "type": "object",
       "properties": {
         "path": {"type": "string"},
         "content": {"type": "string", "description": "Complete file contents (not a diff)"}
       },
       "required": ["path", "content"]
     }
   }
   ```

5. **`compile-typescript`** - Verify changes compile
   ```json
   {
     "name": "compile_typescript",
     "description": "Run TypeScript compiler to check for errors. Use after making code changes.",
     "inputSchema": {
       "type": "object",
       "properties": {}
     }
   }
   ```

6. **`test/run/suite`** - Run tests to verify correctness
   ```json
   {
     "name": "test_run_suite",
     "description": "Run test suite. Use after code changes to ensure nothing broke.",
     "inputSchema": {
       "type": "object",
       "properties": {
         "pattern": {"type": "string", "description": "Test file pattern (optional)"}
       }
     }
   }
   ```

### Example Code Discussion Flow

```
Human: "Can you explain how PersonaUser handles learning mode?"

AI (internal): User asks about code. I should read the file first.

AI tool_call: file_load("system/user/server/PersonaUser.ts")

System: [Returns file contents]

AI response: "PersonaUser loads learning mode in the loadLearningConfig method (line 414).
It reads the room membership to get learningMode, genomeId, and participantRole fields."
```

### Example Code Editing Flow

```
Human: "The minContextMessages is too low - increase it to 15"

AI tool_call: file_load("system/user/server/PersonaUser.ts")

System: [Returns file contents showing line 1731: `const minContextMessages = 3`]

AI tool_call: file_save({
  path: "system/user/server/PersonaUser.ts",
  content: "[Modified file with minContextMessages = 15]"
})

AI tool_call: compile_typescript()

System: [Returns compilation success]

AI response: "I've increased minContextMessages from 3 to 15 in PersonaUser.ts line 1731.
TypeScript compilation passed - the change is ready."
```

---

## Next Steps (Phase 3 Implementation)

**Week 1: Foundation**
1. **Extend AIProviderTypes** with `tools` and `toolCalls`
2. **Update provider adapters** (OpenAI, Anthropic, Ollama)
3. **Create JTAG→MCP tool converter** (command metadata → MCP tool schema)

**Week 2: Integration**
4. **Add tool filtering** based on recipe permissions
5. **Update PersonaUser** to handle tool call responses
6. **Create prompt templates** with tool instructions

**Week 3: Code Sharing**
7. **Prioritize `file/load`, `data/read`, `data/list`** tools
8. **Test code discussion scenarios** with real conversations
9. **Measure hallucination rate** before vs after tool access

**Week 4: Code Editing (Gated)**
10. **Add `file/save`, `compile-typescript`, `test/run/suite`**
11. **Create approval workflow** for code changes
12. **Test with sandbox repos** before enabling on main codebase

---

## References

- [Anthropic MCP Documentation](https://modelcontextprotocol.io/)
- [OpenAI Function Calling Guide](https://platform.openai.com/docs/guides/function-calling)
- [Claude Desktop Tool Use](https://www.anthropic.com/news/model-context-protocol)
- Our implementation: `system/conversation/AI-COMMAND-EXECUTION-ARCHITECTURE.md`
