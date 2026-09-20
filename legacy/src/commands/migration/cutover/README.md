# Migration Cutover Command

Switch all operations from current adapter to the migration target. Saves previous connection for rollback.

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
./jtag migration/cutover --current=<value> --target=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/cutover', {
  // your parameters here
});
```

## Parameters

- **current** (required): `string` - Current (source) connection string to decommission
- **target** (required): `string` - Target connection string to switch to

## Result

Returns `MigrationCutoverResult` with:

Returns CommandResult with:
- **cutover**: `boolean` - Whether cutover was successful
- **previousConnection**: `string` - The connection string that was replaced (for rollback)

## Examples

### Switch from SQLite to PostgreSQL

```bash
./jtag migration/cutover --current="$HOME/.continuum/data/database.sqlite" --target="postgres://joel@localhost:5432/continuum"
```

**Expected result:**
{ cutover: true, previousConnection: "/Users/joel/.continuum/data/database.sqlite" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/cutover
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/cutover'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/cutover
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/cutover'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Cutover/test/unit/MigrationCutoverCommand.test.ts
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
npx tsx commands/Migration Cutover/test/integration/MigrationCutoverIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MigrationCutoverTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationCutoverBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationCutoverServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationCutoverCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationCutoverIntegration.test.ts`
