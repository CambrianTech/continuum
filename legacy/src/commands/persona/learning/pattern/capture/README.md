# Persona Learning Pattern Capture Command

Capture a successful pattern for cross-AI learning. When an AI discovers a working solution, they share it with the team.

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
./jtag persona/learning/pattern/capture --name=<value> --type=<value> --domain=<value> --problem=<value> --solution=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('persona/learning/pattern/capture', {
  // your parameters here
});
```

## Parameters

- **name** (required): `string` - Short descriptive name for the pattern
- **type** (required): `string` - Pattern type: debugging, tool-use, optimization, architecture, communication, other
- **domain** (required): `string` - Domain where pattern applies: chat, code, tools, web, general
- **problem** (required): `string` - The problem this pattern solves
- **solution** (required): `string` - How the pattern solves the problem
- **description** (optional): `string` - Detailed description of the pattern
- **tags** (optional): `string[]` - Searchable tags for pattern discovery
- **applicableWhen** (optional): `string[]` - Conditions when this pattern should be used
- **examples** (optional): `string[]` - Example usages of the pattern
- **makePublic** (optional): `boolean` - Make pattern immediately visible to all AIs (default: false, pending validation)

## Result

Returns `PersonaLearningPatternCaptureResult` with:

Returns CommandResult with:
- **patternId**: `string` - UUID of the created pattern
- **name**: `string` - Pattern name
- **status**: `string` - Initial status: pending or active
- **confidence**: `number` - Initial confidence score (0.5)
- **message**: `string` - Next steps guidance

## Examples

### Capture a debugging pattern

```bash
./jtag persona/learning/pattern/capture --name="Check null before slice" --type="debugging" --domain="code" --problem="slice() on undefined crashes" --solution="Always check if value exists before calling .slice()"
```

**Expected result:**
{ patternId: "abc123", status: "pending", confidence: 0.5 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help persona/learning/pattern/capture
```

**Tool:**
```typescript
// Use your help tool with command name 'persona/learning/pattern/capture'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme persona/learning/pattern/capture
```

**Tool:**
```typescript
// Use your readme tool with command name 'persona/learning/pattern/capture'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Persona Learning Pattern Capture/test/unit/PersonaLearningPatternCaptureCommand.test.ts
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
npx tsx commands/Persona Learning Pattern Capture/test/integration/PersonaLearningPatternCaptureIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PersonaLearningPatternCaptureTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PersonaLearningPatternCaptureBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PersonaLearningPatternCaptureServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PersonaLearningPatternCaptureCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PersonaLearningPatternCaptureIntegration.test.ts`
