# State Content Switch Command

Switch to an existing open content item (set as current/highlighted tab). Does NOT add to openItems - use content/open for that.

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
./jtag state/content/switch --userId=<value> --contentItemId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('state/content/switch', {
  // your parameters here
});
```

## Parameters

- **userId** (required): `UUID` - User ID whose content state to modify
- **contentItemId** (required): `UUID` - ID of the content item to switch to (must already be in openItems)

## Result

Returns `StateContentSwitchResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the switch operation succeeded
- **currentItemId**: `UUID` - Current item ID after switching
- **error**: `string | undefined` - Error message if operation failed (e.g., item not found in openItems)

## Examples

### Switch to an existing open tab

```bash
./jtag state/content/switch --userId="abc-123" --contentItemId="xyz-789"
```

**Expected result:**
{ success: true, currentItemId: "xyz-789" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help state/content/switch
```

**Tool:**
```typescript
// Use your help tool with command name 'state/content/switch'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme state/content/switch
```

**Tool:**
```typescript
// Use your readme tool with command name 'state/content/switch'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/State Content Switch/test/unit/StateContentSwitchCommand.test.ts
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
npx tsx commands/State Content Switch/test/integration/StateContentSwitchIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/StateContentSwitchTypes.ts`
- **Browser**: Browser-specific implementation in `browser/StateContentSwitchBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/StateContentSwitchServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/StateContentSwitchCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/StateContentSwitchIntegration.test.ts`
