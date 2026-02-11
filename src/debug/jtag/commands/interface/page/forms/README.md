# Interface Page Forms Command

Discover all forms on a web page. Returns structured form definitions with field names, types, labels, and submit buttons. Works on ANY page with HTML forms - no WebMCP required. Use this first to understand what you can interact with, then use interface/page/fill and interface/page/submit.

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
./jtag interface/page/forms --url=<value> --waitForSelector=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/page/forms', {
  // your parameters here
});
```

## Parameters

- **url** (required): `string` - The URL of the page to analyze
- **waitForSelector** (required): `string` - CSS selector to wait for before analyzing (useful for dynamic pages)

## Result

Returns `InterfacePageFormsResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether form discovery succeeded
- **pageUrl**: `string` - The final URL after any redirects
- **pageTitle**: `string` - The page title
- **forms**: `FormDefinition[]` - Array of discovered forms with their fields
- **hint**: `string` - Guidance on what to do next based on discovered forms
- **error**: `string` - Error message if discovery failed

## Examples

```bash
./jtag command-name
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/page/forms
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/page/forms'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/page/forms
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/page/forms'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Page Forms/test/unit/InterfacePageFormsCommand.test.ts
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
npx tsx commands/Interface Page Forms/test/integration/InterfacePageFormsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InterfacePageFormsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfacePageFormsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfacePageFormsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfacePageFormsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfacePageFormsIntegration.test.ts`
