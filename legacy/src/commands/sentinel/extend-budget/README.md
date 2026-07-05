# Sentinel Extend Budget Command

Extend budget limits for a running or paused pipeline. Merges new limits into existing checkpoint budget.

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
./jtag sentinel/extend-budget --handle=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('sentinel/extend-budget', {
  // your parameters here
});
```

## Parameters

- **handle** (required): `string` - Sentinel handle ID to extend budget for
- **maxTimeSecs** (optional): `number` - New max time limit in seconds (e.g., 3600 for 1 hour)
- **maxCostUsd** (optional): `number` - New max cost limit in USD (e.g., 5.00)
- **maxTokens** (optional): `number` - New max token limit (e.g., 1000000)
- **maxIterations** (optional): `number` - New max iteration limit (full pipeline loops, not agent turns)

## Result

Returns `SentinelExtendBudgetResult` with:

Returns CommandResult with:
- **handle**: `string` - The sentinel handle whose budget was extended
- **budgetLimits**: `BudgetLimits` - The new merged budget limits after extension

## Examples

### Double time limit for a running pipeline

```bash
./jtag sentinel/extend-budget --handle=abc123 --maxTimeSecs=7200
```

**Expected result:**
{ handle: 'abc123', budgetLimits: { maxTimeSecs: 7200, ... } }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help sentinel/extend-budget
```

**Tool:**
```typescript
// Use your help tool with command name 'sentinel/extend-budget'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme sentinel/extend-budget
```

**Tool:**
```typescript
// Use your readme tool with command name 'sentinel/extend-budget'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Sentinel Extend Budget/test/unit/SentinelExtendBudgetCommand.test.ts
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
npx tsx commands/Sentinel Extend Budget/test/integration/SentinelExtendBudgetIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SentinelExtendBudgetTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SentinelExtendBudgetBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SentinelExtendBudgetServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SentinelExtendBudgetCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SentinelExtendBudgetIntegration.test.ts`
