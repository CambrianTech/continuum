# Runtime Metrics Command

Query Rust module performance metrics including latency percentiles, command counts, and slow command tracking. Enables AI-driven system analysis and optimization.

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
./jtag runtime/metrics [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('runtime/metrics', {
  // your parameters here
});
```

## Parameters

- **mode** (optional): `'all' | 'module' | 'slow' | 'list'` - Query mode: 'all' for all modules (default), 'module' for specific module, 'slow' for recent slow commands, 'list' for module configs
- **module** (optional): `string` - Module name when mode='module' (e.g., 'data', 'embedding', 'cognition')

## Result

Returns `RuntimeMetricsResult` with:

Returns CommandResult with:
- **modules**: `ModuleMetrics[]` - Array of module metrics (when mode='all' or 'module')
- **slowCommands**: `SlowCommand[]` - Array of slow commands (when mode='slow')
- **moduleConfigs**: `ModuleConfig[]` - Array of module configurations (when mode='list')
- **count**: `number` - Number of items in the result
- **thresholdMs**: `number` - Slow command threshold in ms (when mode='slow')

## Examples

### Get metrics for all modules

```bash
./jtag runtime/metrics
```

**Expected result:**
{ modules: [...], count: 13 }

### Get metrics for a specific module

```bash
./jtag runtime/metrics --mode=module --module=embedding
```

**Expected result:**
{ modules: [{ moduleName: 'embedding', avgTimeMs: 90, p99Ms: 552, ... }], count: 1 }

### List recent slow commands

```bash
./jtag runtime/metrics --mode=slow
```

**Expected result:**
{ slowCommands: [...], count: 5, thresholdMs: 50 }

### List all module configurations

```bash
./jtag runtime/metrics --mode=list
```

**Expected result:**
{ moduleConfigs: [...], count: 13 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help runtime/metrics
```

**Tool:**
```typescript
// Use your help tool with command name 'runtime/metrics'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme runtime/metrics
```

**Tool:**
```typescript
// Use your readme tool with command name 'runtime/metrics'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Runtime Metrics/test/unit/RuntimeMetricsCommand.test.ts
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
npx tsx commands/Runtime Metrics/test/integration/RuntimeMetricsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/RuntimeMetricsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/RuntimeMetricsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/RuntimeMetricsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/RuntimeMetricsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/RuntimeMetricsIntegration.test.ts`
