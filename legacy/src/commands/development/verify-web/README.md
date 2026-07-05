# Development Verify Web Command

Verify web output by opening in headless Playwright browser, capturing console errors + screenshot. Used by Academy teacher to grade coding output. No blind training.

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
./jtag development/verify-web --filePath=<value> --url=<value> --waitMs=<value> --screenshot=<value> --screenshotPath=<value> --captureConsole=<value> --viewport=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('development/verify-web', {
  // your parameters here
});
```

## Parameters

- **filePath** (required): `string` - Path to HTML file to verify
- **url** (required): `string` - URL to verify (alternative to filePath)
- **waitMs** (required): `number` - Time to wait after page load before capturing (default: 2000)
- **screenshot** (required): `boolean` - Take screenshot (default: true)
- **screenshotPath** (required): `string` - Screenshot output path (default: auto-generated)
- **captureConsole** (required): `boolean` - Capture all console output (default: true)
- **viewport** (required): `string` - Viewport size WxH (default: 1280x720)

## Result

Returns `DevelopmentVerifyWebResult` with:

Returns CommandResult with:
- **success**: `boolean` - True if page loaded with zero errors
- **errors**: `array` - Runtime JavaScript errors captured from page
- **consoleOutput**: `array` - All console.log/warn/error messages
- **screenshotPath**: `string` - Path to captured screenshot
- **screenshotBase64**: `string` - Base64-encoded screenshot for AI vision
- **pageTitle**: `string` - Document title after load
- **loadTimeMs**: `number` - Page load time in milliseconds

## Examples

### Verify a generated HTML game

```bash
undefined
```

**Expected result:**
Returns errors array + screenshot. success=false if any runtime errors.

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help development/verify-web
```

**Tool:**
```typescript
// Use your help tool with command name 'development/verify-web'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme development/verify-web
```

**Tool:**
```typescript
// Use your readme tool with command name 'development/verify-web'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Development Verify Web/test/unit/DevelopmentVerifyWebCommand.test.ts
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
npx tsx commands/Development Verify Web/test/integration/DevelopmentVerifyWebIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DevelopmentVerifyWebTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DevelopmentVerifyWebBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DevelopmentVerifyWebServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DevelopmentVerifyWebCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DevelopmentVerifyWebIntegration.test.ts`
