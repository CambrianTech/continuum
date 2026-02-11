# Interface Page Fill Command

Fill form fields on a web page. Use interface/page/forms first to discover available forms and their fields. This command fills fields but does NOT submit - use interface/page/submit after filling.

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
./jtag interface/page/fill --url=<value> --formId=<value> --values=<value> --waitForSelector=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/page/fill', {
  // your parameters here
});
```

## Parameters

- **url** (required): `string` - The URL of the page containing the form
- **formId** (required): `string` - The formId from interface/page/forms response
- **values** (required): `object` - Object mapping field names to values, e.g. {"from": "NYC", "to": "LAX"}
- **waitForSelector** (required): `string` - CSS selector to wait for before filling

## Result

Returns `InterfacePageFillResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether all fields were filled successfully
- **formId**: `string` - The form that was filled
- **filledFields**: `string[]` - List of field names that were successfully filled
- **failedFields**: `FieldError[]` - Fields that could not be filled, with reasons
- **remainingRequired**: `string[]` - Required fields that still need values
- **hint**: `string` - Guidance on next steps (submit if ready, or fill remaining required fields)
- **error**: `string` - Error message if operation failed entirely

## Examples

```bash
./jtag command-name
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/page/fill
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/page/fill'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/page/fill
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/page/fill'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Page Fill/test/unit/InterfacePageFillCommand.test.ts
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
npx tsx commands/Interface Page Fill/test/integration/InterfacePageFillIntegration.test.ts
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

**internal** - Internal use only, not exposed to AI personas

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/InterfacePageFillTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfacePageFillBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfacePageFillServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfacePageFillCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfacePageFillIntegration.test.ts`
