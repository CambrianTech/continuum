# Ai Context Slice Command

Retrieve full content of a context item by ID - companion to context/search for getting complete entity data

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
./jtag ai/context/slice --id=<value> --type=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/context/slice', {
  // your parameters here
});
```

## Parameters

- **id** (required): `string` - Entity ID to retrieve
- **type** (required): `string` - Entity type: chat_messages, memories, timeline_events, tool_results
- **personaId** (optional): `string` - Persona ID for per-persona database lookup
- **includeRelated** (optional): `boolean` - Include related items (replies, thread context)
- **relatedLimit** (optional): `number` - Max related items to include (default: 5)

## Result

Returns `AiContextSliceResult` with:

Returns CommandResult with:
- **item**: `ContextSliceItem | null` - The full entity content or null if not found
- **durationMs**: `number` - Retrieval time in milliseconds

## Examples

### Get full content of a memory

```bash
./jtag ai/context/slice --id="abc123" --type="memories"
```

**Expected result:**
{ item: { id: 'abc123', content: '...', ... }, durationMs: 12 }

### Get chat message with thread context

```bash
./jtag ai/context/slice --id="def456" --type="chat_messages" --includeRelated=true
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/context/slice
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/context/slice'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/context/slice
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/context/slice'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Context Slice/test/unit/AiContextSliceCommand.test.ts
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
npx tsx commands/Ai Context Slice/test/integration/AiContextSliceIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AiContextSliceTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiContextSliceBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiContextSliceServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiContextSliceCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiContextSliceIntegration.test.ts`
