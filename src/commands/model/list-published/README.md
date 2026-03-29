# Model List Published Command

List all published models from the continuum-ai HuggingFace org — download counts, likes, improvement scores, hardware targets, tags.

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
./jtag model/list-published [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/list-published', {
  // your parameters here
});
```

## Parameters

- **domain** (optional): `string` - Filter by domain (code, reasoning, general). If omitted, returns all.
- **includeGguf** (optional): `boolean` - Include GGUF variant repos in the list. Default false.

## Result

Returns `ModelListPublishedResult` with:

Returns CommandResult with:
- **models**: `array` - List of published models
- **totalDownloads**: `number` - Sum of all model downloads
- **totalModels**: `number` - Number of published models

## Examples

### List all published models

```bash
./jtag model/list-published
```

**Expected result:**
{ totalModels: 12, totalDownloads: 3649, models: [{ name: "qwen3.5-4b-code-forged", downloads: 141, improvementPct: 26.6 }, ...] }

### List code models only

```bash
./jtag model/list-published --domain=code
```

**Expected result:**
{ totalModels: 5, models: [{ name: "qwen3.5-27b-code-forged", domain: "code" }, ...] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/list-published
```

**Tool:**
```typescript
// Use your help tool with command name 'model/list-published'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/list-published
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/list-published'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model List Published/test/unit/ModelListPublishedCommand.test.ts
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
npx tsx commands/Model List Published/test/integration/ModelListPublishedIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelListPublishedTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelListPublishedBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelListPublishedServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelListPublishedCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelListPublishedIntegration.test.ts`
