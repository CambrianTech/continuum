# Git Push Command

Push workspace branch to remote repository

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
./jtag git/push [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('git/push', {
  // your parameters here
});
```

## Parameters

- **workspacePath** (optional): `string` - Path to workspace (auto-detected from context if not provided)
- **remote** (optional): `string` - Remote name (defaults to 'origin')

## Result

Returns `GitPushResult` with:

Returns CommandResult with:
- **branch**: `string` - Branch that was pushed
- **remote**: `string` - Remote repository pushed to
- **commitsPushed**: `number` - Number of commits pushed

## Examples

### Push to origin

```bash
./jtag git/push
```

**Expected result:**
{ success: true, branch: "helper/docs-edit", commitsPushed: 2 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help git/push
```

**Tool:**
```typescript
// Use your help tool with command name 'git/push'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme git/push
```

**Tool:**
```typescript
// Use your readme tool with command name 'git/push'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Git Push/test/unit/GitPushCommand.test.ts
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
npx tsx commands/Git Push/test/integration/GitPushIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GitPushTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GitPushBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GitPushServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GitPushCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GitPushIntegration.test.ts`
