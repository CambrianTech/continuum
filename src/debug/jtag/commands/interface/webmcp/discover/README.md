# Interface Webmcp Discover Command

Discover WebMCP tools available on the current page. Returns structured tool definitions with schemas. Fails explicitly if WebMCP is not available.

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
./jtag interface/webmcp/discover [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('interface/webmcp/discover', {
  // your parameters here
});
```

## Parameters

- **url** (optional): `string` - URL to navigate to before discovering tools. If not provided, uses current page.

## Result

Returns `InterfaceWebmcpDiscoverResult` with:

Returns CommandResult with:
- **available**: `boolean` - Whether WebMCP is available on this page
- **reason**: `string` - Why WebMCP is unavailable (empty if available)
- **tools**: `WebMCPTool[]` - Array of available tools with name, description, and inputSchema
- **pageUrl**: `string` - URL of the page where tools were discovered

## Examples

### Discover tools on travel demo

```bash
./jtag interface/webmcp/discover --url="https://travel-demo.bandarra.me/"
```

**Expected result:**
{ available: true, tools: [{ name: "searchFlights", description: "Search for flights", inputSchema: {...} }], pageUrl: "https://travel-demo.bandarra.me/" }

### Discover tools when WebMCP unavailable

```bash
./jtag interface/webmcp/discover --url="https://example.com"
```

**Expected result:**
{ available: false, reason: "Chrome Canary 146+ with WebMCP flag required", tools: [], pageUrl: "" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help interface/webmcp/discover
```

**Tool:**
```typescript
// Use your help tool with command name 'interface/webmcp/discover'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme interface/webmcp/discover
```

**Tool:**
```typescript
// Use your readme tool with command name 'interface/webmcp/discover'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Interface Webmcp Discover/test/unit/InterfaceWebmcpDiscoverCommand.test.ts
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
npx tsx commands/Interface Webmcp Discover/test/integration/InterfaceWebmcpDiscoverIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InterfaceWebmcpDiscoverTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InterfaceWebmcpDiscoverBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InterfaceWebmcpDiscoverServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InterfaceWebmcpDiscoverCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InterfaceWebmcpDiscoverIntegration.test.ts`
