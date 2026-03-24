# Model Search Command

Search HuggingFace for base models by name, architecture, or size. Used to find compaction targets (e.g., 'Qwen 3.5 27B'). Different from adapter/search which finds LoRA adapters.

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
./jtag model/search --query=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('model/search', {
  // your parameters here
});
```

## Parameters

- **query** (required): `string` - Search query (e.g., 'Qwen3.5', 'codellama', 'mistral 7b')
- **limit** (optional): `number` - Max results to return. Default: 10.
- **sort** (optional): `string` - Sort by: 'downloads', 'likes', 'recent'. Default: 'downloads'.
- **minSize** (optional): `number` - Minimum model size in billions of parameters (e.g., 7, 14, 27)
- **maxSize** (optional): `number` - Maximum model size in billions of parameters

## Result

Returns `ModelSearchResult` with:

Returns CommandResult with:
- **models**: `object` - Array of { id, author, downloads, likes, tags, pipeline_tag, lastModified }
- **totalCount**: `number` - Total number of results found

## Examples

### Find Qwen 3.5 models

```bash
undefined
```

### Find coding models between 14B and 32B

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help model/search
```

**Tool:**
```typescript
// Use your help tool with command name 'model/search'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme model/search
```

**Tool:**
```typescript
// Use your readme tool with command name 'model/search'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Model Search/test/unit/ModelSearchCommand.test.ts
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
npx tsx commands/Model Search/test/integration/ModelSearchIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ModelSearchTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ModelSearchBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ModelSearchServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ModelSearchCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ModelSearchIntegration.test.ts`
