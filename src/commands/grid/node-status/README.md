# Grid Node Status Command

Query a grid node's current state: GPU utilization, running jobs, queue depth, temperature. Uses the grid transport layer (Tailscale now, Reticulum later).

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
./jtag grid/node-status [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('grid/node-status', {
  // your parameters here
});
```

## Parameters

- **nodeId** (optional): `string` - Target node name or ID. Default: all known nodes.

## Result

Returns `GridNodeStatusResult` with:

Returns CommandResult with:
- **state**: `string` - Node state: 'ready' | 'busy' | 'offline' | 'error'
- **gpu**: `object` - GPU info: { name, utilization, memoryUsedMb, memoryTotalMb, temperatureC }
- **jobs**: `object` - Array of running jobs: { pid, type, detail, cpu, mem }
- **queue**: `object` - Array of queued alloys: { name, path }
- **nodeId**: `string` - The node that responded
- **timestamp**: `string` - ISO 8601 timestamp of the status report

## Examples

### Check bigmama's status

```bash
undefined
```

### Check all nodes

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help grid/node-status
```

**Tool:**
```typescript
// Use your help tool with command name 'grid/node-status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme grid/node-status
```

**Tool:**
```typescript
// Use your readme tool with command name 'grid/node-status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Grid Node Status/test/unit/GridNodeStatusCommand.test.ts
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
npx tsx commands/Grid Node Status/test/integration/GridNodeStatusIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GridNodeStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GridNodeStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GridNodeStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GridNodeStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GridNodeStatusIntegration.test.ts`
