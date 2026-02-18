# Logs Config Command

Get or set logging configuration per persona and category

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
./jtag logs/config [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('logs/config', {
  // your parameters here
});
```

## Parameters

- **persona** (optional): `string` - Persona uniqueId to get/set config for
- **action** (optional): `'get' | 'enable' | 'disable'` - Action: get (default), enable, disable
- **category** (optional): `string` - Specific log category to enable/disable

## Result

Returns `LogsConfigResult` with:

Returns CommandResult with:
- **config**: `LoggingConfigData` - Full logging configuration
- **personaConfig**: `{ enabled: boolean; categories: string[] }` - Config for specific persona
- **message**: `string` - Status message

## Examples

```bash
./jtag command-name
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help logs/config
```

**Tool:**
```typescript
// Use your help tool with command name 'logs/config'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme logs/config
```

**Tool:**
```typescript
// Use your readme tool with command name 'logs/config'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Logs Config/test/unit/LogsConfigCommand.test.ts
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
npx tsx commands/Logs Config/test/integration/LogsConfigIntegration.test.ts
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

**internal** - Internal use only, not exposed to AI personas

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/LogsConfigTypes.ts`
- **Browser**: Browser-specific implementation in `browser/LogsConfigBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/LogsConfigServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/LogsConfigCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/LogsConfigIntegration.test.ts`
