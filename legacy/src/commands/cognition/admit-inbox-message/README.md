# Cognition Admit Inbox Message Command

Run the per-persona admission gate over a single InboxMessage. Returns the typed AdmissionDecision (Admit | Drop | Quarantine) plus the post-call admitted-engram count and trace seam count. Side effects: admitted engram → store, content_hash → dedup record, AIRC event_id → replay-protection record. Wraps the Rust IPC handler shipped in #1121 PR-4.

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
./jtag cognition/admit-inbox-message --personaId=<value> --message=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('cognition/admit-inbox-message', {
  // your parameters here
});
```

## Parameters

- **personaId** (required): `string` - UUID of the persona whose admission gate runs
- **message** (required): `Record<string, unknown>` - InboxMessageRequest — the candidate inbox message to admit. Recipe pipelines pass $signal or the drained-frame entry.

## Result

Returns `CognitionAdmitInboxMessageResult` with:

Returns CommandResult with:
- **decision**: `Record<string, unknown>` - Typed AdmissionDecision (Admit | Drop | Quarantine). See shared/generated/persona/AdmissionDecision.ts for shape.
- **engramCount**: `number` - Total engrams in the persona's admitted store after this call
- **traceSeamCount**: `number` - Number of cognition trace seams emitted during this admission

## Examples

### Admit an inbox message during a chat recipe pipeline

```bash
./jtag cognition/admit-inbox-message --personaId="<uuid>" --message='{"content":"hello","sender_id":"<uuid>"}'
```

**Expected result:**
{ decision: { decision: 'Admit', data: {...} }, engramCount: 12, traceSeamCount: 3 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help cognition/admit-inbox-message
```

**Tool:**
```typescript
// Use your help tool with command name 'cognition/admit-inbox-message'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme cognition/admit-inbox-message
```

**Tool:**
```typescript
// Use your readme tool with command name 'cognition/admit-inbox-message'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Cognition Admit Inbox Message/test/unit/CognitionAdmitInboxMessageCommand.test.ts
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
npx tsx commands/Cognition Admit Inbox Message/test/integration/CognitionAdmitInboxMessageIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CognitionAdmitInboxMessageTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CognitionAdmitInboxMessageBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CognitionAdmitInboxMessageServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CognitionAdmitInboxMessageCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CognitionAdmitInboxMessageIntegration.test.ts`
