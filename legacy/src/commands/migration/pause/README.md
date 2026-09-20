# Migration Pause Command

Pause an in-flight migration. Can be resumed later from the last checkpoint.

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
./jtag migration/pause 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('migration/pause', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `MigrationPauseResult` with:

Returns CommandResult with:
- **paused**: `boolean` - Whether the migration was successfully paused

## Examples

### Pause active migration

```bash
./jtag migration/pause
```

**Expected result:**
{ paused: true }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help migration/pause
```

**Tool:**
```typescript
// Use your help tool with command name 'migration/pause'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme migration/pause
```

**Tool:**
```typescript
// Use your readme tool with command name 'migration/pause'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Migration Pause/test/unit/MigrationPauseCommand.test.ts
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
npx tsx commands/Migration Pause/test/integration/MigrationPauseIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MigrationPauseTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MigrationPauseBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MigrationPauseServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MigrationPauseCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MigrationPauseIntegration.test.ts`
