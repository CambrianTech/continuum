# Development Generate Audit Command

Audit all commands for generator conformance. Scans every command directory and checks for: matching generator spec, static accessor (Name.execute pattern), factory functions, any casts in Types files. Reports conformance status and summary statistics.

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
./jtag development/generate/audit [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('development/generate/audit', {
  // your parameters here
});
```

## Parameters

- **format** (optional): `'summary' | 'full' | 'json'` - Output format: 'summary' (counts only), 'full' (every command, default), 'json' (machine-readable)
- **filter** (optional): `'missing-spec' | 'missing-accessor' | 'has-any' | 'conformant'` - Filter to specific issue type. Omit for all commands.

## Result

Returns `DevelopmentGenerateAuditResult` with:

Returns CommandResult with:
- **totalCommands**: `number` - Total number of command directories found
- **withSpecs**: `number` - Commands that have matching generator specs
- **missingAccessors**: `number` - Commands missing the Name.execute() static accessor pattern
- **missingFactories**: `number` - Commands missing createParams/createResult factory functions
- **totalAnyCasts**: `number` - Total 'any' casts found across all command Types files
- **commandsWithAny**: `number` - Number of commands containing 'any' casts
- **orphanedSpecs**: `string[]` - Spec files with no matching command directory
- **entries**: `Array<{ commandName: string; hasSpec: boolean; hasStaticAccessor: boolean; hasFactoryFunctions: boolean; anyCastCount: number; issues: string[] }>` - Per-command audit details (when format='full' or 'json')

## Examples

### Full audit of all commands

```bash
./jtag development/generate/audit
```

**Expected result:**
{ totalCommands: 201, withSpecs: 32, missingAccessors: 41, totalAnyCasts: 14 }

### Show only commands missing specs

```bash
./jtag development/generate/audit --filter=missing-spec
```

**Expected result:**
{ entries: [{ commandName: 'agent/list', hasSpec: false, ... }, ...] }

### Machine-readable JSON for CI/scripts

```bash
./jtag development/generate/audit --format=json
```

**Expected result:**
{ totalCommands: 201, entries: [...] }

### Quick summary counts only

```bash
./jtag development/generate/audit --format=summary
```

**Expected result:**
{ totalCommands: 201, withSpecs: 32, missingAccessors: 41 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help development/generate/audit
```

**Tool:**
```typescript
// Use your help tool with command name 'development/generate/audit'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme development/generate/audit
```

**Tool:**
```typescript
// Use your readme tool with command name 'development/generate/audit'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Development Generate Audit/test/unit/DevelopmentGenerateAuditCommand.test.ts
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
npx tsx commands/Development Generate Audit/test/integration/DevelopmentGenerateAuditIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DevelopmentGenerateAuditTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DevelopmentGenerateAuditBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DevelopmentGenerateAuditServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DevelopmentGenerateAuditCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DevelopmentGenerateAuditIntegration.test.ts`
