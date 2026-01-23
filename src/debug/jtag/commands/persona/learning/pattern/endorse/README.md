# Persona Learning Pattern Endorse Command

Report the outcome of using a pattern. Updates confidence scores and can trigger validation or deprecation.

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
./jtag persona/learning/pattern/endorse --patternId=<value> --success=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('persona/learning/pattern/endorse', {
  // your parameters here
});
```

## Parameters

- **patternId** (required): `string` - UUID of the pattern being endorsed
- **success** (required): `boolean` - Was the pattern successful (true) or did it fail (false)
- **notes** (optional): `string` - Optional notes about the usage experience

## Result

Returns `PersonaLearningPatternEndorseResult` with:

Returns CommandResult with:
- **patternId**: `string` - Pattern that was endorsed
- **previousConfidence**: `number` - Confidence before endorsement
- **newConfidence**: `number` - Confidence after endorsement
- **statusChanged**: `boolean` - Whether the pattern status changed
- **newStatus**: `string` - Current pattern status
- **message**: `string` - Description of what happened
- **trainingCandidate**: `boolean` - Whether pattern is now eligible for LoRA training

## Examples

### Report successful pattern usage

```bash
./jtag persona/learning/pattern/endorse --patternId="abc123" --success=true
```

**Expected result:**
{ previousConfidence: 0.6, newConfidence: 0.72, statusChanged: false }

### Report pattern failure

```bash
./jtag persona/learning/pattern/endorse --patternId="abc123" --success=false --notes="Pattern didnt apply to this case"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help persona/learning/pattern/endorse
```

**Tool:**
```typescript
// Use your help tool with command name 'persona/learning/pattern/endorse'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme persona/learning/pattern/endorse
```

**Tool:**
```typescript
// Use your readme tool with command name 'persona/learning/pattern/endorse'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Persona Learning Pattern Endorse/test/unit/PersonaLearningPatternEndorseCommand.test.ts
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
npx tsx commands/Persona Learning Pattern Endorse/test/integration/PersonaLearningPatternEndorseIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/PersonaLearningPatternEndorseTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PersonaLearningPatternEndorseBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PersonaLearningPatternEndorseServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PersonaLearningPatternEndorseCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PersonaLearningPatternEndorseIntegration.test.ts`
