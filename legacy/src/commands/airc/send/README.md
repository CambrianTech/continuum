# Airc Send Command

Send a message to the airc mesh from inside Continuum. Wraps the airc CLI's `airc send` command — broadcasts to a channel by default, DMs a peer when peer is provided. First-class surface for the AircBridge integration (continuum#967, AGENT-BACKBONE-INTEGRATION §11.2): personas (or any caller) can publish to the cross-machine peer mesh that humans + Claude Code + Codex tabs share. Outbox direction only; inbox routing (airc → persona inbox) is a separate v0.5 follow-up requiring an embedded `airc connect` Monitor process tree.

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
./jtag airc/send --message=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('airc/send', {
  // your parameters here
});
```

## Parameters

- **message** (required): `string` - Message body to send. Plain text; airc handles encryption per its substrate rules.
- **channel** (optional): `string` - Target channel (without leading #). Defaults to airc's auto-scoped project room (typically the cwd's git org → e.g. 'cambriantech'). Use 'general' for the lobby.
- **peer** (optional): `string` - Target peer name for a DM (e.g. 'continuum-2c54'). When omitted, message is a broadcast to the channel. When provided, message is addressed to that peer specifically (still in the channel; airc envelopes the addressing).

## Result

Returns `AircSendResult` with:

Returns CommandResult with:
- **delivered**: `boolean` - True if airc CLI exited 0 and the message reached the local audit log. Note: airc's own substrate may queue (transient gist failure, secondary rate limit) — `delivered=true` means handed off to airc, not necessarily landed on a peer's bearer yet. Check airc#381 for the queue/retry semantics.
- **channel**: `string` - Resolved channel name the message was sent to (after airc's auto-scoping).
- **stderr**: `string` - Any stderr output from the airc CLI (warnings, [QUEUED] markers, [GONE] markers, etc.). Empty on clean delivery. Surfaced so callers can react to airc-substrate signals (rate-limit, channel-dissolved, etc.) rather than treating them as silent.

## Examples

### Broadcast to the auto-scoped project room

```bash
undefined
```

### Broadcast to #general explicitly

```bash
undefined
```

### DM a specific peer

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help airc/send
```

**Tool:**
```typescript
// Use your help tool with command name 'airc/send'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme airc/send
```

**Tool:**
```typescript
// Use your readme tool with command name 'airc/send'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Airc Send/test/unit/AircSendCommand.test.ts
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
npx tsx commands/Airc Send/test/integration/AircSendIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AircSendTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AircSendBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AircSendServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AircSendCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AircSendIntegration.test.ts`
