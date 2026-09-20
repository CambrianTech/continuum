# Code Shell Execute Command

Execute a shell command in the persona's workspace. Async mode (default) returns execution handle immediately — use code/shell/watch to stream output. Sync mode (wait=true) blocks until completion and returns full stdout/stderr.

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
./jtag code/shell/execute --cmd=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/shell/execute', {
  // your parameters here
});
```

## Parameters

- **cmd** (required): `string` - Shell command to execute (e.g., "npm run build", "cargo test", "xcodebuild")
- **wait** (optional): `boolean` - Wait for completion before returning (default: false — returns handle immediately)
- **timeoutMs** (optional): `number` - Timeout in milliseconds (default: 30000 for sync, no limit for async)

## Result

Returns `CodeShellExecuteResult` with:

Returns CommandResult with:
- **executionId**: `string` - Execution handle — use with code/shell/watch, code/shell/kill
- **status**: `string` - Execution status: running, completed, failed, timed_out, killed
- **stdout**: `string` - Full stdout (only present when wait=true and execution completed)
- **stderr**: `string` - Full stderr (only present when wait=true and execution completed)
- **exitCode**: `number` - Process exit code (only present when execution completed)

## Examples

### Run a build synchronously and wait for result

```bash
./jtag code/shell/execute --cmd="npm run build" --wait=true
```

**Expected result:**
{ executionId: "exec-abc123", status: "completed", stdout: "...", exitCode: 0 }

### Start an async build (returns handle for streaming)

```bash
./jtag code/shell/execute --cmd="cargo build --release"
```

**Expected result:**
{ executionId: "exec-def456", status: "running" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/shell/execute
```

**Tool:**
```typescript
// Use your help tool with command name 'code/shell/execute'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/shell/execute
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/shell/execute'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Shell Execute/test/unit/CodeShellExecuteCommand.test.ts
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
npx tsx commands/Code Shell Execute/test/integration/CodeShellExecuteIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeShellExecuteTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeShellExecuteBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeShellExecuteServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeShellExecuteCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeShellExecuteIntegration.test.ts`
