# Code Edit Command

Edit a file using search-replace, line-range replacement, insert-at, or append. Creates a ChangeNode for undo. Safer than full file write for targeted modifications.

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
./jtag code/edit --filePath=<value> --editType=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/edit', {
  // your parameters here
});
```

## Parameters

- **filePath** (required): `string` - Relative path to file within workspace
- **editType** (required): `string` - Edit mode: 'search_replace', 'line_range', 'insert_at', or 'append'
- **search** (optional): `string` - Text to find (for search_replace mode)
- **replace** (optional): `string` - Replacement text (for search_replace mode)
- **replaceAll** (optional): `boolean` - Replace all occurrences (for search_replace mode, default: false)
- **startLine** (optional): `number` - Start line (for line_range mode, 1-indexed)
- **endLine** (optional): `number` - End line (for line_range mode, 1-indexed, inclusive)
- **newContent** (optional): `string` - New content (for line_range mode)
- **line** (optional): `number` - Line number to insert at (for insert_at mode)
- **content** (optional): `string` - Content to insert or append
- **description** (optional): `string` - Description of what this change does

## Result

Returns `CodeEditResult` with:

Returns CommandResult with:
- **changeId**: `string` - UUID of the ChangeNode created (for undo)
- **filePath**: `string` - Resolved file path
- **bytesWritten**: `number` - New file size in bytes

## Examples

### Search and replace

```bash
./jtag code/edit --filePath="src/main.ts" --editType="search_replace" --search="old text" --replace="new text"
```

### Replace line range

```bash
./jtag code/edit --filePath="src/main.ts" --editType="line_range" --startLine=5 --endLine=10 --newContent="replacement content"
```

### Insert at line

```bash
./jtag code/edit --filePath="src/main.ts" --editType="insert_at" --line=1 --content="// Header comment"
```

### Append to file

```bash
./jtag code/edit --filePath="src/main.ts" --editType="append" --content="// Footer"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/edit
```

**Tool:**
```typescript
// Use your help tool with command name 'code/edit'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/edit
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/edit'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Edit/test/unit/CodeEditCommand.test.ts
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
npx tsx commands/Code Edit/test/integration/CodeEditIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeEditTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeEditBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeEditServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeEditCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeEditIntegration.test.ts`
