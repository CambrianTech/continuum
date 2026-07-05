# Plasticity Compact Command

Physically remove pruned heads from a model's safetensors. Reads gate_gradients.json from adapter directory, computes which heads to prune, then slices Q/K/V/O projection weights to remove dead heads. Produces a smaller model with fewer parameters. Handles both single-file and multi-shard models.

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
./jtag plasticity/compact --adapterPath=<value> --modelPath=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('plasticity/compact', {
  // your parameters here
});
```

## Parameters

- **adapterPath** (required): `string` - Path to adapter directory containing gate_gradients.json
- **modelPath** (required): `string` - Path to base model safetensors directory (single or multi-shard)
- **outputPath** (optional): `string` - Output path for compacted safetensors. Default: <adapterPath>/compacted/
- **config** (optional): `object` - Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold }

## Result

Returns `PlasticityCompactResult` with:

Returns CommandResult with:
- **modelPath**: `string` - Path to the compacted model safetensors
- **topologyPath**: `string` - Path to the head_topology.json file
- **originalSizeBytes**: `number` - Original model size in bytes
- **compactedSizeBytes**: `number` - Compacted model size in bytes

## Examples

### Compact a model using trained adapter's gate gradients

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help plasticity/compact
```

**Tool:**
```typescript
// Use your help tool with command name 'plasticity/compact'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme plasticity/compact
```

**Tool:**
```typescript
// Use your readme tool with command name 'plasticity/compact'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Plasticity Compact/test/unit/PlasticityCompactCommand.test.ts
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
npx tsx commands/Plasticity Compact/test/integration/PlasticityCompactIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PlasticityCompactTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PlasticityCompactBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PlasticityCompactServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PlasticityCompactCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PlasticityCompactIntegration.test.ts`
