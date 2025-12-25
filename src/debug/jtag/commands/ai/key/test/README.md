# Ai Key Test Command

Test an API key before saving it. Makes a minimal API call to verify the key is valid and has sufficient permissions.

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
./jtag ai/key/test --provider=<value> --key=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/key/test', {
  // your parameters here
});
```

## Parameters

- **provider** (required): `string` - Provider to test (anthropic, openai, groq, deepseek, xai, together, fireworks)
- **key** (required): `string` - API key to test (will NOT be stored)

## Result

Returns `AiKeyTestResult` with:

Returns CommandResult with:
- **valid**: `boolean` - Whether the key is valid
- **provider**: `string` - Provider that was tested
- **responseTime**: `number` - Response time in milliseconds
- **error**: `string` - Error message if key is invalid (optional)
- **models**: `string[]` - Available models for this key (optional)

## Examples

### Test an Anthropic API key

```bash
./jtag ai/key/test --provider="anthropic" --key="sk-ant-api03-..."
```

**Expected result:**
{ valid: true, provider: 'anthropic', responseTime: 342 }

### Test an invalid key

```bash
./jtag ai/key/test --provider="openai" --key="sk-invalid"
```

**Expected result:**
{ valid: false, provider: 'openai', error: 'Invalid API key' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/key/test
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/key/test'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/key/test
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/key/test'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Key Test/test/unit/AiKeyTestCommand.test.ts
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
npx tsx commands/Ai Key Test/test/integration/AiKeyTestIntegration.test.ts
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

**human-only** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/AiKeyTestTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiKeyTestBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiKeyTestServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiKeyTestCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiKeyTestIntegration.test.ts`
