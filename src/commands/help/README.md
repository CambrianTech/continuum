# Help Command

Discover and display help documentation from command READMEs, auto-generating templates for gaps

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
./jtag help [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('help', {
  // your parameters here
});
```

## Parameters

- **path** (optional): `string` - Command path (e.g., 'interface', 'interface/screenshot')
- **format** (optional): `'markdown' | 'json' | 'rag'` - Output format for different consumers
- **list** (optional): `boolean` - List all available help topics

## Result

Returns `HelpResult` with:

Returns CommandResult with:
- **path**: `string` - The help path that was queried
- **content**: `string` - The help content (markdown, json, or condensed for RAG)
- **topics**: `HelpTopic[]` - List of available help topics (when list=true)
- **generated**: `boolean` - Whether the content was auto-generated (no README found)
- **format**: `string` - The format used for output

## Examples

### List all help topics

```bash
./jtag help --list
```

**Expected result:**
{ topics: [...], content: '# JTAG Command Help\n...' }

### Get help for interface commands

```bash
./jtag help --path=interface
```

**Expected result:**
{ content: '# Interface Commands\n...', generated: false }

### Get condensed help for RAG context

```bash
./jtag help --path=interface/screenshot --format=rag
```

**Expected result:**
{ content: '[condensed content]', format: 'rag' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help help
```

**Tool:**
```typescript
// Use your help tool with command name 'help'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme help
```

**Tool:**
```typescript
// Use your readme tool with command name 'help'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Help/test/unit/HelpCommand.test.ts
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
npx tsx commands/Help/test/integration/HelpIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/HelpTypes.ts`
- **Browser**: Browser-specific implementation in `browser/HelpBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/HelpServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/HelpCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/HelpIntegration.test.ts`
