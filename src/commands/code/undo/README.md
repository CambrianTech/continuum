# Code Undo Command

Undo a specific change or the last N changes. Applies reverse diffs from the change graph to restore previous file state.

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
./jtag code/undo [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/undo', {
  // your parameters here
});
```

## Parameters

- **changeId** (optional): `string` - UUID of a specific change to undo
- **count** (optional): `number` - Number of most recent changes to undo (default: 1)

## Result

Returns `CodeUndoResult` with:

Returns CommandResult with:
- **changesUndone**: `object[]` - Array of undo results with changeId, filePath, and bytesWritten for each undone change

## Examples

### Undo last change

```bash
./jtag code/undo
```

### Undo last 3 changes

```bash
./jtag code/undo --count=3
```

### Undo specific change

```bash
./jtag code/undo --changeId="abc-123"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/undo
```

**Tool:**
```typescript
// Use your help tool with command name 'code/undo'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/undo
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/undo'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Undo/test/unit/CodeUndoCommand.test.ts
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
npx tsx commands/Code Undo/test/integration/CodeUndoIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeUndoTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeUndoBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeUndoServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeUndoCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeUndoIntegration.test.ts`
