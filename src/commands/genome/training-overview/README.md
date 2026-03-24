# Genome Training Overview Command

Aggregate all training data across local and grid nodes in one call. Returns adapters with loss histories, academy sessions, and per-node stats. Used by the training dashboard to avoid sequential grid/send chains from the browser.

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
./jtag genome/training-overview [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/training-overview', {
  // your parameters here
});
```

## Parameters

- **includeGrid** (optional): `boolean` - Include data from remote grid nodes (default: true)
- **personaId** (optional): `string` - Filter to a specific persona UUID

## Result

Returns `GenomeTrainingOverviewResult` with:

Returns CommandResult with:
- **adapters**: `object[]` - All adapters with training metrics, loss histories, and node info
- **sessions**: `object[]` - All academy sessions (active and recent completed)
- **nodes**: `object[]` - Grid node summary (name, GPU, adapter count)
- **summary**: `object` - Aggregate stats: total adapters, total sessions, best loss, avg maturity

## Examples

### Get all training data across grid

```bash
./jtag genome/training-overview
```

**Expected result:**
{ adapters: [...], sessions: [...], nodes: [...], summary: {...} }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/training-overview
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/training-overview'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/training-overview
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/training-overview'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Training Overview/test/unit/GenomeTrainingOverviewCommand.test.ts
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
npx tsx commands/Genome Training Overview/test/integration/GenomeTrainingOverviewIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeTrainingOverviewTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeTrainingOverviewBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeTrainingOverviewServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeTrainingOverviewCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeTrainingOverviewIntegration.test.ts`
