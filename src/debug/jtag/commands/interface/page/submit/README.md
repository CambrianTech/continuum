# Interface Page Submit Command

Submit a form on a web page. Use interface/page/forms to discover forms, interface/page/fill to populate fields, then this command to submit. Returns the resulting page state after submission.

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
./jtag interface/page/submit --url=<value> --formId=<value> --values=<value> --waitForNavigation=<value> --waitForSelector=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/page/submit', {
  // your parameters here
});
```

## Parameters

- **url** (required): `string` - The URL of the page containing the form
- **formId** (required): `string` - The formId from interface/page/forms response
- **values** (required): `object` - Optional: fill these values before submitting (combines fill + submit)
- **waitForNavigation** (required): `boolean` - Wait for page navigation after submit (default: true)
- **waitForSelector** (required): `string` - Wait for this selector on the result page

## Result

Returns `InterfacePageSubmitResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether form was submitted successfully
- **formId**: `string` - The form that was submitted
- **navigatedTo**: `string` - The URL after form submission (may be same page or new page)
- **pageTitle**: `string` - Title of the resulting page
- **pageContent**: `string` - Brief summary of the result page content (first 500 chars of visible text)
- **hasMoreForms**: `boolean` - Whether the result page has forms (call interface/page/forms to discover)
- **hint**: `string` - Guidance on what to do with the result page
- **error**: `string` - Error message if submission failed

## Examples

```bash
./jtag command-name
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/page/submit
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/page/submit'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/page/submit
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/page/submit'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Page Submit/test/unit/InterfacePageSubmitCommand.test.ts
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
npx tsx commands/Interface Page Submit/test/integration/InterfacePageSubmitIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InterfacePageSubmitTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfacePageSubmitBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfacePageSubmitServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfacePageSubmitCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfacePageSubmitIntegration.test.ts`
