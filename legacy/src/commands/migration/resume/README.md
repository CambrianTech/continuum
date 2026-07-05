# Migration Resume Command

Resume a paused migration from its last checkpoint

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
./jtag migration/resume 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/resume', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `MigrationResumeResult` with:

Returns CommandResult with:
- **total**: `number` - Total records across all collections
- **migrated**: `number` - Records successfully migrated after resume
- **failed**: `number` - Records that failed to migrate
- **paused**: `boolean` - Whether migration is still paused
- **collections**: `object[]` - Per-collection migration status

## Examples

### Resume paused migration

```bash
./jtag migration/resume
```

**Expected result:**
{ total: 50000, migrated: 50000, failed: 0, paused: false }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/resume
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/resume'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/resume
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/resume'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Resume/test/unit/MigrationResumeCommand.test.ts
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
npx tsx commands/Migration Resume/test/integration/MigrationResumeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MigrationResumeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationResumeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationResumeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationResumeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationResumeIntegration.test.ts`
