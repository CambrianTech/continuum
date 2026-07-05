# Ai Key Save Command

Save an API key for a cloud AI provider. Persists to ~/.continuum/config.env, sets process.env, and emits system:config:key-added event to trigger persona creation.

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
./jtag ai/key/save --provider=<value> --value=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/key/save', {
  // your parameters here
});
```

## Parameters

- **provider** (required): `string` - The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
- **value** (required): `string` - The API key value to save

## Result

Returns `AiKeySaveResult` with:

Returns CommandResult with:
- **saved**: `boolean` - Whether the key was saved successfully
- **provider**: `string` - The config key name that was saved

## Examples

### Save an Anthropic API key

```bash
./jtag ai/key/save --provider=ANTHROPIC_API_KEY --value=sk-ant-xxx
```

**Expected result:**
{ saved: true, provider: 'ANTHROPIC_API_KEY' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/key/save
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/key/save'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/key/save
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/key/save'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Key Save/test/unit/AiKeySaveCommand.test.ts
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
npx tsx commands/Ai Key Save/test/integration/AiKeySaveIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AiKeySaveTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiKeySaveBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiKeySaveServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiKeySaveCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiKeySaveIntegration.test.ts`
