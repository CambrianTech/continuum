# Skill Validate Command

Validate a generated skill by running TypeScript compilation and tests in an ExecutionSandbox. Updates SkillEntity with validation results.

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
./jtag skill/validate --skillId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('skill/validate', {
  // your parameters here
});
```

## Parameters

- **skillId** (required): `string` - ID of the SkillEntity to validate

## Result

Returns `SkillValidateResult` with:

Returns CommandResult with:
- **skillId**: `string` - ID of the SkillEntity
- **name**: `string` - Skill command name
- **status**: `string` - Lifecycle status after validation
- **compiled**: `boolean` - Whether TypeScript compilation succeeded
- **testsRun**: `number` - Number of tests executed
- **testsPassed**: `number` - Number of tests that passed
- **errors**: `object` - Array of error messages from compilation or tests
- **message**: `string` - Human-readable result message

## Examples

### Validate a generated skill

```bash
./jtag skill/validate --skillId="uuid-of-skill"
```

**Expected result:**
{ compiled: true, testsRun: 3, testsPassed: 3, status: "validated" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help skill/validate
```

**Tool:**
```typescript
// Use your help tool with command name 'skill/validate'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme skill/validate
```

**Tool:**
```typescript
// Use your readme tool with command name 'skill/validate'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Skill Validate/test/unit/SkillValidateCommand.test.ts
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
npx tsx commands/Skill Validate/test/integration/SkillValidateIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SkillValidateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SkillValidateBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SkillValidateServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SkillValidateCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SkillValidateIntegration.test.ts`
