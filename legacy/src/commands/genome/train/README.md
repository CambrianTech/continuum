# Genome Train Command

Execute LoRA fine-tuning on a JSONL dataset using PEFTLoRAAdapter. Wraps trainLoRA() as a command for Sentinel pipeline orchestration

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
./jtag genome/train --personaId=<value> --personaName=<value> --traitType=<value> --datasetPath=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/train', {
  // your parameters here
});
```

## Parameters

- **personaId** (required): `UUID` - Persona to train adapter for
- **personaName** (required): `string` - Display name (used in adapter naming)
- **traitType** (required): `string` - Trait type label for the adapter
- **datasetPath** (required): `string` - Path to JSONL training dataset file
- **baseModel** (optional): `string` - Base model to fine-tune (default: 'smollm2:135m')
- **rank** (optional): `number` - LoRA rank (default: 32)
- **epochs** (optional): `number` - Number of training epochs (default: 3)
- **learningRate** (optional): `number` - Learning rate (default: 0.0001)
- **batchSize** (optional): `number` - Batch size (default: 4)

## Result

Returns `GenomeTrainResult` with:

Returns CommandResult with:
- **adapterPath**: `string` - Path to the trained adapter files
- **metrics**: `object` - Training metrics: finalLoss, trainingTime, examplesProcessed, epochs

## Examples

### Train with defaults

```bash
./jtag genome/train --personaId="<uuid>" --personaName="Helper AI" --traitType="conversational" --datasetPath=".continuum/genome/datasets/helper-ai-conversational-1234.jsonl"
```

**Expected result:**
{ success: true, adapterPath: ".continuum/genome/adapters/helper-ai-conversational-1234/", metrics: { finalLoss: 0.42 } }

### Train with custom hyperparameters

```bash
./jtag genome/train --personaId="<uuid>" --personaName="Helper AI" --traitType="conversational" --datasetPath="<path>" --baseModel="smollm2:135m" --rank=16 --epochs=5 --learningRate=0.00005
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/train
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/train'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/train
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/train'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Train/test/unit/GenomeTrainCommand.test.ts
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
npx tsx commands/Genome Train/test/integration/GenomeTrainIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeTrainTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeTrainBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeTrainServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeTrainCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeTrainIntegration.test.ts`
