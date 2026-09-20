# Sentinel Approve Command

Approve or reject a pending pipeline approval step. Resolves the blocking approval gate in the Rust executor.

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
./jtag sentinel/approve --handle=<value> --approved=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('sentinel/approve', {
  // your parameters here
});
```

## Parameters

- **handle** (required): `string` - Sentinel handle ID with pending approval
- **approved** (required): `boolean` - Whether to approve (true) or reject (false) the pending step
- **reason** (optional): `string` - Human-readable reason for the approval decision
- **approverId** (optional): `string` - UUID of the user or persona approving/rejecting

## Result

Returns `SentinelApproveResult` with:

Returns CommandResult with:
- **handle**: `string` - The sentinel handle that was approved/rejected
- **approved**: `boolean` - Whether the approval was granted

## Examples

### Approve a pending pipeline step

```bash
./jtag sentinel/approve --handle=abc123 --approved=true --reason='Looks good'
```

**Expected result:**
{ handle: 'abc123', approved: true }

### Reject a pending pipeline step

```bash
./jtag sentinel/approve --handle=abc123 --approved=false --reason='Needs more tests'
```

**Expected result:**
{ handle: 'abc123', approved: false }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help sentinel/approve
```

**Tool:**
```typescript
// Use your help tool with command name 'sentinel/approve'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme sentinel/approve
```

**Tool:**
```typescript
// Use your readme tool with command name 'sentinel/approve'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Sentinel Approve/test/unit/SentinelApproveCommand.test.ts
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
npx tsx commands/Sentinel Approve/test/integration/SentinelApproveIntegration.test.ts
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

**system** - System-level command, requires elevated permissions

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/SentinelApproveTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SentinelApproveBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SentinelApproveServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SentinelApproveCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SentinelApproveIntegration.test.ts`
