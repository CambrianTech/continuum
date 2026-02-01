# Code Tree Command

Generate a directory tree for the workspace or a subdirectory. Shows file/directory structure with sizes. Skips common ignored directories (node_modules, .git, etc).

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
./jtag code/tree [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/tree', {
  // your parameters here
});
```

## Parameters

- **path** (optional): `string` - Subdirectory to tree (default: workspace root)
- **maxDepth** (optional): `number` - Maximum directory depth (default: 10)
- **includeHidden** (optional): `boolean` - Include hidden files and directories (default: false)

## Result

Returns `CodeTreeResult` with:

Returns CommandResult with:
- **root**: `object` - TreeNode with name, path, isDirectory, sizeBytes, and children array
- **totalFiles**: `number` - Total number of files in tree
- **totalDirectories**: `number` - Total number of directories in tree

## Examples

### Show full workspace tree

```bash
./jtag code/tree
```

### Show src directory, 3 levels deep

```bash
./jtag code/tree --path="src" --maxDepth=3
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/tree
```

**Tool:**
```typescript
// Use your help tool with command name 'code/tree'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/tree
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/tree'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Tree/test/unit/CodeTreeCommand.test.ts
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
npx tsx commands/Code Tree/test/integration/CodeTreeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeTreeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeTreeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeTreeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeTreeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeTreeIntegration.test.ts`
