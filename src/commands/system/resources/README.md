# System Resources Command

Query system resource usage: CPU load, memory pressure, swap, and optionally top processes by CPU and memory. Uses sysinfo for cross-platform monitoring (macOS/Linux/Windows). On Apple Silicon, memory pressure directly impacts GPU headroom since VRAM is unified.

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
./jtag system/resources [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('system/resources', {
  // your parameters here
});
```

## Parameters

- **includeProcesses** (optional): `boolean` - Include top processes by CPU and memory usage. More expensive — iterates all OS processes.
- **topN** (optional): `number` - Number of top processes to return (default: 10). Only used when includeProcesses=true.

## Result

Returns `SystemResourcesResult` with:

Returns CommandResult with:
- **cpu**: `CpuStatsInfo` - CPU stats: brand, core counts, global usage (0.0-1.0), per-core usage
- **memory**: `MemoryStatsInfo` - Memory stats: total, used, available bytes, pressure (0.0-1.0), swap
- **processes**: `ProcessStatsInfo` - Top processes by CPU and memory (only when includeProcesses=true)
- **timestampMs**: `number` - Snapshot timestamp in milliseconds since epoch
- **uptimeSeconds**: `number` - System uptime in seconds

## Examples

### Get CPU and memory overview

```bash
./jtag system/resources
```

**Expected result:**
{ cpu: { brand: 'Apple M1 Pro', globalUsage: 0.65, physicalCores: 10 }, memory: { pressure: 0.82, totalBytes: 34359738368 }, ... }

### Get full report with top 10 processes

```bash
./jtag system/resources --includeProcesses=true --topN=10
```

**Expected result:**
{ cpu: {...}, memory: {...}, processes: { topByCpu: [...], topByMemory: [...] } }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help system/resources
```

**Tool:**
```typescript
// Use your help tool with command name 'system/resources'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme system/resources
```

**Tool:**
```typescript
// Use your readme tool with command name 'system/resources'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/System Resources/test/unit/SystemResourcesCommand.test.ts
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
npx tsx commands/System Resources/test/integration/SystemResourcesIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SystemResourcesTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SystemResourcesBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SystemResourcesServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SystemResourcesCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SystemResourcesIntegration.test.ts`
