# Ai Local Inference Start Command

Ensure Continuum's local inference HTTP server is running and return its URL. Idempotent — if already running, returns the existing URL without restarting. External agents (Claude Code via ANTHROPIC_BASE_URL, future Codex via OPENAI_BASE_URL) should call this once at startup, then use the returned URL. First-class surface for the AGENT-BACKBONE integration story (PR #976 §1-§4); previously only reachable as the Sentinel-internal sentinel/local-inference-start IPC command.

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Testing](#testing)
  - [Unit Tests](#unit-tests)
  - [Integration Tests](#integration-tests)
- [Getting Help](#getting-help)
- [Access Level](#access-level)
- [Implementation Notes](#implementation-notes)

## Usage

### CLI Usage

From the command line using the jtag CLI:

```bash
./jtag ai/local-inference/start 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/local-inference/start', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `AiLocalInferenceStartResult` with:

Returns CommandResult with:
- **url**: `string` - Base URL where the local inference server is accepting requests (e.g., http://127.0.0.1:8421)
- **port**: `number` - TCP port the server is bound to
- **protocol**: `string` - Wire protocol the server speaks. Currently always 'anthropic' (Messages API).
- **alreadyRunning**: `boolean` - True if the server was already up before this call (no spawn happened); false if this call started it

## Examples

### Start local inference (idempotent)

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/local-inference/start
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/local-inference/start'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/local-inference/start
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/local-inference/start'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Local Inference Start/test/unit/AiLocalInferenceStartCommand.test.ts
```

**What's tested:**
- Command structure and parameter validation
- Mock command execution patterns
- Required parameter validation (throws ValidationError)
- Optional parameter handling (sensible defaults)
- Performance requirements
- Assertion utility helpers

**TDD Workflow:**
1. Write/modify unit test first (test-driven development)
2. Run test, see it fail
3. Implement feature
4. Run test, see it pass
5. Refactor if needed

### Integration Tests

Test command with real client connections and system integration:

```bash
# Prerequisites: Server must be running
npm start  # Wait 90+ seconds for deployment

# Run integration tests
npx tsx commands/Ai Local Inference Start/test/integration/AiLocalInferenceStartIntegration.test.ts
```

**What's tested:**
- Client connection to live system
- Real command execution via WebSocket
- ValidationError handling for missing params
- Optional parameter defaults
- Performance under load
- Various parameter combinations

**Best Practice:**
Run unit tests frequently during development (fast feedback). Run integration tests before committing (verify system integration).

## Access Level

**ai-safe** - Safe for AI personas to call autonomously

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/AiLocalInferenceStartTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiLocalInferenceStartBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiLocalInferenceStartServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiLocalInferenceStartCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiLocalInferenceStartIntegration.test.ts`
