# Development Generate Help Command

Display comprehensive generator documentation including spec reference, example specs, type reference, access levels, workflow guide, and audit information. This is the primary documentation entry point for AI agents learning to use the generator.

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
./jtag development/generate/help [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('development/generate/help', {
  // your parameters here
});
```

## Parameters

- **topic** (optional): `'full' | 'spec' | 'types' | 'examples' | 'audit' | 'workflow'` - Specific help topic. Omit for full documentation.

## Result

Returns `DevelopmentGenerateHelpResult` with:

Returns CommandResult with:
- **content**: `string` - Formatted help text with spec reference, examples, and workflow guidance
- **topic**: `string` - The topic that was displayed

## Examples

### Full generator documentation

```bash
./jtag development/generate/help
```

**Expected result:**
{ content: '╔═══...374 lines of help...', topic: 'full' }

### Just the spec reference

```bash
./jtag development/generate/help --topic=spec
```

**Expected result:**
{ content: 'SPEC REFERENCE (CommandSpec):\n...', topic: 'spec' }

### Example specs to copy from

```bash
./jtag development/generate/help --topic=examples
```

**Expected result:**
{ content: 'EXAMPLE SPECS:\n...minimal, standard, rust-ipc, browser-only...', topic: 'examples' }

### Type reference for writing params/results

```bash
./jtag development/generate/help --topic=types
```

**Expected result:**
{ content: 'TYPE REFERENCE:\n...string, number, boolean, string[]...', topic: 'types' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help development/generate/help
```

**Tool:**
```typescript
// Use your help tool with command name 'development/generate/help'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme development/generate/help
```

**Tool:**
```typescript
// Use your readme tool with command name 'development/generate/help'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Development Generate Help/test/unit/DevelopmentGenerateHelpCommand.test.ts
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
npx tsx commands/Development Generate Help/test/integration/DevelopmentGenerateHelpIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DevelopmentGenerateHelpTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DevelopmentGenerateHelpBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DevelopmentGenerateHelpServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DevelopmentGenerateHelpCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DevelopmentGenerateHelpIntegration.test.ts`
