# Voice Start Command

Start voice chat session for real-time audio communication with AI

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
./jtag voice/start [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('voice/start', {
  // your parameters here
});
```

## Parameters

- **room** (optional): `string` - Room ID or name to join (defaults to 'general')
- **model** (optional): `string` - LLM model to use for responses (defaults to system default)
- **voice** (optional): `string` - TTS voice ID for AI responses

## Result

Returns `VoiceStartResult` with:

Returns CommandResult with:
- **handle**: `string` - Session handle (UUID) for correlation
- **wsUrl**: `string` - WebSocket URL to connect for audio streaming
- **roomId**: `string` - Resolved room ID

## Examples

### Start voice chat in general room

```bash
./jtag voice/start --room="general"
```

**Expected result:**
{ handle: "abc-123", wsUrl: "ws://localhost:3000/ws/voice?handle=abc-123" }

### Start with specific model

```bash
./jtag voice/start --room="general" --model="llama3.2:3b"
```

**Expected result:**
{ handle: "def-456", wsUrl: "ws://..." }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help voice/start
```

**Tool:**
```typescript
// Use your help tool with command name 'voice/start'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme voice/start
```

**Tool:**
```typescript
// Use your readme tool with command name 'voice/start'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Voice Start/test/unit/VoiceStartCommand.test.ts
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
npx tsx commands/Voice Start/test/integration/VoiceStartIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/VoiceStartTypes.ts`
- **Browser**: Browser-specific implementation in `browser/VoiceStartBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/VoiceStartServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/VoiceStartCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/VoiceStartIntegration.test.ts`
