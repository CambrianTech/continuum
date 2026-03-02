# Genome Convert Command

Convert LoRA adapters between formats. Supports: merge LoRA into full-precision model, merge + quantize to GGUF, quantize base model to GGUF, and validate converted models. Uses convert-adapter.py via Rust sentinel for process isolation.

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
./jtag genome/convert --operation=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/convert', {
  // your parameters here
});
```

## Parameters

- **operation** (required): `string` - Conversion operation: 'merge-full' (LoRA → merged FP16), 'merge-and-quantize' (LoRA → merged GGUF), 'quantize-base' (HF → GGUF), 'validate' (sanity check)
- **adapterPath** (optional): `string` - Path to LoRA adapter directory (required for merge-full and merge-and-quantize)
- **baseModel** (optional): `string` - Base model name or HuggingFace ID (required for merge and quantize operations)
- **bits** (optional): `number` - Quantization bits: 4 or 8 (default: 4, only for quantize operations)
- **outputPath** (optional): `string` - Output directory. Default: sibling directory with format suffix
- **validate** (optional): `boolean` - Run validation inference after conversion (default: true)

## Result

Returns `GenomeConvertResult` with:

Returns CommandResult with:
- **outputPath**: `string` - Path to converted model/adapter
- **format**: `string` - Output format: 'safetensors-fp16', 'gguf-q4_0', 'gguf-q8_0'
- **sizeMB**: `number` - Output size in megabytes
- **durationSeconds**: `number` - Conversion duration in seconds
- **compressionRatio**: `number` - Original size / converted size (for quantize operations)
- **validation**: `object` - Validation result if --validate was run

## Examples

### Merge LoRA into full-precision model

```bash
./jtag genome/convert --operation=merge-full --adapterPath=.continuum/genome/adapters/helper-coding-123 --baseModel=unsloth/Llama-3.2-3B-Instruct
```

**Expected result:**
{ outputPath: '.continuum/genome/converted/...', format: 'safetensors-fp16', sizeMB: 6144 }

### Merge LoRA and quantize to 4-bit GGUF

```bash
./jtag genome/convert --operation=merge-and-quantize --adapterPath=.continuum/genome/adapters/helper-coding-123 --baseModel=unsloth/Llama-3.2-3B-Instruct --bits=4
```

**Expected result:**
{ outputPath: '.continuum/genome/converted/...', format: 'gguf-q4_0', sizeMB: 1800, compressionRatio: 3.4 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/convert
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/convert'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/convert
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/convert'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Convert/test/unit/GenomeConvertCommand.test.ts
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
npx tsx commands/Genome Convert/test/integration/GenomeConvertIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeConvertTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeConvertBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeConvertServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeConvertCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeConvertIntegration.test.ts`
