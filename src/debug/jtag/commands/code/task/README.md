# Code Task Command

Execute a coding task end-to-end via the coding agent pipeline. Formulates a plan using LLM reasoning, enforces security tiers, and executes steps via code/* commands. Supports dry-run mode, governance approval for high-risk plans, and multi-agent delegation.

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
./jtag code/task --description=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('code/task', {
  // your parameters here
});
```

## Parameters

- **description** (required): `string` - What the coding task should accomplish (natural language)
- **taskType** (optional): `string` - Task type for model selection: 'planning' | 'generation' | 'editing' | 'review' | 'quick-fix' | 'discovery'. Defaults to 'generation'
- **relevantFiles** (optional): `string[]` - File paths already known to be relevant (hints for discovery phase)
- **dryRun** (optional): `boolean` - Execute read-only commands normally but mock writes. Returns predicted changes without modifying files
- **securityTier** (optional): `string` - Override security tier: 'discovery' | 'read' | 'write' | 'system'. Defaults to plan's assessed risk level
- **delegationEnabled** (optional): `boolean` - Enable multi-agent delegation for parallel execution across file clusters
- **maxDurationMs** (optional): `number` - Maximum execution time in milliseconds (default: 120000)
- **maxToolCalls** (optional): `number` - Maximum number of tool calls allowed (default: 15)

## Result

Returns `CodeTaskResult` with:

Returns CommandResult with:
- **status**: `string` - Overall status: 'completed' | 'partial' | 'failed' | 'budget_exceeded' | 'pending_approval'
- **summary**: `string` - Human-readable summary of what was accomplished
- **planSummary**: `string` - The LLM-generated plan summary
- **riskLevel**: `string` - Assessed risk level: 'low' | 'medium' | 'high' | 'critical'
- **securityTier**: `string` - Security tier used for execution
- **stepsTotal**: `number` - Total number of steps in the plan
- **stepsCompleted**: `number` - Number of steps that completed successfully
- **filesModified**: `string[]` - Files that were modified during execution
- **filesCreated**: `string[]` - Files that were created during execution
- **totalToolCalls**: `number` - Total tool calls used
- **totalDurationMs**: `number` - Total execution time in milliseconds
- **changeIds**: `string[]` - Change IDs from file operations (for potential undo)
- **errors**: `string[]` - Errors encountered during execution
- **proposalId**: `string` - Governance proposal ID if plan requires approval (status='pending_approval')

## Examples

### Simple code edit task

```bash
./jtag code/task --description="Add input validation to the login function in auth.ts"
```

**Expected result:**
{ status: "completed", stepsCompleted: 3, filesModified: ["auth.ts"] }

### Dry run to preview changes

```bash
./jtag code/task --description="Refactor UserService to use dependency injection" --dryRun=true
```

**Expected result:**
{ status: "completed", filesModified: [], summary: "Dry run: would modify 3 files" }

### Discovery-only task

```bash
./jtag code/task --description="Find all files using deprecated API" --taskType="discovery" --securityTier="discovery"
```

**Expected result:**
{ status: "completed", stepsCompleted: 2, filesModified: [] }

### With relevant file hints

```bash
./jtag code/task --description="Fix the off-by-one error" --relevantFiles='["src/utils/pagination.ts"]'
```

**Expected result:**
{ status: "completed", filesModified: ["src/utils/pagination.ts"] }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help code/task
```

**Tool:**
```typescript
// Use your help tool with command name 'code/task'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme code/task
```

**Tool:**
```typescript
// Use your readme tool with command name 'code/task'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Code Task/test/unit/CodeTaskCommand.test.ts
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
npx tsx commands/Code Task/test/integration/CodeTaskIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CodeTaskTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CodeTaskBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CodeTaskServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CodeTaskCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CodeTaskIntegration.test.ts`
