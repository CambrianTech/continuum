# Grid Job Submit Command

Submit a forge job to a grid node's queue. The node executes when ready (GPU free). Returns a job ID for tracking. Replaces direct SSH forge execution.

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
./jtag grid/job-submit --nodeId=<value> --alloy=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('grid/job-submit', {
  // your parameters here
});
```

## Parameters

- **nodeId** (required): `string` - Target node to queue the job on
- **alloy** (required): `object` - Complete alloy JSON (recipe) to execute
- **priority** (optional): `number` - Queue priority 0-10 (higher = sooner). Default: 5

## Result

Returns `GridJobSubmitResult` with:

Returns CommandResult with:
- **jobId**: `string` - Unique job identifier for tracking and control
- **position**: `number` - Position in the queue (0 = running now)
- **nodeId**: `string` - Node the job was queued on
- **estimatedStart**: `string` - Estimated start time (ISO 8601) based on queue depth

## Examples

### Submit a reasoning forge to bigmama

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help grid/job-submit
```

**Tool:**
```typescript
// Use your help tool with command name 'grid/job-submit'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme grid/job-submit
```

**Tool:**
```typescript
// Use your readme tool with command name 'grid/job-submit'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Grid Job Submit/test/unit/GridJobSubmitCommand.test.ts
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
npx tsx commands/Grid Job Submit/test/integration/GridJobSubmitIntegration.test.ts
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

**owner** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/GridJobSubmitTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GridJobSubmitBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GridJobSubmitServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GridJobSubmitCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GridJobSubmitIntegration.test.ts`
