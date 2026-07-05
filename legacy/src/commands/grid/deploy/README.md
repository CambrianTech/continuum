# Grid Deploy Command

Pull latest code and rebuild on grid nodes. Runs git pull + npm run build:ts on each reachable node via SSH over Tailscale. Keeps all nodes in sync without manual SSH.

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
./jtag grid/deploy [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('grid/deploy', {
  // your parameters here
});
```

## Parameters

- **nodes** (optional): `string` - Comma-separated node names or IPs to deploy to. Default: all known grid nodes.
- **branch** (optional): `string` - Git branch to checkout. Default: current branch on each node.
- **skipBuild** (optional): `boolean` - Skip npm run build:ts after pull (just update code). Default: false.
- **restart** (optional): `boolean` - Restart the system (npm stop + npm start) after build. Default: false.

## Result

Returns `GridDeployResult` with:

Returns CommandResult with:
- **deployedNodes**: `object` - Array of { nodeId, status, branch, buildSuccess, error? } per node
- **totalDeployed**: `number` - Number of nodes successfully deployed

## Examples

### Deploy to all grid nodes

```bash
undefined
```

### Deploy specific branch to the 5090 tower

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help grid/deploy
```

**Tool:**
```typescript
// Use your help tool with command name 'grid/deploy'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme grid/deploy
```

**Tool:**
```typescript
// Use your readme tool with command name 'grid/deploy'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Grid Deploy/test/unit/GridDeployCommand.test.ts
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
npx tsx commands/Grid Deploy/test/integration/GridDeployIntegration.test.ts
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

**owner** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/GridDeployTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GridDeployBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GridDeployServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GridDeployCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GridDeployIntegration.test.ts`
