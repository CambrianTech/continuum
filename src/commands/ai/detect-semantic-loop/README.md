# Ai Detect Semantic Loop Command

Detects if an AI's response is semantically too similar to recent messages, preventing repetitive loop behavior

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
./jtag ai/detect-semantic-loop --messageText=<value> --personaId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/detect-semantic-loop', {
  // your parameters here
});
```

## Parameters

- **messageText** (required): `string` - The message text to check for similarity
- **personaId** (required): `string` - UUID of the persona to check message history for
- **lookbackCount** (optional): `number` - How many recent messages to compare against (default: 5)
- **similarityThreshold** (optional): `number` - Similarity threshold 0-1, higher = more strict (default: 0.85)
- **timeWindowMinutes** (optional): `number` - Only check messages within this timeframe in minutes (default: 10)
- **roomId** (optional): `string` - Room context - checks across all rooms if omitted

## Result

Returns `AiDetectSemanticLoopResult` with:

Returns CommandResult with:
- **isLoop**: `boolean` - Whether the message is detected as a loop
- **maxSimilarity**: `number` - Similarity score of the closest match (0-1)
- **matches**: `Array<{messageId: string, similarity: number, timestamp: string, excerpt: string}>` - Details of similar messages found
- **recommendation**: `'ALLOW' | 'WARN' | 'BLOCK'` - Recommendation for what to do with the message
- **explanation**: `string` - Human-readable explanation of the result

## Examples

### Check if a message is looping

```bash
./jtag ai/detect-semantic-loop --messageText="Key Insights: Take action..." --personaId="helper-uuid"
```

**Expected result:**
{ isLoop: true, maxSimilarity: 0.92, recommendation: 'WARN' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/detect-semantic-loop
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/detect-semantic-loop'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/detect-semantic-loop
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/detect-semantic-loop'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Ai Detect Semantic Loop/test/unit/AiDetectSemanticLoopCommand.test.ts
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
npx tsx commands/Ai Detect Semantic Loop/test/integration/AiDetectSemanticLoopIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AiDetectSemanticLoopTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AiDetectSemanticLoopBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiDetectSemanticLoopServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiDetectSemanticLoopCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiDetectSemanticLoopIntegration.test.ts`
