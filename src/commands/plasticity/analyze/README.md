# Plasticity Analyze Command

Dry-run analysis of what compaction would do to a model. Reads gate_gradients.json from the adapter directory, computes per-head utilization scores, and returns a topology showing which heads would be pruned/compressed/kept. Does NOT modify any files.

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
./jtag plasticity/analyze --adapterPath=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('plasticity/analyze', {
  // your parameters here
});
```

## Parameters

- **adapterPath** (required): `string` - Path to adapter directory containing gate_gradients.json (output from training with GateGradientCallback)
- **config** (optional): `object` - Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization }

## Result

Returns `PlasticityAnalyzeResult` with:

Returns CommandResult with:
- **topology**: `object` - HeadTopology: per-layer head precision assignments (prune/Q2/Q4/Q8/BF16)
- **layerSummaries**: `object` - Per-layer summary: head counts by precision tier, parameter reduction %
- **estimatedSavingsBytes**: `number` - Estimated memory savings in bytes from compaction
- **saturatedHeads**: `object` - Heads with utilization > saturatedThreshold that may benefit from higher rank LoRA

## Examples

### Analyze a trained adapter's gate gradients

```bash
undefined
```

### Analyze with custom thresholds

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help plasticity/analyze
```

**Tool:**
```typescript
// Use your help tool with command name 'plasticity/analyze'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme plasticity/analyze
```

**Tool:**
```typescript
// Use your readme tool with command name 'plasticity/analyze'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Plasticity Analyze/test/unit/PlasticityAnalyzeCommand.test.ts
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
npx tsx commands/Plasticity Analyze/test/integration/PlasticityAnalyzeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PlasticityAnalyzeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PlasticityAnalyzeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PlasticityAnalyzeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PlasticityAnalyzeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PlasticityAnalyzeIntegration.test.ts`
