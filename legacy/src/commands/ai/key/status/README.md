# Ai Key Status Command

Report redacted API-key availability and fingerprints without exposing raw or masked secret values.

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
./jtag ai/key/status [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/key/status', {
  // your parameters here
});
```

## Parameters

- **provider** (optional): `string` - Optional provider name or config key. Omit to list all known keys.

## Result

Returns `AiKeyStatusResult` with:

Returns CommandResult with:
- **entries**: `array` - Redacted key status entries containing provider names, config key names, booleans, source, and short fingerprints only.
- **configuredCount**: `number` - Number of configured keys.
- **totalCount**: `number` - Number of checked keys.

## Examples

### List all known AI key statuses

```bash
./jtag ai/key/status
```

**Expected result:**
{ success: true, configuredCount: 1, totalCount: 11 }

### Check one provider by config key

```bash
./jtag ai/key/status --provider=OPENAI_API_KEY
```

**Expected result:**
{ success: true, configuredCount: 1, totalCount: 1 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/key/status
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/key/status'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/key/status
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/key/status'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Key Status/test/unit/AiKeyStatusCommand.test.ts
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
npx tsx commands/Ai Key Status/test/integration/AiKeyStatusIntegration.test.ts
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

**owner-only** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/AiKeyStatusTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiKeyStatusBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiKeyStatusServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiKeyStatusCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiKeyStatusIntegration.test.ts`
