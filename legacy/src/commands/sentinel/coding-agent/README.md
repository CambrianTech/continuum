# Sentinel Coding Agent

Execute an external coding agent (Claude Code) for complex multi-file coding tasks. The agent reads, writes, tests, and iterates autonomously within a managed workspace.

## Usage

### CLI Usage

```bash
./jtag sentinel/coding-agent --prompt="Add error handling to the auth module"
./jtag sentinel/coding-agent --prompt="Fix the failing test" --cwd="/path/to/project" --permissionMode="bypassPermissions"
```

### Tool Usage

As a tool call from AI personas:
```
sentinel/coding-agent(prompt="Implement fibonacci in math.ts with tests", maxTurns=25, maxBudgetUsd=1.0)
```

## Parameters

- **prompt** (required): `string` - Task description for the coding agent
- **provider** (optional): `string` - Agent provider: "claude-code" (default). Future: "codex", "aider"
- **cwd** (optional): `string` - Working directory for the agent
- **systemPrompt** (optional): `string` - System prompt override
- **model** (optional): `string` - Model override (e.g., "sonnet", "opus")
- **allowedTools** (optional): `array` - Allowed tools (provider-specific names)
- **maxTurns** (optional): `number` - Maximum conversation turns (default: 25)
- **maxBudgetUsd** (optional): `number` - Maximum budget in USD (default: 5.0)
- **permissionMode** (optional): `string` - "default", "acceptEdits", or "bypassPermissions"
- **resumeSessionId** (optional): `string` - Resume a prior agent session
- **sentinelHandle** (optional): `string` - Sentinel handle for progress events
- **captureTraining** (optional): `boolean` - Capture interactions for LoRA training (default: true if personaId set)
- **personaId** (optional): `string` - Persona UUID for workspace resolution and identity
- **repoPath** (optional): `string` - Path to external git repo (triggers project worktree workspace)
- **taskSlug** (optional): `string` - Branch slug for workspace (default: "work")
- **personaName** (optional): `string` - Display name for git identity

## Result

- **success**: `boolean` - Whether the agent completed successfully
- **text**: `string` - Agent's final text output
- **agentSessionId**: `string` - Session ID for resume capability
- **toolCalls**: `array` - All tool calls made during the session
- **interactions**: `array` - Full interaction history (for training)
- **totalCostUsd**: `number` - Total cost in USD
- **numTurns**: `number` - Number of conversation turns
- **durationMs**: `number` - Total execution time in milliseconds
- **model**: `string` - Model used
- **workspaceDir**: `string` - Resolved workspace directory
- **branch**: `string` - Git branch created for the workspace

## Access Level

ai-safe
