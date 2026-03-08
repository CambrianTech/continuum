# Genome Adapter Info Command

Get detailed information about a specific LoRA adapter including full manifest, training provenance, architecture details, and compatibility status.

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
./jtag genome/adapter-info --name=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/adapter-info', {
  // your parameters here
});
```

## Parameters

- **name** (required): `string` - Adapter name or path to inspect

## Result

Returns `GenomeAdapterInfoResult` with:

Returns CommandResult with:
- **name**: `string` - Adapter name
- **domain**: `string` - Skill domain this adapter was trained for
- **sizeMB**: `number` - Adapter size on disk in megabytes
- **baseModel**: `string` - Base model this adapter was trained on
- **createdAt**: `string` - ISO timestamp of when adapter was trained
- **lastUsedAt**: `string` - ISO timestamp of last inference use
- **trainingInfo**: `object` - Training provenance: dataset, epochs, loss, examples count
- **architecture**: `object` - LoRA architecture: rank, alpha, target modules, layer count
- **compatibility**: `object` - Model compatibility status: compatible models, quantization format
- **isActive**: `boolean` - Whether this adapter is currently loaded in GPU memory

## Examples

### Inspect a specific adapter

```bash
./jtag genome/adapter-info --name=helper-coding-2026-03-01
```

**Expected result:**
{ name: 'helper-coding-2026-03-01', domain: 'coding', sizeMB: 312, baseModel: 'unsloth/Llama-3.2-3B-Instruct', ... }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/adapter-info
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/adapter-info'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/adapter-info
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/adapter-info'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Adapter Info/test/unit/GenomeAdapterInfoCommand.test.ts
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
npx tsx commands/Genome Adapter Info/test/integration/GenomeAdapterInfoIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeAdapterInfoTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeAdapterInfoBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeAdapterInfoServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeAdapterInfoCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeAdapterInfoIntegration.test.ts`
