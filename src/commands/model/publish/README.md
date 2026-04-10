# Model Publish Command

Publish a forged model to HuggingFace — safetensors, config, tokenizer, model card, and alloy provenance. This is the Factory's shipping department: the forge produces the artifact on a grid node, this command pushes it to HuggingFace where anyone can download it. Supports publishing from a local forged directory (bigmama-style) or from a grid node's finished/ station via grid/send.

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
./jtag model/publish --forgedDir=<value> --repoName=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/publish', {
  // your parameters here
});
```

## Parameters

- **forgedDir** (required): `string` - Path to the forged model directory containing safetensors, config.json, tokenizer files, and optionally the .alloy.json recipe
- **repoName** (required): `string` - HuggingFace repo name (e.g., 'mixtral-8x7b-instruct-compacted-conservative'). Published under the org.
- **org** (optional): `string` - HuggingFace organization (default: 'continuum-ai')
- **cardPath** (optional): `string` - Path to a custom README.md model card. If omitted, a card is auto-generated from the alloy's results + metadata.
- **alloyPath** (optional): `string` - Path to the .alloy.json recipe file. If omitted, searches forgedDir for *.alloy.json. Included in the published repo as provenance.
- **includeGguf** (optional): `boolean` - Include GGUF quantized files if present in the forged dir (default: true)
- **private** (optional): `boolean` - Publish as private repo (default: false)
- **evalPending** (optional): `boolean` - Mark the model card as 'eval in progress' with placeholder benchmark fields. Use when publishing before eval completes. Card will be updated via model/update-card when eval finishes.
- **nodeId** (optional): `string` - If forgedDir is on a remote grid node, specify the node ID. The command will use grid/send to execute the publish on the remote node.
- **tags** (optional): `string[]` - Additional HuggingFace tags beyond the auto-generated ones

## Result

Returns `ModelPublishResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the publish succeeded
- **repoUrl**: `string` - Full HuggingFace repo URL (e.g., 'https://huggingface.co/continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
- **repoId**: `string` - HuggingFace repo ID (e.g., 'continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
- **filesUploaded**: `number` - Number of files uploaded
- **totalSizeGb**: `number` - Total size of uploaded files in GB
- **cardIncluded**: `boolean` - Whether a model card was included
- **alloyIncluded**: `boolean` - Whether the alloy recipe was included

## Examples

### Publish a locally forged Mixtral model

```bash
undefined
```

### Publish from a remote grid node

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/publish
```

**Tool:**
```typescript
// Use your help tool with command name 'model/publish'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/publish
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/publish'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model Publish/test/unit/ModelPublishCommand.test.ts
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
npx tsx commands/Model Publish/test/integration/ModelPublishIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelPublishTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelPublishBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelPublishServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelPublishCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelPublishIntegration.test.ts`
