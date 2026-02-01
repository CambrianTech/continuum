# Code Diff Command

Preview an edit as a unified diff without applying it. Useful for reviewing changes before committing them. Uses the same edit modes as code/edit.

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
./jtag code/diff --filePath=<value> --editType=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/diff', {
  // your parameters here
});
```

## Parameters

- **filePath** (required): `string` - Relative path to file within workspace
- **editType** (required): `string` - Edit mode: 'search_replace', 'line_range', 'insert_at', or 'append'
- **search** (optional): `string` - Text to find (for search_replace mode)
- **replace** (optional): `string` - Replacement text (for search_replace mode)
- **replaceAll** (optional): `boolean` - Replace all occurrences (for search_replace mode)
- **startLine** (optional): `number` - Start line (for line_range mode)
- **endLine** (optional): `number` - End line (for line_range mode)
- **newContent** (optional): `string` - New content (for line_range mode)
- **line** (optional): `number` - Line number (for insert_at mode)
- **content** (optional): `string` - Content to insert or append

## Result

Returns `CodeDiffResult` with:

Returns CommandResult with:
- **unified**: `string` - Unified diff text showing the proposed changes

## Examples

### Preview a search-replace diff

```bash
./jtag code/diff --filePath="src/main.ts" --editType="search_replace" --search="console.log" --replace="logger.info"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/diff
```

**Tool:**
```typescript
// Use your help tool with command name 'code/diff'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/diff
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/diff'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Diff/test/unit/CodeDiffCommand.test.ts
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
npx tsx commands/Code Diff/test/integration/CodeDiffIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeDiffTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeDiffBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeDiffServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeDiffCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeDiffIntegration.test.ts`
