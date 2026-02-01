# Code Write Command

Write or create a file in the persona's workspace. Creates a ChangeNode in the change graph for undo support. File extension must be in the allowlist.

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
./jtag code/write --filePath=<value> --content=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/write', {
  // your parameters here
});
```

## Parameters

- **filePath** (required): `string` - Relative path to file within workspace
- **content** (required): `string` - File content to write
- **description** (optional): `string` - Description of what this change does

## Result

Returns `CodeWriteResult` with:

Returns CommandResult with:
- **changeId**: `string` - UUID of the ChangeNode created (for undo)
- **filePath**: `string` - Resolved file path
- **bytesWritten**: `number` - Number of bytes written

## Examples

### Create a new file

```bash
./jtag code/write --filePath="src/utils.ts" --content="export function greet() { return 'hello'; }" --description="Add greet utility"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/write
```

**Tool:**
```typescript
// Use your help tool with command name 'code/write'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/write
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/write'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Write/test/unit/CodeWriteCommand.test.ts
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
npx tsx commands/Code Write/test/integration/CodeWriteIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeWriteTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeWriteBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeWriteServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeWriteCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeWriteIntegration.test.ts`
