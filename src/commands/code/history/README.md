# Code History Command

Get change history for a specific file or the entire workspace. Returns change graph nodes with diffs, timestamps, and descriptions.

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
./jtag code/history [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/history', {
  // your parameters here
});
```

## Parameters

- **filePath** (optional): `string` - Filter history to a specific file (optional, defaults to all)
- **limit** (optional): `number` - Maximum number of history entries to return (default: 50)

## Result

Returns `CodeHistoryResult` with:

Returns CommandResult with:
- **nodes**: `object[]` - Array of ChangeNode objects with id, filePath, operation, timestamp, description, and diffs
- **totalCount**: `number` - Total number of changes in history

## Examples

### Get all workspace history

```bash
./jtag code/history
```

### Get history for specific file

```bash
./jtag code/history --filePath="src/main.ts" --limit=10
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/history
```

**Tool:**
```typescript
// Use your help tool with command name 'code/history'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/history
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/history'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code History/test/unit/CodeHistoryCommand.test.ts
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
npx tsx commands/Code History/test/integration/CodeHistoryIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeHistoryTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeHistoryBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeHistoryServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeHistoryCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeHistoryIntegration.test.ts`
