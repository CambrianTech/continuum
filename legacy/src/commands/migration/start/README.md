# Migration Start Command

Start streaming data migration between any two storage adapters (e.g., SQLite to PostgreSQL)

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
./jtag migration/start --source=<value> --target=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/start', {
  // your parameters here
});
```

## Parameters

- **source** (required): `string` - Source connection string (file path for SQLite, postgres:// URL for Postgres)
- **target** (required): `string` - Target connection string (file path for SQLite, postgres:// URL for Postgres)
- **batchSize** (optional): `number` - Records per batch (default: 500)
- **throttleMs** (optional): `number` - Milliseconds to pause between batches (default: 10)
- **collections** (optional): `string[]` - Specific collections to migrate (default: all)

## Result

Returns `MigrationStartResult` with:

Returns CommandResult with:
- **total**: `number` - Total records across all collections
- **migrated**: `number` - Records successfully migrated
- **failed**: `number` - Records that failed to migrate
- **paused**: `boolean` - Whether migration was paused
- **collections**: `object[]` - Per-collection migration status

## Examples

### Migrate SQLite to PostgreSQL

```bash
./jtag migration/start --source="$HOME/.continuum/data/database.sqlite" --target="postgres://joel@localhost:5432/continuum"
```

**Expected result:**
{ total: 50000, migrated: 50000, failed: 0, paused: false }

### Migrate with custom batch size

```bash
./jtag migration/start --source="/path/to/source.db" --target="postgres://localhost/mydb" --batchSize=1000 --throttleMs=50
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/start
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/start'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/start
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/start'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Start/test/unit/MigrationStartCommand.test.ts
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
npx tsx commands/Migration Start/test/integration/MigrationStartIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MigrationStartTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationStartBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationStartServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationStartCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationStartIntegration.test.ts`
