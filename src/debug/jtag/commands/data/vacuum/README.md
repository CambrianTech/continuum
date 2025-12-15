# Data Vacuum Command

Reclaim disk space after bulk deletes (VACUUM for SQLite, OPTIMIZE for MySQL, etc.)

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
./jtag data/vacuum [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('data/vacuum', {
  // your parameters here
});
```

## Parameters

- **dbHandle** (optional): `string` - Database handle to vacuum (defaults to primary database)

## Result

Returns `DataVacuumResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether vacuum completed successfully
- **dbHandle**: `string` - Database handle that was vacuumed
- **beforeSize**: `number` - Database size before vacuum (bytes)
- **afterSize**: `number` - Database size after vacuum (bytes)
- **duration**: `number` - Duration of vacuum operation (ms)
- **timestamp**: `string` - When vacuum completed

## Examples

### Vacuum primary database

```bash
undefined
```

### Vacuum specific database handle

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help data/vacuum
```

**Tool:**
```typescript
// Use your help tool with command name 'data/vacuum'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme data/vacuum
```

**Tool:**
```typescript
// Use your readme tool with command name 'data/vacuum'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Data Vacuum/test/unit/DataVacuumCommand.test.ts
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
npx tsx commands/Data Vacuum/test/integration/DataVacuumIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DataVacuumTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DataVacuumBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DataVacuumServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DataVacuumCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DataVacuumIntegration.test.ts`
