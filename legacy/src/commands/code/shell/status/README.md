# Code Shell Status Command

Get shell session info for the persona's workspace — current working directory, active and total execution count. No parameters required (userId auto-injected).

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
./jtag code/shell/status 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/shell/status', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `CodeShellStatusResult` with:

Returns CommandResult with:
- **sessionId**: `string` - Shell session identifier
- **personaId**: `string` - Persona that owns this shell session
- **cwd**: `string` - Current working directory of the shell session
- **workspaceRoot**: `string` - Root directory of the workspace
- **activeExecutions**: `number` - Number of currently running executions
- **totalExecutions**: `number` - Total number of executions (running + completed)

## Examples

### Check shell session status

```bash
./jtag code/shell/status
```

**Expected result:**
{ sessionId: "sess-abc", cwd: "/workspace/game", activeExecutions: 1, totalExecutions: 5 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/shell/status
```

**Tool:**
```typescript
// Use your help tool with command name 'code/shell/status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/shell/status
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/shell/status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Shell Status/test/unit/CodeShellStatusCommand.test.ts
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
npx tsx commands/Code Shell Status/test/integration/CodeShellStatusIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeShellStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeShellStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeShellStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeShellStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeShellStatusIntegration.test.ts`
