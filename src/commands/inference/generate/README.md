# Inference Generate Command

Generate text using local or cloud AI inference. Auto-routes to best available backend (Candle → Ollama → cloud). Handles model loading, LoRA adapters, and provider failover automatically.

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
./jtag inference/generate --prompt=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('inference/generate', {
  // your parameters here
});
```

## Parameters

- **prompt** (required): `string` - The prompt text to generate from
- **model** (optional): `string` - Model to use (e.g., 'llama3.2:3b', 'Qwen/Qwen2-1.5B-Instruct'). Defaults to LOCAL_MODELS.DEFAULT
- **provider** (optional): `string` - Preferred provider: 'candle' | 'ollama' | 'anthropic' | 'openai' | 'groq' | 'together' | 'fireworks'. Auto-routes if not specified
- **maxTokens** (optional): `number` - Maximum tokens to generate (default: 2048)
- **temperature** (optional): `number` - Sampling temperature 0.0-2.0 (default: 0.7)
- **systemPrompt** (optional): `string` - System prompt to prepend
- **adapters** (optional): `string[]` - LoRA adapter names to apply (local inference only). Skips missing adapters gracefully

## Result

Returns `InferenceGenerateResult` with:

Returns CommandResult with:
- **text**: `string` - Generated text
- **model**: `string` - Actual model used (may differ from requested if mapped)
- **provider**: `string` - Provider that handled the request
- **isLocal**: `boolean` - Whether inference was local (Candle/Ollama) or cloud
- **adaptersApplied**: `string[]` - LoRA adapters that were actually applied
- **inputTokens**: `number` - Number of input tokens processed
- **outputTokens**: `number` - Number of tokens generated
- **responseTimeMs**: `number` - Total response time in milliseconds

## Examples

### Simple local generation

```bash
./jtag inference/generate --prompt="Say hello"
```

**Expected result:**
{ text: "Hello! How can I help?", provider: "candle", isLocal: true }

### Specify model and max tokens

```bash
./jtag inference/generate --prompt="Explain recursion" --model="llama3.2:3b" --maxTokens=500
```

**Expected result:**
{ text: "Recursion is...", model: "Qwen/Qwen2-1.5B-Instruct" }

### Use cloud provider

```bash
./jtag inference/generate --prompt="Write a haiku" --provider="anthropic"
```

**Expected result:**
{ text: "...", provider: "anthropic", isLocal: false }

### Apply LoRA adapters

```bash
./jtag inference/generate --prompt="Review this code" --adapters='["typescript-expertise"]'
```

**Expected result:**
{ adaptersApplied: ["typescript-expertise"] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help inference/generate
```

**Tool:**
```typescript
// Use your help tool with command name 'inference/generate'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme inference/generate
```

**Tool:**
```typescript
// Use your readme tool with command name 'inference/generate'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Inference Generate/test/unit/InferenceGenerateCommand.test.ts
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
npx tsx commands/Inference Generate/test/integration/InferenceGenerateIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/InferenceGenerateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/InferenceGenerateBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/InferenceGenerateServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/InferenceGenerateCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/InferenceGenerateIntegration.test.ts`
