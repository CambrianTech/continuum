# Genome Adapter Prune Command

Prune unused LoRA adapters to reclaim disk space. Removes adapters not used since the specified cutoff date. Supports dry-run mode to preview what would be deleted.

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
./jtag genome/adapter-prune [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/adapter-prune', {
  // your parameters here
});
```

## Parameters

- **unusedSince** (optional): `string` - Prune adapters unused since this duration (e.g., '30d', '7d', '90d'). Default: '30d'
- **dryRun** (optional): `boolean` - Preview what would be pruned without actually deleting (default: true for safety)
- **personaId** (optional): `string` - Only prune adapters for a specific persona
- **domain** (optional): `string` - Only prune adapters for a specific domain
- **keepLatest** (optional): `number` - Always keep the N most recent adapters per domain regardless of age (default: 1)

## Result

Returns `GenomeAdapterPruneResult` with:

Returns CommandResult with:
- **prunedCount**: `number` - Number of adapters pruned (or would be pruned in dry-run)
- **reclaimedMB**: `number` - Disk space reclaimed (or would be reclaimed) in megabytes
- **prunedAdapters**: `array` - List of pruned adapter names with sizes
- **keptCount**: `number` - Number of adapters kept
- **isDryRun**: `boolean` - Whether this was a dry-run (no actual deletion)

## Examples

### Preview pruning adapters unused for 30 days

```bash
./jtag genome/adapter-prune --unusedSince=30d --dryRun=true
```

**Expected result:**
{ prunedCount: 5, reclaimedMB: 1500, isDryRun: true, ... }

### Actually prune old adapters

```bash
./jtag genome/adapter-prune --unusedSince=30d --dryRun=false
```

**Expected result:**
{ prunedCount: 5, reclaimedMB: 1500, isDryRun: false, ... }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/adapter-prune
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/adapter-prune'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/adapter-prune
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/adapter-prune'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Adapter Prune/test/unit/GenomeAdapterPruneCommand.test.ts
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
npx tsx commands/Genome Adapter Prune/test/integration/GenomeAdapterPruneIntegration.test.ts
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

**admin** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/GenomeAdapterPruneTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeAdapterPruneBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeAdapterPruneServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeAdapterPruneCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeAdapterPruneIntegration.test.ts`
