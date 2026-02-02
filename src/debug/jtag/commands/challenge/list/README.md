# Challenge List Command

List available coding challenges with their difficulty, status, and best scores. Shows progressive challenge sequence for AI training.

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
./jtag challenge/list [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('challenge/list', {
  // your parameters here
});
```

## Parameters

- **difficulty** (optional): `string` - Filter by difficulty: beginner, intermediate, advanced, expert
- **personaId** (optional): `string` - Show scores for a specific persona

## Result

Returns `ChallengeListResult` with:

Returns CommandResult with:
- **challenges**: `object[]` - Array of challenge summaries with name, difficulty, sequence, attempts, best score
- **totalChallenges**: `number` - Total number of challenges
- **completedByPersona**: `number` - Number of challenges passed by the specified persona

## Examples

### List all challenges

```bash
./jtag challenge/list
```

**Expected result:**
{ totalChallenges: 5, challenges: [{ name: "Add a function...", difficulty: "beginner", ... }] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help challenge/list
```

**Tool:**
```typescript
// Use your help tool with command name 'challenge/list'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme challenge/list
```

**Tool:**
```typescript
// Use your readme tool with command name 'challenge/list'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Challenge List/test/unit/ChallengeListCommand.test.ts
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
npx tsx commands/Challenge List/test/integration/ChallengeListIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/ChallengeListTypes.ts`
- **Browser**: Browser-specific implementation in `browser/ChallengeListBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/ChallengeListServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/ChallengeListCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/ChallengeListIntegration.test.ts`
