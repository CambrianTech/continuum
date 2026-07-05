# Model Download Command

Download a base model from HuggingFace to a local or remote grid node. Routes to GPU-capable node if needed. Wraps huggingface_hub snapshot_download with progress reporting via chat.

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
./jtag model/download --modelId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/download', {
  // your parameters here
});
```

## Parameters

- **modelId** (required): `string` - HuggingFace model ID (e.g., 'Qwen/Qwen3.5-27B')
- **node** (optional): `string` - Target grid node IP or name. Default: local machine, or GPU node if model requires GPU.
- **revision** (optional): `string` - Specific revision/branch/tag to download. Default: main.

## Result

Returns `ModelDownloadResult` with:

Returns CommandResult with:
- **downloadPath**: `string` - Local path where the model was downloaded
- **sizeGb**: `number` - Total download size in GB
- **nodeId**: `string` - Which grid node the model was downloaded to

## Examples

### Download Qwen 3.5 27B to the 5090 tower

```bash
undefined
```

### Download a model locally

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/download
```

**Tool:**
```typescript
// Use your help tool with command name 'model/download'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/download
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/download'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model Download/test/unit/ModelDownloadCommand.test.ts
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
npx tsx commands/Model Download/test/integration/ModelDownloadIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelDownloadTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelDownloadBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelDownloadServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelDownloadCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelDownloadIntegration.test.ts`
