# Development Code Smell Command

Detect code smells: raw Commands.execute, any casts, god classes, missing accessors, type violations. Uses Generator SDK audit + grep patterns.

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
./jtag development/code-smell [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('development/code-smell', {
  // your parameters here
});
```

## Parameters

- **category** (optional): `string` - Filter by smell category: any-casts, raw-execute, god-class, missing-accessor, all (default: all)
- **path** (optional): `string` - Limit scan to a specific directory path relative to src/
- **fix** (optional): `boolean` - Auto-fix where possible (e.g. add missing static accessors)
- **verbose** (optional): `boolean` - Show individual file-level details instead of summary

## Result

Returns `DevelopmentCodeSmellResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the scan completed
- **totalSmells**: `number` - Total code smells found
- **categories**: `object` - Smells grouped by category with counts and file locations
- **summary**: `string` - Human-readable summary

## Examples

### Full scan

```bash
./jtag development/code-smell
```

**Expected result:**
Returns all code smells across the codebase

### Only any-casts

```bash
./jtag development/code-smell --category=any-casts
```

**Expected result:**
Returns only any-cast violations

### Scan and fix

```bash
./jtag development/code-smell --fix
```

**Expected result:**
Auto-fixes what it can (e.g. missing accessors)

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help development/code-smell
```

**Tool:**
```typescript
// Use your help tool with command name 'development/code-smell'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme development/code-smell
```

**Tool:**
```typescript
// Use your readme tool with command name 'development/code-smell'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Development Code Smell/test/unit/DevelopmentCodeSmellCommand.test.ts
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
npx tsx commands/Development Code Smell/test/integration/DevelopmentCodeSmellIntegration.test.ts
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

**internal** - Internal use only, not exposed to AI personas

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/DevelopmentCodeSmellTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DevelopmentCodeSmellBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DevelopmentCodeSmellServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DevelopmentCodeSmellCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DevelopmentCodeSmellIntegration.test.ts`
