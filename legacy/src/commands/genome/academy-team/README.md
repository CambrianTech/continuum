# Genome Academy Team Command

Start a collaborative team training project. Decomposes a project description into roles, trains each student for their role, then orchestrates collaborative building. Teacher grades both the overall project and individual role performance. Students communicate via the academy chat room.

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
./jtag genome/academy-team --projectDescription=<value> --skill=<value> --model=<value> --provider=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/academy-team', {
  // your parameters here
});
```

## Parameters

- **projectDescription** (required): `string` - What the team should build (e.g., 'side-scrolling game with mushroom people afraid of sunlight')
- **skill** (required): `string` - Skill domain (e.g., 'game-development', 'web-app', 'music-production')
- **team** (optional): `object` - Explicit team members: [{ personaId, personaName, role, roleDescription }]. If omitted, teacher LLM decomposes project into roles and RecipeAssembler matches available personas.
- **recipeId** (optional): `string` - Recipe with pre-defined roles (e.g., 'coding'). Roles extracted from recipe, personas matched by RecipeAssembler.
- **baseModel** (optional): `string` - Base model for student training (default: LOCAL_MODELS.DEFAULT)
- **model** (required): `string` - Teacher LLM model (required — teacher must be a capable cloud model)
- **provider** (required): `string` - Teacher LLM provider (required — e.g., 'deepseek', 'anthropic')
- **epochs** (optional): `number` - Training epochs per topic (default: 3)
- **buildMilestones** (optional): `number` - Number of build milestones (default: 3)

## Result

Returns `GenomeAcademyTeamResult` with:

Returns CommandResult with:
- **teamProjectId**: `string` - The created team project entity ID
- **teacherHandle**: `string` - Sentinel handle for the teacher pipeline
- **memberHandles**: `object` - Array of { personaId, personaName, role, studentHandle, sessionId } for each team member

## Examples

### Build a game with an auto-assembled team

```bash
undefined
```

### Build a web app with explicit team

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/academy-team
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/academy-team'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/academy-team
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/academy-team'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Academy Team/test/unit/GenomeAcademyTeamCommand.test.ts
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
npx tsx commands/Genome Academy Team/test/integration/GenomeAcademyTeamIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeAcademyTeamTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeAcademyTeamBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeAcademyTeamServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeAcademyTeamCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeAcademyTeamIntegration.test.ts`
