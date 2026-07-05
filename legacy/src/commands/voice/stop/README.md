# Voice Stop Command

Stop an active voice chat session

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
./jtag voice/stop [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('voice/stop', {
  // your parameters here
});
```

## Parameters

- **handle** (optional): `string` - Session handle to stop (defaults to current session)

## Result

Returns `VoiceStopResult` with:

Returns CommandResult with:
- **stopped**: `boolean` - Whether the session was successfully stopped
- **handle**: `string` - Handle of the stopped session
- **duration**: `number` - Session duration in seconds

## Examples

### Stop current voice session

```bash
./jtag voice/stop
```

**Expected result:**
{ stopped: true, duration: 120 }

### Stop specific session by handle

```bash
./jtag voice/stop --handle="abc-123"
```

**Expected result:**
{ stopped: true, handle: "abc-123", duration: 45 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help voice/stop
```

**Tool:**
```typescript
// Use your help tool with command name 'voice/stop'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme voice/stop
```

**Tool:**
```typescript
// Use your readme tool with command name 'voice/stop'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Voice Stop/test/unit/VoiceStopCommand.test.ts
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
npx tsx commands/Voice Stop/test/integration/VoiceStopIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/VoiceStopTypes.ts`
- **Browser**: Browser-specific implementation in `browser/VoiceStopBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/VoiceStopServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/VoiceStopCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/VoiceStopIntegration.test.ts`
