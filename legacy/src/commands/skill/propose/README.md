# Skill Propose Command

Propose a new skill (command) specification. Creates a SkillEntity with status 'proposed'. For team-scoped skills, creates a DecisionProposal for governance approval.

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
./jtag skill/propose --name=<value> --description=<value> --skillParams=<value> --skillResults=<value> --implementation=<value> --personaId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('skill/propose', {
  // your parameters here
});
```

## Parameters

- **name** (required): `string` - Command name (e.g., 'analysis/complexity')
- **description** (required): `string` - What the skill does
- **skillParams** (required): `object` - Input parameters spec array [{name, type, optional?, description?}]
- **skillResults** (required): `object` - Output fields spec array [{name, type, description?}]
- **implementation** (required): `string` - Natural language description of the implementation logic
- **scope** (optional): `string` - Who can use it: 'personal' (default) or 'team' (requires approval)
- **examples** (optional): `object` - Usage examples array [{description, command, expectedResult?}]
- **personaId** (required): `string` - AI persona proposing this skill

## Result

Returns `SkillProposeResult` with:

Returns CommandResult with:
- **skillId**: `string` - ID of the created SkillEntity
- **name**: `string` - Skill command name
- **status**: `string` - Lifecycle status after proposal
- **scope**: `string` - Skill scope (personal or team)
- **proposalId**: `string` - DecisionProposal ID if team-scoped
- **message**: `string` - Human-readable result message

## Examples

### Propose a personal analysis skill

```bash
./jtag skill/propose --name="analysis/complexity" --description="Analyze code complexity" --implementation="Count cyclomatic complexity per function" --personaId="ai-001"
```

**Expected result:**
{ skillId: "uuid", name: "analysis/complexity", status: "proposed", scope: "personal" }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help skill/propose
```

**Tool:**
```typescript
// Use your help tool with command name 'skill/propose'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme skill/propose
```

**Tool:**
```typescript
// Use your readme tool with command name 'skill/propose'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Skill Propose/test/unit/SkillProposeCommand.test.ts
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
npx tsx commands/Skill Propose/test/integration/SkillProposeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SkillProposeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SkillProposeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SkillProposeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SkillProposeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SkillProposeIntegration.test.ts`
