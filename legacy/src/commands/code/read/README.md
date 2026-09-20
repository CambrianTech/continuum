# Code Read Command

Read a file or line range from the persona's workspace. Returns content with line numbers and metadata. Supports partial reads via start/end line parameters.

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
./jtag code/read --filePath=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/read', {
  // your parameters here
});
```

## Parameters

- **filePath** (required): `string` - Relative path to file within workspace
- **startLine** (optional): `number` - First line to read (1-indexed, inclusive)
- **endLine** (optional): `number` - Last line to read (1-indexed, inclusive)

## Result

Returns `CodeReadResult` with:

Returns CommandResult with:
- **content**: `string` - File content (or line range)
- **filePath**: `string` - Resolved file path
- **totalLines**: `number` - Total lines in file
- **linesReturned**: `number` - Number of lines returned
- **startLine**: `number` - Start line of returned content
- **endLine**: `number` - End line of returned content
- **sizeBytes**: `number` - File size in bytes

## Examples

### Read entire file

```bash
./jtag code/read --filePath="src/main.ts"
```

### Read line range

```bash
./jtag code/read --filePath="src/main.ts" --startLine=10 --endLine=25
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/read
```

**Tool:**
```typescript
// Use your help tool with command name 'code/read'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/read
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/read'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Read/test/unit/CodeReadCommand.test.ts
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
npx tsx commands/Code Read/test/integration/CodeReadIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeReadTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeReadBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeReadServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeReadCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeReadIntegration.test.ts`
