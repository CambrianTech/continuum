# Claude Context Command

Generates a comprehensive context summary for Claude Code session resumption — recent git changes, open issues, team chat, system health, and active work state. This is Claude's bridge from stateless sessions to persistent citizenship.

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
./jtag claude/context [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('claude/context', {
  // your parameters here
});
```

## Parameters

- **includeGit** (optional): `boolean` - Include recent git log and uncommitted changes (default: true)
- **includeIssues** (optional): `boolean` - Include open GitHub issues summary (default: true)
- **includeChat** (optional): `boolean` - Include recent team chat messages (default: true)
- **includeHealth** (optional): `boolean` - Include system health status (default: true)
- **chatLimit** (optional): `number` - Number of recent chat messages to include (default: 20)
- **gitLimit** (optional): `number` - Number of recent git commits to include (default: 10)
- **issueLimit** (optional): `number` - Number of open issues to include (default: 20)

## Result

Returns `ClaudeContextResult` with:

Returns CommandResult with:
- **git**: `object` - Git state: branch, recent commits, uncommitted changes
- **issues**: `object` - Open issues grouped by phase from gap analysis
- **chat**: `object` - Recent team chat messages and active discussions
- **health**: `object` - System health: server status, browser connection, active personas
- **summary**: `string` - Human-readable summary of current state for session resumption

## Examples

### Full context dump for new session

```bash
undefined
```

### Quick status check

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help claude/context
```

**Tool:**
```typescript
// Use your help tool with command name 'claude/context'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme claude/context
```

**Tool:**
```typescript
// Use your readme tool with command name 'claude/context'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Claude Context/test/unit/ClaudeContextCommand.test.ts
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
npx tsx commands/Claude Context/test/integration/ClaudeContextIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ClaudeContextTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ClaudeContextBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ClaudeContextServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ClaudeContextCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ClaudeContextIntegration.test.ts`
