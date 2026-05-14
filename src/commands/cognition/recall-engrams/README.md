# Cognition Recall Engrams Command

Query a persona's admitted-engram store. Modes: 'recent' (default) returns newest-first N engrams; 'by_id' looks up by exact engram id; 'by_keyword' does case-insensitive substring match; 'by_origin' filters by EngramOriginKind (chat | airc | tool | self_reflection). Wraps the Rust IPC handler shipped in #1121 PR-5.

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
./jtag cognition/recall-engrams --personaId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('cognition/recall-engrams', {
  // your parameters here
});
```

## Parameters

- **personaId** (required): `string` - UUID of the persona whose engram store to query
- **kind** (optional): `'recent' | 'by_id' | 'by_keyword' | 'by_origin'` - Recall mode (default: 'recent')
- **limit** (optional): `number` - Max engrams to return (default: 10). Ignored when kind='by_id'.
- **id** (optional): `string` - Engram UUID (required when kind='by_id')
- **keyword** (optional): `string` - Substring to match against engram content (required when kind='by_keyword')
- **origin** (optional): `'chat' | 'airc' | 'tool' | 'self_reflection'` - Origin filter (required when kind='by_origin')

## Result

Returns `CognitionRecallEngramsResult` with:

Returns CommandResult with:
- **engrams**: `Array<Record<string, unknown>>` - Matching engrams (typed as Engram in shared/generated/persona/Engram.ts)
- **count**: `number` - Number of engrams returned

## Examples

### Recall the 5 most recent engrams during rag/build

```bash
./jtag cognition/recall-engrams --personaId="<uuid>" --kind="recent" --limit=5
```

**Expected result:**
{ engrams: [...], count: 5 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help cognition/recall-engrams
```

**Tool:**
```typescript
// Use your help tool with command name 'cognition/recall-engrams'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme cognition/recall-engrams
```

**Tool:**
```typescript
// Use your readme tool with command name 'cognition/recall-engrams'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Cognition Recall Engrams/test/unit/CognitionRecallEngramsCommand.test.ts
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
npx tsx commands/Cognition Recall Engrams/test/integration/CognitionRecallEngramsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CognitionRecallEngramsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CognitionRecallEngramsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CognitionRecallEngramsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CognitionRecallEngramsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CognitionRecallEngramsIntegration.test.ts`
