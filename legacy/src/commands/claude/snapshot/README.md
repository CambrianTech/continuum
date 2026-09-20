# Claude Snapshot Command

Saves a work-state snapshot for session continuity. Captures what Claude was doing, what's pending, and what comes next — so the next Claude instance can resume without reading 200 lines of MEMORY.md and guessing.

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
./jtag claude/snapshot --summary=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('claude/snapshot', {
  // your parameters here
});
```

## Parameters

- **summary** (required): `string` - What was being worked on — the current task and approach
- **pendingWork** (optional): `string` - What's unfinished — branches, uncommitted code, failing tests
- **nextSteps** (optional): `string` - What should happen next — the plan for the next session
- **decisions** (optional): `string` - Key decisions made this session and why — so the next instance doesn't relitigate them
- **issuesWorked** (optional): `string` - Comma-separated issue numbers touched this session (e.g. '376,335,317')

## Result

Returns `ClaudeSnapshotResult` with:

Returns CommandResult with:
- **snapshotId**: `string` - Unique ID for this snapshot
- **filePath**: `string` - Where the snapshot was saved
- **timestamp**: `string` - When the snapshot was taken

## Examples

### Save end-of-session snapshot

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help claude/snapshot
```

**Tool:**
```typescript
// Use your help tool with command name 'claude/snapshot'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme claude/snapshot
```

**Tool:**
```typescript
// Use your readme tool with command name 'claude/snapshot'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Claude Snapshot/test/unit/ClaudeSnapshotCommand.test.ts
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
npx tsx commands/Claude Snapshot/test/integration/ClaudeSnapshotIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ClaudeSnapshotTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ClaudeSnapshotBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ClaudeSnapshotServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ClaudeSnapshotCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ClaudeSnapshotIntegration.test.ts`
