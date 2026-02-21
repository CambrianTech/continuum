# Migration Rollback Command

Revert to the previous connection string after a cutover. Source data is never deleted.

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
./jtag migration/rollback --current=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/rollback', {
  // your parameters here
});
```

## Parameters

- **current** (required): `string` - Current (target) connection string to roll back from

## Result

Returns `MigrationRollbackResult` with:

Returns CommandResult with:
- **rolledBack**: `boolean` - Whether rollback was successful
- **restoredConnection**: `string` - The connection string that was restored

## Examples

### Roll back to SQLite after failed cutover

```bash
./jtag migration/rollback --current="postgres://joel@localhost:5432/continuum"
```

**Expected result:**
{ rolledBack: true, restoredConnection: "/Users/joel/.continuum/data/database.sqlite" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/rollback
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/rollback'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/rollback
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/rollback'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Rollback/test/unit/MigrationRollbackCommand.test.ts
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
npx tsx commands/Migration Rollback/test/integration/MigrationRollbackIntegration.test.ts
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

**system** - System-level command, requires elevated permissions

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/MigrationRollbackTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationRollbackBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationRollbackServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationRollbackCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationRollbackIntegration.test.ts`
