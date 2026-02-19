# Collaboration Live Start Command

Start a live session with selected participants. Creates or finds the DM room for the participant set, then joins the live session. Like Discord's group call - select users, click call.

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
./jtag collaboration/live/start --participants=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('collaboration/live/start', {
  // your parameters here
});
```

## Parameters

- **participants** (required): `string | string[]` - User IDs or uniqueIds to include (current user auto-added)
- **name** (optional): `string` - Optional room name (defaults to participant names)
- **withVideo** (optional): `boolean` - Start with video enabled (default: false, audio only)

## Result

Returns `CollaborationLiveStartResult` with:

Returns CommandResult with:
- **roomId**: `UUID` - The DM room ID (created or found)
- **sessionId**: `UUID` - The live session ID
- **room**: `RoomEntity` - The full room entity
- **session**: `LiveSessionEntity` - The live session with participants
- **existed**: `boolean` - Whether room already existed (true) or was created (false)
- **participants**: `LiveParticipant[]` - Current participants in the live session

## Examples

### Start live call with Helper AI

```bash
./jtag collaboration/live/start --participants="helper"
```

**Expected result:**
{ roomId: "...", sessionId: "...", existed: false }

### Start group call with multiple AIs

```bash
./jtag collaboration/live/start --participants='["helper","teacher","codereview"]'
```

**Expected result:**
{ roomId: "...", sessionId: "...", participants: [...] }

### Start video call

```bash
./jtag collaboration/live/start --participants="helper" --withVideo=true
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help collaboration/live/start
```

**Tool:**
```typescript
// Use your help tool with command name 'collaboration/live/start'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme collaboration/live/start
```

**Tool:**
```typescript
// Use your readme tool with command name 'collaboration/live/start'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Collaboration Live Start/test/unit/CollaborationLiveStartCommand.test.ts
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
npx tsx commands/Collaboration Live Start/test/integration/CollaborationLiveStartIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CollaborationLiveStartTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CollaborationLiveStartBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CollaborationLiveStartServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CollaborationLiveStartCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CollaborationLiveStartIntegration.test.ts`
