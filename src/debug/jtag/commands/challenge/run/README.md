# Challenge Run Command

Run a coding challenge against the AI coding pipeline. Sets up a fresh workspace, executes the challenge via code/task, evaluates with AI judge, and records the attempt.

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
./jtag challenge/run [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('challenge/run', {
  // your parameters here
});
```

## Parameters

- **challengeId** (optional): `string` - Specific challenge ID to run. If not provided, runs the next unbeaten challenge
- **challengeNumber** (optional): `number` - Run challenge by sequence number (1-5)
- **personaId** (optional): `string` - Which AI persona runs the challenge. Defaults to the calling user
- **skipJudge** (optional): `boolean` - Skip AI judge evaluation (faster, just checks execution success)

## Result

Returns `ChallengeRunResult` with:

Returns CommandResult with:
- **challengeName**: `string` - Name of the challenge that was run
- **difficulty**: `string` - Challenge difficulty level
- **status**: `string` - Attempt outcome: passed, failed, partial, timeout, error
- **score**: `number` - Judge score from 0-100
- **feedback**: `string` - Judge feedback on the attempt
- **durationMs**: `number` - Total execution time in milliseconds
- **toolCallsUsed**: `number` - Number of tool calls consumed
- **filesModified**: `string[]` - Files modified during the attempt
- **filesCreated**: `string[]` - Files created during the attempt
- **errors**: `string[]` - Errors encountered during execution

## Examples

### Run the next unbeaten challenge

```bash
./jtag challenge/run
```

**Expected result:**
{ status: "passed", score: 85, challengeName: "Add a function to a single file" }

### Run a specific challenge by number

```bash
./jtag challenge/run --challengeNumber=3
```

**Expected result:**
{ status: "partial", score: 60, challengeName: "Extract shared utility from duplicate code" }

### Quick run without AI judge

```bash
./jtag challenge/run --challengeNumber=1 --skipJudge=true
```

**Expected result:**
{ status: "passed", score: 70, feedback: "Pipeline completed." }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help challenge/run
```

**Tool:**
```typescript
// Use your help tool with command name 'challenge/run'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme challenge/run
```

**Tool:**
```typescript
// Use your readme tool with command name 'challenge/run'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Challenge Run/test/unit/ChallengeRunCommand.test.ts
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
npx tsx commands/Challenge Run/test/integration/ChallengeRunIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ChallengeRunTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ChallengeRunBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ChallengeRunServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ChallengeRunCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ChallengeRunIntegration.test.ts`
