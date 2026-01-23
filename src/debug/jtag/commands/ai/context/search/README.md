# Ai Context Search Command

Semantic context navigation - search memories, messages, timeline across all entity types using cosine similarity via Rust embedding worker

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
./jtag ai/context/search --query=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/context/search', {
  // your parameters here
});
```

## Parameters

- **query** (required): `string` - Natural language query - what you're looking for
- **types** (optional): `string[]` - Entity types to search: chat_messages, memories, timeline_events, tool_results (default: all)
- **personaId** (optional): `string` - Persona ID to scope search (default: current)
- **excludeContextId** (optional): `string` - Context/room ID to exclude (for cross-context search)
- **limit** (optional): `number` - Max results (default: 10, max: 50)
- **minSimilarity** (optional): `number` - Min cosine similarity threshold 0-1 (default: 0.5)
- **since** (optional): `string` - ISO timestamp - only search items after this time
- **mode** (optional): `string` - Search mode: semantic, keyword, or hybrid (default: semantic)

## Result

Returns `AiContextSearchResult` with:

Returns CommandResult with:
- **items**: `ContextSearchItem[]` - Matching items ranked by semantic similarity
- **totalMatches**: `number` - Total matches found
- **durationMs**: `number` - Search time in milliseconds

## Examples

### Search for code-related memories

```bash
./jtag ai/context/search --query="TypeScript error handling patterns"
```

**Expected result:**
{ items: [...], totalMatches: 5, durationMs: 45 }

### Cross-context search excluding current room

```bash
./jtag ai/context/search --query="database optimization" --excludeContextId="abc123" --types="memories,timeline_events"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/context/search
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/context/search'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/context/search
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/context/search'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Context Search/test/unit/AiContextSearchCommand.test.ts
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
npx tsx commands/Ai Context Search/test/integration/AiContextSearchIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AiContextSearchTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiContextSearchBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiContextSearchServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiContextSearchCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiContextSearchIntegration.test.ts`
