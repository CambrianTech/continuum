# Ai Key Remove Command

Remove an API key for a cloud AI provider. Removes from ~/.continuum/config.env, clears process.env, and emits system:config:key-removed event to deactivate personas.

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
./jtag ai/key/remove --provider=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/key/remove', {
  // your parameters here
});
```

## Parameters

- **provider** (required): `string` - The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')

## Result

Returns `AiKeyRemoveResult` with:

Returns CommandResult with:
- **removed**: `boolean` - Whether the key was removed successfully
- **provider**: `string` - The config key name that was removed

## Examples

### Remove an Anthropic API key

```bash
./jtag ai/key/remove --provider=ANTHROPIC_API_KEY
```

**Expected result:**
{ removed: true, provider: 'ANTHROPIC_API_KEY' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/key/remove
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/key/remove'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/key/remove
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/key/remove'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Key Remove/test/unit/AiKeyRemoveCommand.test.ts
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
npx tsx commands/Ai Key Remove/test/integration/AiKeyRemoveIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AiKeyRemoveTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiKeyRemoveBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiKeyRemoveServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiKeyRemoveCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiKeyRemoveIntegration.test.ts`
