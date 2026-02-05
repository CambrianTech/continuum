# Workspace List Command

List all persona workspaces across the team — worktree paths, git branches, modified files, shell activity. Scans both in-memory active workspaces and persisted git worktrees on disk.

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
./jtag workspace/list [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('workspace/list', {
  // your parameters here
});
```

## Parameters

- **personaId** (optional): `string` - Filter to a specific persona's workspaces. If omitted, returns all.
- **includeGitStatus** (optional): `boolean` - Include git status (branch, modified files, staged) for each workspace. Defaults to true.

## Result

Returns `WorkspaceListResult` with:

Returns CommandResult with:
- **workspaces**: `WorkspaceInfo[]` - Array of workspace info objects for each discovered workspace
- **totalCount**: `number` - Total number of workspaces found
- **activeCount**: `number` - Number of workspaces currently active in memory (server session)

## Examples

### List all persona workspaces

```bash
./jtag workspace/list
```

**Expected result:**
{ workspaces: [...], totalCount: 3, activeCount: 1 }

### List workspaces for a specific persona

```bash
./jtag workspace/list --personaId="deepseek"
```

**Expected result:**
{ workspaces: [{ personaId: 'deepseek', branch: 'ai/deepseek-assistant/default', ... }] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help workspace/list
```

**Tool:**
```typescript
// Use your help tool with command name 'workspace/list'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme workspace/list
```

**Tool:**
```typescript
// Use your readme tool with command name 'workspace/list'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Workspace List/test/unit/WorkspaceListCommand.test.ts
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
npx tsx commands/Workspace List/test/integration/WorkspaceListIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/WorkspaceListTypes.ts`
- **Browser**: Browser-specific implementation in `browser/WorkspaceListBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/WorkspaceListServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/WorkspaceListCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/WorkspaceListIntegration.test.ts`
