# Genome Adapter List Command

List all LoRA adapters in the genome with sizes, domains, last-used timestamps, and cascade scores. Shows both on-disk (available) and loaded (active) adapters.

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
./jtag genome/adapter-list [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/adapter-list', {
  // your parameters here
});
```

## Parameters

- **personaId** (optional): `string` - Filter adapters for a specific persona. If omitted, lists all adapters across all personas.
- **domain** (optional): `string` - Filter adapters by skill domain (e.g., 'coding', 'reasoning')
- **sortBy** (optional): `string` - Sort field: 'name' (default), 'size', 'lastUsed', 'domain', 'created'
- **includeMetrics** (optional): `boolean` - Include cascade scores and training provenance (default: false)

## Result

Returns `GenomeAdapterListResult` with:

Returns CommandResult with:
- **adapters**: `array` - Array of adapter info objects
- **totalCount**: `number` - Total number of adapters found
- **totalSizeMB**: `number` - Total disk usage in megabytes
- **activeCount**: `number` - Number of adapters currently loaded in GPU memory

## Examples

### List all adapters

```bash
./jtag genome/adapter-list
```

**Expected result:**
{ adapters: [...], totalCount: 12, totalSizeMB: 4200, activeCount: 2 }

### List coding adapters for helper persona

```bash
./jtag genome/adapter-list --personaId=helper --domain=coding
```

**Expected result:**
{ adapters: [...], totalCount: 3, totalSizeMB: 900, activeCount: 1 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/adapter-list
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/adapter-list'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/adapter-list
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/adapter-list'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Adapter List/test/unit/GenomeAdapterListCommand.test.ts
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
npx tsx commands/Genome Adapter List/test/integration/GenomeAdapterListIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeAdapterListTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeAdapterListBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeAdapterListServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeAdapterListCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeAdapterListIntegration.test.ts`
