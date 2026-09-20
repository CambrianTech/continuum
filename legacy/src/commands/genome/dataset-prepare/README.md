# Genome Dataset Prepare Command

Collect training data from chat history for a persona and export as JSONL dataset for LoRA fine-tuning

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
./jtag genome/dataset-prepare --personaId=<value> --personaName=<value> --roomId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/dataset-prepare', {
  // your parameters here
});
```

## Parameters

- **personaId** (required): `UUID` - Persona to collect training data for
- **personaName** (required): `string` - Display name (used in dataset metadata and file naming)
- **roomId** (required): `UUID` - Room to collect conversation data from
- **traitType** (optional): `string` - Trait type label for the dataset (default: 'conversational')
- **minMessages** (optional): `number` - Minimum messages required to produce a dataset (default: 10)
- **maxMessages** (optional): `number` - Maximum messages to process (default: 500)

## Result

Returns `GenomeDatasetPrepareResult` with:

Returns CommandResult with:
- **datasetPath**: `string` - Absolute path to the generated JSONL file
- **exampleCount**: `number` - Number of training examples in the dataset
- **personaId**: `UUID` - Persona ID the dataset was built for
- **traitType**: `string` - Trait type label

## Examples

### Prepare dataset from general room

```bash
./jtag genome/dataset-prepare --personaId="<uuid>" --personaName="Helper AI" --roomId="<room-uuid>"
```

**Expected result:**
{ success: true, datasetPath: ".continuum/genome/datasets/helper-ai-conversational-1234.jsonl", exampleCount: 42 }

### Prepare with custom trait type and message limits

```bash
./jtag genome/dataset-prepare --personaId="<uuid>" --personaName="Teacher AI" --roomId="<room-uuid>" --traitType="teaching" --maxMessages=200
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/dataset-prepare
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/dataset-prepare'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/dataset-prepare
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/dataset-prepare'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Dataset Prepare/test/unit/GenomeDatasetPrepareCommand.test.ts
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
npx tsx commands/Genome Dataset Prepare/test/integration/GenomeDatasetPrepareIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeDatasetPrepareTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeDatasetPrepareBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeDatasetPrepareServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeDatasetPrepareCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeDatasetPrepareIntegration.test.ts`
