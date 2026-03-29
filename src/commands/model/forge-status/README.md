# Model Forge Status Command

Get the current status of active model forges — phase, step, loss, VRAM usage, ETA. Polls status.json from forge nodes on the grid.

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
./jtag model/forge-status [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/forge-status', {
  // your parameters here
});
```

## Parameters

- **nodeId** (optional): `string` - Optional grid node ID to query. If omitted, returns status from all nodes.

## Result

Returns `ModelForgeStatusResult` with:

Returns CommandResult with:
- **forges**: `array` - List of active forge jobs

## Examples

### Check active forges across all nodes

```bash
./jtag model/forge-status
```

**Expected result:**
{ forges: [{ nodeName: "bigmama", phase: "training", step: 350, totalSteps: 1000, loss: 2.65, vramGb: 7.0 }] }

### Check specific node

```bash
./jtag model/forge-status --nodeId=bigmama
```

**Expected result:**
{ forges: [{ nodeName: "bigmama", phase: "training", model: "Qwen/Qwen3.5-27B", domain: "code" }] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/forge-status
```

**Tool:**
```typescript
// Use your help tool with command name 'model/forge-status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/forge-status
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/forge-status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model Forge Status/test/unit/ModelForgeStatusCommand.test.ts
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
npx tsx commands/Model Forge Status/test/integration/ModelForgeStatusIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelForgeStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelForgeStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelForgeStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelForgeStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelForgeStatusIntegration.test.ts`
