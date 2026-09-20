# Migration Status Command

Get current migration progress with per-collection breakdown

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
./jtag migration/status 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/status', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `MigrationStatusResult` with:

Returns CommandResult with:
- **total**: `number` - Total records across all collections
- **migrated**: `number` - Records successfully migrated so far
- **failed**: `number` - Records that failed to migrate
- **paused**: `boolean` - Whether migration is currently paused
- **collections**: `object[]` - Per-collection status (collection, status, total, migrated, failed, error)

## Examples

### Check migration progress

```bash
./jtag migration/status
```

**Expected result:**
{ total: 50000, migrated: 25000, failed: 0, paused: false, collections: [...] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/status
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/status
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Status/test/unit/MigrationStatusCommand.test.ts
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
npx tsx commands/Migration Status/test/integration/MigrationStatusIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MigrationStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationStatusIntegration.test.ts`
