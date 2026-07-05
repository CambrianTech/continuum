# ai/agent

Universal agentic loop command. Generates text via LLM, parses tool calls, executes tools, feeds results back, and re-generates until the model stops calling tools.

## Usage

```bash
# Simple prompt with specific tools
./jtag ai/agent --prompt="List all TypeScript files in commands/" --tools='["code/tree"]'

# With model selection
./jtag ai/agent --prompt="Search for TODO comments" --tools='["code/search"]' --model="claude-sonnet-4-5-20250929" --provider="anthropic"

# No tools (single-shot generation)
./jtag ai/agent --prompt="Explain the builder pattern" --tools='[]'

# All tools enabled (default)
./jtag ai/agent --prompt="Find and fix the bug in src/main.ts"

# With system prompt
./jtag ai/agent --prompt="Review this code" --systemPrompt="You are a senior code reviewer. Be concise."
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `prompt` | string | One of prompt/messages | Simple text prompt |
| `messages` | ChatMessage[] | One of prompt/messages | Full message array |
| `systemPrompt` | string | No | System prompt |
| `model` | string | No | Model ID (default: claude-sonnet-4-5-20250929) |
| `provider` | string | No | Provider (default: anthropic) |
| `temperature` | number | No | Sampling temperature (default: 0.7) |
| `maxTokens` | number | No | Max tokens (default: 4096) |
| `tools` | string[] | No | Tool subset. undefined=all, []=none |
| `maxIterations` | number | No | Override safety cap |
| `callerId` | string | No | Caller identity for tool attribution |
| `sentinelHandle` | string | No | Sentinel handle for log correlation |

## Result

```typescript
{
  success: boolean;
  text: string;           // Final LLM response
  toolCalls: [{           // All tool calls made
    toolName: string;
    params: Record<string, string>;
    success: boolean;
    content?: string;
    error?: string;
    durationMs: number;
  }];
  iterations: number;     // Tool loop iterations
  tokenUsage?: { input: number; output: number };
  model?: string;
  provider?: string;
  durationMs: number;
}
```

## Sentinel Pipeline Usage

```json
{
  "type": "llm",
  "prompt": "List all files and summarize the project structure",
  "agentMode": true,
  "tools": ["code/tree", "code/read"],
  "model": "claude-sonnet-4-5-20250929",
  "provider": "anthropic"
}
```

When `agentMode: true`, the Rust LLM step routes to this command via CommandExecutor IPC.

## Safety

- Tiered safety caps: 25 iterations (frontier), 10 (native tools), 5 (XML/local)
- Loop detection: identical tool calls within 60s are blocked
- Tool name/param correction: handles LLM confusion automatically
- Admin-gated: personas cannot invoke this command (prevents recursive loops)
