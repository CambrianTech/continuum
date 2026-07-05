# Code Shell Kill Command

Kill a running shell execution. Use the executionId returned by code/shell/execute to identify the target.

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
./jtag code/shell/kill --executionId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/shell/kill', {
  // your parameters here
});
```

## Parameters

- **executionId** (required): `string` - Execution handle to kill (from code/shell/execute)

## Result

Returns `CodeShellKillResult` with:

Returns CommandResult with:
- **executionId**: `string` - Echo of the killed execution handle
- **killed**: `boolean` - Whether the execution was successfully killed

## Examples

### Kill a running build

```bash
./jtag code/shell/kill --executionId="exec-abc123"
```

**Expected result:**
{ executionId: "exec-abc123", killed: true }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/shell/kill
```

**Tool:**
```typescript
// Use your help tool with command name 'code/shell/kill'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/shell/kill
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/shell/kill'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Shell Kill/test/unit/CodeShellKillCommand.test.ts
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
npx tsx commands/Code Shell Kill/test/integration/CodeShellKillIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeShellKillTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeShellKillBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeShellKillServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeShellKillCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeShellKillIntegration.test.ts`
