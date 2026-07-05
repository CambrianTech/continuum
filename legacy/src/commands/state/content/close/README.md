# State Content Close Command

Close a content item (remove from user's open tabs). Handles currentItemId reassignment if closing the active tab.

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
./jtag state/content/close --userId=<value> --contentItemId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('state/content/close', {
  // your parameters here
});
```

## Parameters

- **userId** (required): `UUID` - User ID whose content state to modify
- **contentItemId** (required): `UUID` - ID of the content item to close

## Result

Returns `StateContentCloseResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the close operation succeeded
- **openItemsCount**: `number` - Number of open items after closing
- **currentItemId**: `UUID | undefined` - New current item ID after closing (may change if closed item was active)
- **error**: `string | undefined` - Error message if operation failed

## Examples

### Close a specific tab by content item ID

```bash
./jtag state/content/close --userId="abc-123" --contentItemId="xyz-789"
```

**Expected result:**
{ success: true, openItemsCount: 2, currentItemId: "new-current-id" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help state/content/close
```

**Tool:**
```typescript
// Use your help tool with command name 'state/content/close'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme state/content/close
```

**Tool:**
```typescript
// Use your readme tool with command name 'state/content/close'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/State Content Close/test/unit/StateContentCloseCommand.test.ts
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
npx tsx commands/State Content Close/test/integration/StateContentCloseIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/StateContentCloseTypes.ts`
- **Browser**: Browser-specific implementation in `browser/StateContentCloseBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/StateContentCloseServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/StateContentCloseCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/StateContentCloseIntegration.test.ts`
