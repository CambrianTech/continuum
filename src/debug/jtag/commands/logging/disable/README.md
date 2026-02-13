# Logging Disable Command

Disable logging for a persona. Persists to .continuum/logging.json

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
./jtag logging/disable --persona=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('logging/disable', {
  // your parameters here
});
```

## Parameters

- **persona** (required): `string` - Persona uniqueId to disable logging for (e.g., 'helper', 'codereview')
- **category** (optional): `string` - Specific category to disable. If not specified, disables all logging for the persona

## Result

Returns `LoggingDisableResult` with:

Returns CommandResult with:
- **persona**: `string` - The persona that was disabled
- **enabled**: `boolean` - Whether any logging remains enabled for this persona
- **categories**: `string[]` - Categories still enabled (empty if all disabled)
- **message**: `string` - Human-readable status message

## Examples

### Disable all logging for helper persona

```bash
./jtag logging/disable --persona=helper
```

**Expected result:**
{ persona: 'helper', enabled: false, categories: [], message: 'Disabled all logging for helper' }

### Disable only training logs

```bash
./jtag logging/disable --persona=helper --category=training
```

**Expected result:**
{ persona: 'helper', enabled: true, categories: ['cognition'], message: 'Disabled training logging for helper' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help logging/disable
```

**Tool:**
```typescript
// Use your help tool with command name 'logging/disable'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme logging/disable
```

**Tool:**
```typescript
// Use your readme tool with command name 'logging/disable'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Logging Disable/test/unit/LoggingDisableCommand.test.ts
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
npx tsx commands/Logging Disable/test/integration/LoggingDisableIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/LoggingDisableTypes.ts`
- **Browser**: Browser-specific implementation in `browser/LoggingDisableBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/LoggingDisableServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/LoggingDisableCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/LoggingDisableIntegration.test.ts`
