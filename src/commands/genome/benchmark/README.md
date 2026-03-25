# Genome Benchmark Command

Run standard and Continuum-specific benchmarks against a model or adapter. Stores results in BenchmarkResultEntity and embeds in adapter manifest for model card publishing. Supports HumanEval, MBPP, RealClassEval, and collaborative team benchmarks.

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
./jtag genome/benchmark --suite=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/benchmark', {
  // your parameters here
});
```

## Parameters

- **model** (optional): `string` - Model path or HuggingFace ID to benchmark. Uses LOCAL_MODELS.DEFAULT if not specified.
- **adapter** (optional): `string` - Path to LoRA adapter directory to apply on top of base model
- **suite** (required): `string` - Comma-separated benchmark suites: humaneval, mbpp, realclasseval, mmlu, collaborative
- **teamProjectId** (optional): `string` - Team project ID for collaborative benchmark (computes from existing session data)
- **academySessionId** (optional): `string` - Academy session ID for phenotype benchmark (extracts before/after scores)
- **output** (optional): `string` - Path to write benchmark results JSON. Default: stdout
- **limit** (optional): `number` - Max problems to run per suite (for quick testing). Default: all

## Result

Returns `GenomeBenchmarkResult` with:

Returns CommandResult with:
- **suites**: `object` - Per-suite results: { humaneval: { score, total, passed, ... }, mbpp: { ... } }
- **overallScore**: `number` - Weighted average across all suites (0-100)
- **benchmarkId**: `string` - ID of the stored BenchmarkResultEntity

## Examples

### Run HumanEval on a compacted model

```bash
undefined
```

### Benchmark a trained adapter

```bash
undefined
```

### Extract collaborative benchmark from team project

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/benchmark
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/benchmark'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/benchmark
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/benchmark'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Benchmark/test/unit/GenomeBenchmarkCommand.test.ts
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
npx tsx commands/Genome Benchmark/test/integration/GenomeBenchmarkIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeBenchmarkTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeBenchmarkBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeBenchmarkServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeBenchmarkCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeBenchmarkIntegration.test.ts`
