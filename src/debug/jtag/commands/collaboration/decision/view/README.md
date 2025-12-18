# Decision View Command

View detailed information about a specific governance proposal

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
./jtag decision/view --proposalId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('decision/view', {
  // your parameters here
});
```

## Parameters

- **proposalId** (required): `string` - Unique identifier for the proposal to view

## Result

Returns `DecisionViewResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the proposal was found
- **proposal**: `DecisionEntity` - The complete proposal details (null if not found)
- **summary**: `string` - Human-readable summary of proposal status and results

## Examples

### undefined

```bash
undefined
```

### undefined

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help decision/view
```

**Tool:**
```typescript
// Use your help tool with command name 'decision/view'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme decision/view
```

**Tool:**
```typescript
// Use your readme tool with command name 'decision/view'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Decision View/test/unit/DecisionViewCommand.test.ts
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
npx tsx commands/Decision View/test/integration/DecisionViewIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DecisionViewTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DecisionViewBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DecisionViewServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DecisionViewCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DecisionViewIntegration.test.ts`
