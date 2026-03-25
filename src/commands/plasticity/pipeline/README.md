# Plasticity Pipeline Command

End-to-end plasticity pipeline: gate_gradients.json → analysis → compaction. The 'wake up to a compacted model' command. Given a gate capture directory and a model path, runs the full pipeline: load gradients, compute optimization plan, build topology, compact model (multi-shard aware), write compacted model + topology + analysis.

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
./jtag plasticity/pipeline --capturePath=<value> --modelPath=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('plasticity/pipeline', {
  // your parameters here
});
```

## Parameters

- **capturePath** (required): `string` - Gate capture directory containing gate_gradients.json (from PEFT training with GateGradientCallback)
- **modelPath** (required): `string` - Base model path — directory for multi-shard, file for single safetensors
- **outputPath** (optional): `string` - Output directory for compacted model. Default: <capturePath>/compacted/
- **config** (optional): `object` - CompactionConfig overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization, targetSizeGb }

## Result

Returns `PlasticityPipelineResult` with:

Returns CommandResult with:
- **modelPath**: `string` - Path to the compacted model safetensors
- **topologyPath**: `string` - Path to the head_topology.json file
- **originalSizeBytes**: `number` - Original model size in bytes
- **compactedSizeBytes**: `number` - Compacted model size in bytes

## Examples

### Full pipeline: train → gate gradients → compact

```bash
undefined
```

### Compact targeting 16GB MacBook Air

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help plasticity/pipeline
```

**Tool:**
```typescript
// Use your help tool with command name 'plasticity/pipeline'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme plasticity/pipeline
```

**Tool:**
```typescript
// Use your readme tool with command name 'plasticity/pipeline'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Plasticity Pipeline/test/unit/PlasticityPipelineCommand.test.ts
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
npx tsx commands/Plasticity Pipeline/test/integration/PlasticityPipelineIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PlasticityPipelineTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PlasticityPipelineBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PlasticityPipelineServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PlasticityPipelineCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PlasticityPipelineIntegration.test.ts`
