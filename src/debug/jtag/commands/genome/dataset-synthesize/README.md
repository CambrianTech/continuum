# Genome Dataset Synthesize Command

Uses an LLM to synthesize training data for a given topic/skill. Generates Q&A pairs in the persona's voice, saved as JSONL compatible with genome/train.

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
./jtag genome/dataset-synthesize --topic="TypeScript generics" --skill="typescript" --personaName="Helper AI"
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { GenomeDatasetSynthesize } from '@commands/genome/dataset-synthesize/shared/GenomeDatasetSynthesizeTypes';

const result = await GenomeDatasetSynthesize.execute({
  topic: 'TypeScript generic type parameters',
  skill: 'typescript',
  personaName: 'Helper AI',
  exampleCount: 20,
  difficulty: 'intermediate',
});
```

## Parameters

- **topic** (required): `string` - Topic to generate training data about
- **skill** (required): `string` - Parent skill domain (e.g., "typescript", "ethical-reasoning")
- **personaName** (required): `string` - Student persona name (for voice matching in generated data)
- **exampleCount** (optional): `number` - Number of training examples to generate (default: 20)
- **difficulty** (optional): `'beginner' | 'intermediate' | 'advanced'` - Difficulty level (default: 'intermediate')
- **model** (optional): `string` - LLM model for generation
- **provider** (optional): `string` - LLM provider for generation
- **outputPath** (optional): `string` - Override default output path

## Result

Returns `GenomeDatasetSynthesizeResult` with:

- **success**: `boolean` - Whether synthesis succeeded
- **datasetPath**: `string` - Absolute path to the generated JSONL file
- **exampleCount**: `number` - Number of training examples generated
- **topic**: `string` - Topic the data was generated for
- **generatedBy**: `string` - Model that generated the data
- **error**: `string` (optional) - Error message if failed

## Examples

### Basic synthesis

```bash
./jtag genome/dataset-synthesize --topic="TypeScript generics" --skill="typescript" --personaName="Helper AI"
```

**Expected result:**
```json
{ "success": true, "datasetPath": "/path/to/.continuum/genome/datasets/synth-typescript-generics-1234.jsonl", "exampleCount": 20, "generatedBy": "deepseek-chat" }
```

### Advanced with specific model

```bash
./jtag genome/dataset-synthesize --topic="Async/await patterns" --skill="typescript" --personaName="Code Tutor" --exampleCount=50 --difficulty="advanced" --provider="anthropic"
```

### Feed into training pipeline

```bash
# 1. Synthesize data
./jtag genome/dataset-synthesize --topic="React hooks" --skill="react" --personaName="Helper AI"

# 2. Train on synthesized data
./jtag genome/train --personaId="<uuid>" --personaName="Helper AI" --datasetPath="/path/from/step1.jsonl" --baseModel="smollm2:135m"
```

## Getting Help

### Using the Help Tool

```bash
./jtag help genome/dataset-synthesize
```

### Using the README Tool

```bash
./jtag readme genome/dataset-synthesize
```

## Testing

### Unit Tests

```bash
npx vitest run tests/unit/semantic-cognition.test.ts
```

### Integration Tests

```bash
# Prerequisites: Server must be running + LLM available
npm start  # Wait 90+ seconds for deployment

npx vitest run tests/integration/sentinel-lora-training.test.ts
```

## Access Level

**ai-safe** - Safe for AI personas to call autonomously. Used by the Teacher Sentinel in Academy Dojo sessions to generate curriculum-specific training data.

## Implementation Notes

- **Shared Logic**: Types and factories in `shared/GenomeDatasetSynthesizeTypes.ts`
- **Browser**: Delegates to server in `browser/GenomeDatasetSynthesizeBrowserCommand.ts`
- **Server**: LLM synthesis logic in `server/GenomeDatasetSynthesizeServerCommand.ts`
- Output path: `.continuum/genome/datasets/synth-{topic}-{timestamp}.jsonl`
- JSONL format matches `genome/train` expectations (messages array per line)
- Used by `TeacherPipeline` in Academy Dojo for automated data generation
