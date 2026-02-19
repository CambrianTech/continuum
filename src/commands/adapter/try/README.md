# Adapter Try Command

Temporarily load a LoRA adapter and run A/B comparison test

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
./jtag adapter/try --adapterId=<value> --testPrompt=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('adapter/try', {
  // your parameters here
});
```

## Parameters

- **adapterId** (required): `string` - Adapter ID (HuggingFace repo ID or local adapter name)
- **testPrompt** (required): `string` - Prompt to test with and without the adapter
- **scale** (optional): `number` - Adapter scale/weight (default: 1.0)
- **maxTokens** (optional): `number` - Max tokens for test generation (default: 100)

## Result

Returns `AdapterTryResult` with:

Returns CommandResult with:
- **adapterId**: `string` - Adapter that was tested
- **baselineOutput**: `string` - Output without adapter
- **adapterOutput**: `string` - Output with adapter loaded
- **baselineTimeMs**: `number` - Generation time without adapter
- **adapterTimeMs**: `number` - Generation time with adapter
- **adapterMetadata**: `object` - Adapter metadata (base model, rank, etc.)

## Examples

### Test a tool-calling adapter

```bash
./jtag adapter/try --adapterId="codelion/Llama-3.2-1B-Instruct-tool-calling-lora" --testPrompt="List all files in the current directory"
```

**Expected result:**
{ baselineOutput: '...', adapterOutput: '...', ... }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help adapter/try
```

**Tool:**
```typescript
// Use your help tool with command name 'adapter/try'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme adapter/try
```

**Tool:**
```typescript
// Use your readme tool with command name 'adapter/try'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Adapter Try/test/unit/AdapterTryCommand.test.ts
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
npx tsx commands/Adapter Try/test/integration/AdapterTryIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AdapterTryTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AdapterTryBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AdapterTryServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AdapterTryCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AdapterTryIntegration.test.ts`
