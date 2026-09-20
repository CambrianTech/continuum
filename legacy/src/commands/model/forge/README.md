# Model Forge Command

Start a model forge job — sends forge parameters to a grid node with GPU for training. Returns job ID for status tracking.

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
./jtag model/forge --model=<value> --domain=<value> --steps=<value> --pruneLevel=<value> --pruneStrategy=<value> --cycles=<value> --learningRate=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/forge', {
  // your parameters here
});
```

## Parameters

- **model** (required): `string` - Base model to forge (e.g., 'Qwen/Qwen3.5-4B')
- **domain** (required): `string` - Training domain (code, reasoning, general)
- **steps** (required): `number` - Number of training steps
- **pruneLevel** (required): `number` - Pruning level 0.0-0.7 (fraction of heads to prune)
- **pruneStrategy** (required): `string` - Pruning strategy: entropy, random, magnitude
- **cycles** (required): `number` - Number of prune→recover forge cycles
- **learningRate** (required): `string` - Learning rate (e.g., '2e-4', '5e-5')
- **experts** (optional): `number` - Number of MoE experts to keep (0 for non-MoE models)
- **nodeId** (optional): `string` - Target grid node. If omitted, routes to first available GPU node.

## Result

Returns `ModelForgeResult` with:

Returns CommandResult with:
- **jobId**: `string` - Unique forge job ID for status tracking
- **nodeId**: `string` - Grid node the job was routed to
- **nodeName**: `string` - Human-readable node name
- **estimatedDuration**: `string` - Estimated duration based on model size and steps

## Examples

### Start a code forge on Qwen3.5-4B

```bash
./jtag model/forge --model=Qwen/Qwen3.5-4B --domain=code --steps=2000 --pruneLevel=0.3 --pruneStrategy=entropy --cycles=3 --learningRate=2e-4
```

**Expected result:**
{ jobId: 'forge-abc123', nodeId: 'bigmama', nodeName: 'BigMama', estimatedDuration: '~45 minutes' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/forge
```

**Tool:**
```typescript
// Use your help tool with command name 'model/forge'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/forge
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/forge'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model Forge/test/unit/ModelForgeCommand.test.ts
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
npx tsx commands/Model Forge/test/integration/ModelForgeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelForgeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelForgeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelForgeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelForgeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelForgeIntegration.test.ts`
