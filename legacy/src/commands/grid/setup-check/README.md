# Grid Setup Check Command

Diagnose grid setup: Tailscale install, connectivity, HTTPS certs, peers, Docker grid profile, and actionable fix steps. Run this to see what's needed before enabling distributed compute.

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
./jtag grid/setup-check 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('grid/setup-check', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `GridSetupCheckResult` with:

Returns CommandResult with:
- **ready**: `boolean` - True if grid is fully operational (Tailscale connected, transport listening)
- **tailscaleIp**: `string` - This node's Tailscale IP (null if not connected)
- **dnsName**: `string` - This node's Tailscale DNS name (e.g., bigmama.tailnet-name.ts.net)
- **peerCount**: `number` - Number of online Tailscale peers
- **checks**: `array` - Diagnostic checks: { check, status (pass|fail|warn|info|skip), detail, peers? }
- **actions**: `array` - Ordered list of actionable fix steps (empty if everything passes)
- **summary**: `string` - Human-readable summary of grid readiness

## Examples

### Check if grid is ready

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help grid/setup-check
```

**Tool:**
```typescript
// Use your help tool with command name 'grid/setup-check'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme grid/setup-check
```

**Tool:**
```typescript
// Use your readme tool with command name 'grid/setup-check'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Grid Setup Check/test/unit/GridSetupCheckCommand.test.ts
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
npx tsx commands/Grid Setup Check/test/integration/GridSetupCheckIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GridSetupCheckTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GridSetupCheckBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GridSetupCheckServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GridSetupCheckCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GridSetupCheckIntegration.test.ts`
