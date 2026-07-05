# Interface Browser Capabilities Command

Check available browser automation capabilities. Returns explicit status for each capability (webmcp, puppeteer, etc). No fallbacks - AIs see exactly what is/isn't available.

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
./jtag interface/browser/capabilities 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/browser/capabilities', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `InterfaceBrowserCapabilitiesResult` with:

Returns CommandResult with:
- **webmcp**: `boolean` - Whether WebMCP (navigator.modelContext) is available
- **webmcpReason**: `string` - Why WebMCP is unavailable (empty if available)
- **puppeteer**: `boolean` - Whether Puppeteer automation is available
- **puppeteerReason**: `string` - Why Puppeteer is unavailable (empty if available)
- **systemBrowser**: `boolean` - Whether system browser (open/xdg-open) is available
- **availableBackends**: `string[]` - List of available browser automation backends

## Examples

### Check capabilities (WebMCP available)

```bash
./jtag interface/browser/capabilities
```

**Expected result:**
{ webmcp: true, webmcpReason: "", puppeteer: true, puppeteerReason: "", systemBrowser: true, availableBackends: ["webmcp", "puppeteer", "system-browser"] }

### Check capabilities (WebMCP not available)

```bash
./jtag interface/browser/capabilities
```

**Expected result:**
{ webmcp: false, webmcpReason: "Chrome Canary 146+ with WebMCP flag required", puppeteer: true, puppeteerReason: "", systemBrowser: true, availableBackends: ["puppeteer", "system-browser"] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/browser/capabilities
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/browser/capabilities'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/browser/capabilities
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/browser/capabilities'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Browser Capabilities/test/unit/InterfaceBrowserCapabilitiesCommand.test.ts
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
npx tsx commands/Interface Browser Capabilities/test/integration/InterfaceBrowserCapabilitiesIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InterfaceBrowserCapabilitiesTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfaceBrowserCapabilitiesBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfaceBrowserCapabilitiesServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfaceBrowserCapabilitiesCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfaceBrowserCapabilitiesIntegration.test.ts`
