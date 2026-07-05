# Interface Webmcp Call Command

Call a WebMCP tool on the current page. Returns structured result from the tool. Fails explicitly if WebMCP is not available or tool not found.

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
./jtag interface/webmcp/call --toolName=<value> --params=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/webmcp/call', {
  // your parameters here
});
```

## Parameters

- **toolName** (required): `string` - Name of the tool to call (from discover results)
- **params** (required): `object` - Parameters to pass to the tool (must match tool's inputSchema)
- **url** (optional): `string` - URL to navigate to before calling tool. If not provided, uses current page.

## Result

Returns `InterfaceWebmcpCallResult` with:

Returns CommandResult with:
- **called**: `boolean` - Whether the tool was successfully called
- **reason**: `string` - Why the call failed (empty if successful)
- **toolName**: `string` - Name of the tool that was called
- **result**: `unknown` - Result returned by the tool
- **pageUrl**: `string` - URL of the page where tool was called

## Examples

### Search flights using WebMCP

```bash
./jtag interface/webmcp/call --toolName="searchFlights" --params='{"origin":"SFO","destination":"NYC","date":"2026-03-15"}'
```

**Expected result:**
{ called: true, toolName: "searchFlights", result: { flights: [...] }, pageUrl: "https://travel-demo.bandarra.me/" }

### Call tool when WebMCP unavailable

```bash
./jtag interface/webmcp/call --toolName="searchFlights" --params='{}'
```

**Expected result:**
{ called: false, reason: "WebMCP not available - Chrome Canary 146+ with WebMCP flag required", toolName: "searchFlights", result: null }

### Call non-existent tool

```bash
./jtag interface/webmcp/call --toolName="nonExistent" --params='{}'
```

**Expected result:**
{ called: false, reason: "Tool 'nonExistent' not found on page", toolName: "nonExistent", result: null }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/webmcp/call
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/webmcp/call'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/webmcp/call
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/webmcp/call'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Webmcp Call/test/unit/InterfaceWebmcpCallCommand.test.ts
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
npx tsx commands/Interface Webmcp Call/test/integration/InterfaceWebmcpCallIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InterfaceWebmcpCallTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfaceWebmcpCallBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfaceWebmcpCallServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfaceWebmcpCallCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfaceWebmcpCallIntegration.test.ts`
