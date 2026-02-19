# Decision Create Command

Create a new governance proposal with voting options

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
./jtag decision/create --proposalId=<value> --topic=<value> --rationale=<value> --description=<value> --options=<value> --tags=<value> --votingDeadline=<value> --requiredQuorum=<value> --visibility=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('decision/create', {
  // your parameters here
});
```

## Parameters

- **proposalId** (required): `string` - Unique identifier for the proposal (e.g., 'PROP-2024-001')
- **topic** (required): `string` - Short title for the proposal
- **rationale** (required): `string` - Why this decision is needed
- **description** (required): `string` - Detailed explanation of the proposal
- **options** (required): `DecisionOption[]` - Array of voting options (minimum 2 required)
- **tags** (required): `string[]` - Optional tags for categorization
- **votingDeadline** (required): `string` - Optional ISO timestamp for voting deadline
- **requiredQuorum** (required): `number` - Optional minimum number of votes required
- **visibility** (required): `string` - Visibility level: 'public' or 'private' (defaults to 'public')

## Result

Returns `DecisionCreateResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether proposal was created successfully
- **proposalId**: `string` - The created proposal ID
- **proposedBy**: `UUID` - ID of the user who created the proposal
- **proposedAt**: `string` - ISO timestamp of creation
- **status**: `string` - Initial status (typically 'open')

## Examples

### undefined

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help decision/create
```

**Tool:**
```typescript
// Use your help tool with command name 'decision/create'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme decision/create
```

**Tool:**
```typescript
// Use your readme tool with command name 'decision/create'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Decision Create/test/unit/DecisionCreateCommand.test.ts
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
npx tsx commands/Decision Create/test/integration/DecisionCreateIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DecisionCreateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DecisionCreateBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DecisionCreateServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DecisionCreateCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DecisionCreateIntegration.test.ts`
