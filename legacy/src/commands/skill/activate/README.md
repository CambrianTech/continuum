# Skill Activate Command

Activate a validated skill by registering it as a live command. The skill becomes available for use by the creator (personal) or all personas (team).

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
./jtag skill/activate --skillId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('skill/activate', {
  // your parameters here
});
```

## Parameters

- **skillId** (required): `string` - ID of the SkillEntity to activate

## Result

Returns `SkillActivateResult` with:

Returns CommandResult with:
- **skillId**: `string` - ID of the SkillEntity
- **name**: `string` - Skill command name
- **status**: `string` - Lifecycle status after activation
- **activatedAt**: `number` - Timestamp when the skill was activated
- **message**: `string` - Human-readable result message

## Examples

### Activate a validated skill

```bash
./jtag skill/activate --skillId="uuid-of-skill"
```

**Expected result:**
{ skillId: "uuid", name: "analysis/complexity", status: "active" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help skill/activate
```

**Tool:**
```typescript
// Use your help tool with command name 'skill/activate'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme skill/activate
```

**Tool:**
```typescript
// Use your readme tool with command name 'skill/activate'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Skill Activate/test/unit/SkillActivateCommand.test.ts
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
npx tsx commands/Skill Activate/test/integration/SkillActivateIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SkillActivateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SkillActivateBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SkillActivateServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SkillActivateCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SkillActivateIntegration.test.ts`
