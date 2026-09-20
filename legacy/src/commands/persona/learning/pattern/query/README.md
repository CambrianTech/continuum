# Persona Learning Pattern Query Command

Query the collective pattern knowledge base. Search for patterns that might help solve the current problem.

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
./jtag persona/learning/pattern/query [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('persona/learning/pattern/query', {
  // your parameters here
});
```

## Parameters

- **domain** (optional): `string` - Filter by domain: chat, code, tools, web, general
- **type** (optional): `string` - Filter by type: debugging, tool-use, optimization, architecture, communication
- **keywords** (optional): `string[]` - Keywords to search for in pattern name/problem/solution
- **search** (optional): `string` - Free text search across all pattern fields
- **minConfidence** (optional): `number` - Minimum confidence score 0-1 (default: 0.3)
- **status** (optional): `string` - Filter by status: pending, validated, active, deprecated
- **limit** (optional): `number` - Max results to return (default: 10)
- **orderBy** (optional): `string` - Sort by: confidence, successCount, discoveredAt (default: confidence)

## Result

Returns `PersonaLearningPatternQueryResult` with:

Returns CommandResult with:
- **patterns**: `PatternSummary[]` - Matching patterns with problem/solution
- **totalMatches**: `number` - Total patterns matching the query
- **message**: `string` - Usage guidance

## Examples

### Find debugging patterns

```bash
./jtag persona/learning/pattern/query --type="debugging" --domain="code"
```

**Expected result:**
{ patterns: [...], totalMatches: 5 }

### Search for slice-related patterns

```bash
./jtag persona/learning/pattern/query --search="slice undefined" --minConfidence=0.5
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help persona/learning/pattern/query
```

**Tool:**
```typescript
// Use your help tool with command name 'persona/learning/pattern/query'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme persona/learning/pattern/query
```

**Tool:**
```typescript
// Use your readme tool with command name 'persona/learning/pattern/query'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Persona Learning Pattern Query/test/unit/PersonaLearningPatternQueryCommand.test.ts
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
npx tsx commands/Persona Learning Pattern Query/test/integration/PersonaLearningPatternQueryIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PersonaLearningPatternQueryTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PersonaLearningPatternQueryBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PersonaLearningPatternQueryServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PersonaLearningPatternQueryCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PersonaLearningPatternQueryIntegration.test.ts`
