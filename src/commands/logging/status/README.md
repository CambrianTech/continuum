# Logging Status Command

Show current logging configuration for all personas or a specific persona

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
./jtag logging/status [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('logging/status', {
  // your parameters here
});
```

## Parameters

- **persona** (optional): `string` - Specific persona to show status for. If not specified, shows all personas

## Result

Returns `LoggingStatusResult` with:

Returns CommandResult with:
- **personas**: `object[]` - Array of persona logging statuses with { persona, enabled, categories }
- **systemEnabled**: `boolean` - Whether system logging is enabled
- **defaultEnabled**: `boolean` - Default enabled state for unconfigured personas
- **availableCategories**: `string[]` - List of valid category names
- **summary**: `string` - Human-readable summary of logging state

## Examples

### Show all logging status

```bash
./jtag logging/status
```

**Expected result:**
{ personas: [...], systemEnabled: true, defaultEnabled: false, summary: '2/11 personas logging enabled' }

### Show status for specific persona

```bash
./jtag logging/status --persona=helper
```

**Expected result:**
{ personas: [{ persona: 'helper', enabled: true, categories: ['cognition'] }], summary: 'helper: ON (cognition)' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help logging/status
```

**Tool:**
```typescript
// Use your help tool with command name 'logging/status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme logging/status
```

**Tool:**
```typescript
// Use your readme tool with command name 'logging/status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Logging Status/test/unit/LoggingStatusCommand.test.ts
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
npx tsx commands/Logging Status/test/integration/LoggingStatusIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/LoggingStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/LoggingStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/LoggingStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/LoggingStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/LoggingStatusIntegration.test.ts`
