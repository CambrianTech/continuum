# Logging Enable Command

Enable logging for a persona. Persists to .continuum/logging.json

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
./jtag logging/enable --persona=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('logging/enable', {
  // your parameters here
});
```

## Parameters

- **persona** (required): `string` - Persona uniqueId to enable logging for (e.g., 'helper', 'codereview')
- **category** (optional): `string` - Specific category to enable (e.g., 'cognition', 'hippocampus'). If not specified, enables all categories

## Result

Returns `LoggingEnableResult` with:

Returns CommandResult with:
- **persona**: `string` - The persona that was enabled
- **categories**: `string[]` - Categories now enabled for this persona
- **message**: `string` - Human-readable status message

## Examples

### Enable all logging for helper persona

```bash
./jtag logging/enable --persona=helper
```

**Expected result:**
{ persona: 'helper', categories: ['*'], message: 'Enabled all logging for helper' }

### Enable only cognition logs

```bash
./jtag logging/enable --persona=helper --category=cognition
```

**Expected result:**
{ persona: 'helper', categories: ['cognition'], message: 'Enabled cognition logging for helper' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help logging/enable
```

**Tool:**
```typescript
// Use your help tool with command name 'logging/enable'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme logging/enable
```

**Tool:**
```typescript
// Use your readme tool with command name 'logging/enable'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Logging Enable/test/unit/LoggingEnableCommand.test.ts
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
npx tsx commands/Logging Enable/test/integration/LoggingEnableIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/LoggingEnableTypes.ts`
- **Browser**: Browser-specific implementation in `browser/LoggingEnableBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/LoggingEnableServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/LoggingEnableCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/LoggingEnableIntegration.test.ts`
