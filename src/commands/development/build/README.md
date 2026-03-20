# Development Build Command

Zero-friction TypeScript build check. Returns success or structured errors.

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
./jtag development/build [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('development/build', {
  // your parameters here
});
```

## Parameters

- **quiet** (optional): `boolean` - Only return success/failure, no output details

## Result

Returns `DevelopmentBuildResult` with:

Returns CommandResult with:
- **errorCount**: `number` - Number of compilation errors
- **errors**: `array` - Parsed errors with file, line, column, code, message
- **duration**: `number` - Build time in milliseconds
- **output**: `string` - Raw compiler output (omitted in quiet mode)

## Examples

### Quick build check

```bash
./jtag development/build
```

**Expected result:**
{ success: true, errorCount: 0, duration: 1234 }

### Quiet mode

```bash
./jtag development/build --quiet
```

**Expected result:**
{ success: false, errorCount: 3 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help development/build
```

**Tool:**
```typescript
// Use your help tool with command name 'development/build'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme development/build
```

**Tool:**
```typescript
// Use your readme tool with command name 'development/build'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Development Build/test/unit/DevelopmentBuildCommand.test.ts
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
npx tsx commands/Development Build/test/integration/DevelopmentBuildIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/DevelopmentBuildTypes.ts`
- **Browser**: Browser-specific implementation in `browser/DevelopmentBuildBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/DevelopmentBuildServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/DevelopmentBuildCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/DevelopmentBuildIntegration.test.ts`
