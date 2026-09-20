# Code Search Command

Search for a regex pattern across workspace files. Respects .gitignore, supports glob-based file filtering. Returns matching lines with context.

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
./jtag code/search --pattern=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/search', {
  // your parameters here
});
```

## Parameters

- **pattern** (required): `string` - Regex pattern to search for
- **fileGlob** (optional): `string` - Glob pattern to filter files (e.g., '*.ts', 'src/**/*.rs')
- **maxResults** (optional): `number` - Maximum number of matches to return (default: 100)

## Result

Returns `CodeSearchResult` with:

Returns CommandResult with:
- **matches**: `object[]` - Array of SearchMatch objects with filePath, lineNumber, lineContent, matchStart, matchEnd
- **totalMatches**: `number` - Total number of matches found
- **filesSearched**: `number` - Number of files searched

## Examples

### Search for function definitions

```bash
./jtag code/search --pattern="function\s+\w+" --fileGlob="*.ts"
```

### Search for TODO comments

```bash
./jtag code/search --pattern="TODO|FIXME|HACK" --maxResults=50
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/search
```

**Tool:**
```typescript
// Use your help tool with command name 'code/search'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/search
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/search'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Search/test/unit/CodeSearchCommand.test.ts
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
npx tsx commands/Code Search/test/integration/CodeSearchIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeSearchTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeSearchBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeSearchServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeSearchCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeSearchIntegration.test.ts`
