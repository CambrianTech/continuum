# Git Commit Command

Commit changes in git workspace with persona identity

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
./jtag git/commit --message=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('git/commit', {
  // your parameters here
});
```

## Parameters

- **message** (required): `string` - Commit message
- **workspacePath** (optional): `string` - Path to workspace (auto-detected from context if not provided)
- **files** (optional): `array` - Specific files to commit (defaults to all changed files)

## Result

Returns `GitCommitResult` with:

Returns CommandResult with:
- **commitHash**: `string` - Git commit hash
- **shortHash**: `string` - Short commit hash (7 chars)
- **filesCommitted**: `number` - Number of files committed

## Examples

### Commit all changes

```bash
./jtag git/commit --message="Updated technical design section"
```

**Expected result:**
{ success: true, commitHash: "abc123...", filesCommitted: 3 }

### Commit specific files

```bash
./jtag git/commit --message="Fix typos" --files='["docs/readme.md"]'
```

**Expected result:**
{ success: true, commitHash: "def456...", filesCommitted: 1 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help git/commit
```

**Tool:**
```typescript
// Use your help tool with command name 'git/commit'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme git/commit
```

**Tool:**
```typescript
// Use your readme tool with command name 'git/commit'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Git Commit/test/unit/GitCommitCommand.test.ts
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
npx tsx commands/Git Commit/test/integration/GitCommitIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GitCommitTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GitCommitBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GitCommitServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GitCommitCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GitCommitIntegration.test.ts`
