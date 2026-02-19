# TDD in Templates: Test-Driven Development Baked In

**Philosophy: Every generated component comes with its tests ready to go.**

> "Yeah we should build in TDD to these commands, daemons and widgets too, by the template" - Joel

---

## Table of Contents

- [The Vision](#the-vision)
- [Why TDD in Templates](#why-tdd-in-templates)
- [Two-Layer Testing Strategy](#two-layer-testing-strategy)
- [Generated Test Structure](#generated-test-structure)
- [TDD Workflow](#tdd-workflow)
- [Template Design](#template-design)
- [Running Tests](#running-tests)
- [For Personas](#for-personas)
- [Future: Daemons and Widgets](#future-daemons-and-widgets)

---

## The Vision

When you generate a command with `./jtag generate`, you get:

```
commands/my-command/
├── shared/MyCommandTypes.ts           # Types and business logic
├── server/MyCommandServerCommand.ts   # Server implementation
├── browser/MyCommandBrowserCommand.ts # Browser delegation
├── test/
│   ├── unit/MyCommandCommand.test.ts         # Unit tests (mock dependencies)
│   └── integration/MyCommandIntegration.test.ts  # Integration tests (live system)
└── README.md                          # Complete documentation with Testing section
```

**Result**: Every command is testable from day one. No excuses, no setup friction. Tests are part of the pattern.

---

## Why TDD in Templates

### 1. **Zero Friction** - Tests are already there
No need to "add tests later" (which never happens). The test file is generated, structured, and ready to fill in.

### 2. **Perfect Patterns** - Tests demonstrate best practices
Generated tests show developers (and AIs!) how to:
- Test required parameter validation
- Mock dependencies properly
- Use assertion helpers
- Test performance
- Handle edge cases

### 3. **Safety Net** - Tests catch regressions immediately
Before committing, run tests. If they fail, don't commit. Simple as that.

### 4. **Documentation** - Tests are executable specifications
The test file shows exactly what the command does, with concrete examples.

### 5. **Confidence** - Refactor without fear
With comprehensive tests, you can refactor knowing you won't break anything.

---

## Two-Layer Testing Strategy

### Layer 1: Unit Tests (Fast, Isolated)

**Purpose**: Test command logic in isolation using mock dependencies.

**Characteristics**:
- No server required
- No WebSocket connection
- Uses mock context, mock execution
- Runs in milliseconds
- Tests pure logic

**Example**:
```bash
# Run instantly, no server needed
npx tsx commands/my-command/test/unit/MyCommandCommand.test.ts
```

**What's Tested**:
1. Command structure validation
2. Mock command execution
3. Required parameter validation (throws ValidationError)
4. Optional parameter handling (sensible defaults)
5. Performance requirements
6. Assertion utility helpers

### Layer 2: Integration Tests (Real, System-Wide)

**Purpose**: Test command with real client connections and system integration.

**Characteristics**:
- Server must be running (`npm start`)
- Real WebSocket connection
- Actual command execution
- Tests full stack
- Slower (seconds)

**Example**:
```bash
# Prerequisites: npm start (wait 90+ seconds)
npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
```

**What's Tested**:
1. Client connection to live system
2. Real command execution via WebSocket
3. ValidationError handling in production
4. Optional parameter defaults in production
5. Performance under load
6. Various parameter combinations

---

## Generated Test Structure

### Unit Test Template

Every generated unit test includes:

```typescript
/**
 * Test 1: Command structure validation
 * - Validates param structure
 * - Ensures context/sessionId present
 */
function testMyCommandCommandStructure() { /* ... */ }

/**
 * Test 2: Mock command execution
 * - Creates mock result
 * - Tests mock command
 * - Validates result structure
 */
async function testMockMyCommandExecution() { /* ... */ }

/**
 * Test 3: Required parameter validation (CRITICAL)
 * - Ensures command throws ValidationError when required params missing
 * - Tests error message is tool-agnostic
 * - BEST PRACTICE demonstration
 */
async function testMyCommandRequiredParams() { /* ... */ }

/**
 * Test 4: Optional parameter handling
 * - Tests sensible defaults
 * - Validates command succeeds without optional params
 */
async function testMyCommandOptionalParams() { /* ... */ }

/**
 * Test 5: Performance validation
 * - Sets reasonable performance expectations
 * - Validates command completes within time limit
 */
async function testMyCommandPerformance() { /* ... */ }

/**
 * Test 6: Result assertion helpers
 * - Tests assertion utility functions
 * - Validates result matching
 */
async function testMyCommandAssertionHelpers() { /* ... */ }
```

### Integration Test Template

Every generated integration test includes:

```typescript
/**
 * Test 1: Client connection
 * - Connects to live JTAG system
 * - Validates connection info
 */
async function testMyCommandClientConnection() { /* ... */ }

/**
 * Test 2: Real command execution
 * - Executes command via WebSocket
 * - Validates result from live system
 */
async function testRealMyCommandExecution() { /* ... */ }

/**
 * Test 3: Missing required parameters (CRITICAL)
 * - Ensures live system throws ValidationError
 * - Validates error handling in production
 */
async function testMyCommandMissingParams() { /* ... */ }

/**
 * Test 4: Optional parameters
 * - Tests with optional params
 * - Tests without optional params (defaults)
 */
async function testMyCommandOptionalParams() { /* ... */ }

/**
 * Test 5: Performance under load
 * - Runs multiple iterations
 * - Measures average and max execution time
 */
async function testMyCommandPerformance() { /* ... */ }

/**
 * Test 6: Parameter combinations
 * - Tests edge cases
 * - Various valid parameter combinations
 */
async function testMyCommandParameterCombinations() { /* ... */ }
```

---

## TDD Workflow

### Phase 1: Build in Isolation

```bash
# 1. Generate command with tests
npx tsx generator/CommandGenerator.ts /tmp/my-spec.json /tmp/output

# 2. You now have:
#    - Implementation files (with TODO comments)
#    - Unit test file (with TODO sections)
#    - Integration test file (with TODO sections)
```

### Phase 2: Write Tests First (TDD)

```bash
# 3. Fill in unit tests FIRST (test-driven development)
# Edit: /tmp/output/my-command/test/unit/MyCommandCommand.test.ts
# - Fill in Test 3 (required param validation) - CRITICAL!
# - Fill in Test 4 (optional param handling)
# - Fill in other tests as needed

# 4. Run tests, watch them FAIL (red)
npx tsx /tmp/output/my-command/test/unit/MyCommandCommand.test.ts
# ❌ Tests fail because implementation is still TODO
```

### Phase 3: Implement to Pass Tests (Green)

```bash
# 5. Implement the command logic
# Edit: /tmp/output/my-command/server/MyCommandServerCommand.ts
# - Add validation that throws ValidationError
# - Implement actual logic
# - Handle optional params with defaults

# 6. Run tests again, watch them PASS (green)
npx tsx /tmp/output/my-command/test/unit/MyCommandCommand.test.ts
# ✅ All tests pass!
```

### Phase 4: Integration Testing

```bash
# 7. Copy to live system (only after unit tests pass)
cp -r /tmp/output/my-command src/commands/

# 8. Deploy
npm run build:ts
npm start  # Wait 90+ seconds

# 9. Fill in integration tests
# Edit: commands/my-command/test/integration/MyCommandIntegration.test.ts

# 10. Run integration tests
npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
# ✅ Integration tests pass!
```

### Phase 5: Commit with Confidence

```bash
# 11. All tests pass - safe to commit
git add src/commands/my-command
git commit -m "Add my-command with comprehensive tests"

# No fear of regressions - tests prove it works
```

---

## Template Design

### TODO Markers Guide Development

Generated tests include `TODO` markers to guide developers:

```typescript
/**
 * Test 3: Required parameter validation
 */
async function testMyCommandRequiredParams() {
  console.log('\n🚨 Test 3: Required parameter validation');

  // TODO: Create mock command that validates required params
  // Example:
  // const strictMyCommandCommand = async (params: MyCommandParams): Promise<MyCommandResult> => {
  //   if (!params.requiredParam || params.requiredParam.trim() === '') {
  //     throw new ValidationError(
  //       'requiredParam',
  //       `Missing required parameter 'requiredParam'. ` +
  //       `Use the help tool with 'my-command' or see the my-command README for usage information.`
  //     );
  //   }
  //   // ... implementation
  // };

  console.log('⚠️  TODO: Add required parameter validation for my-command');
}
```

### Example Code Shows Best Practices

Each TODO section includes example code demonstrating:
- ValidationError usage
- Tool-agnostic error messages
- Mock command patterns
- Performance expectations
- Assertion helpers

### Progressive Disclosure

Tests are structured to guide developers through complexity:
1. Start with simple structure validation
2. Move to mock execution
3. Then critical validation (required params)
4. Then optional params
5. Then performance
6. Finally assertion helpers

---

## Running Tests

### Quick Reference

```bash
# Unit tests (fast, no server)
npx tsx commands/[command-name]/test/unit/[ClassName]Command.test.ts

# Integration tests (slow, requires server)
npm start  # Wait 90+ seconds
npx tsx commands/[command-name]/test/integration/[ClassName]Integration.test.ts

# Run all tests for a command
npx tsx commands/[command-name]/test/unit/[ClassName]Command.test.ts && \
npx tsx commands/[command-name]/test/integration/[ClassName]Integration.test.ts
```

### Test Utilities Available

Commands have access to comprehensive test utilities:

**From `CommandTestUtils.ts`**:
- `createTestContext()` - Create mock JTAG context
- `createTestPayload()` - Create test command payload
- `validateCommandResult()` - Validate result structure
- `createMockCommandExecution()` - Create mock command with delay
- `testCommandErrorHandling()` - Test error scenarios
- `assertCommandResult()` - Assert result matches expectations
- `testCommandPerformance()` - Measure execution time

**From `MockUtils.ts`**:
- `createMockContext()` - Mock context
- `MOCK_BROWSER_ENV` - Mock browser environment info
- `MOCK_SERVER_ENV` - Mock server environment info
- `createMockPayload()` - Create mock payload
- `MOCK_ERROR_SCENARIOS` - Common error scenarios

**From `ClientTestUtils.ts`**:
- `testClientConnection()` - Test client connection
- `testClientCommandExecution()` - Execute command via client
- `validateConnectionResult()` - Validate connection
- `assertConnectionResult()` - Assert connection expectations

---

## For Personas

**The Power**: PersonaUsers can generate commands with built-in tests, ensuring their self-created tools are reliable.

### Persona Workflow

```typescript
// 1. Persona generates command spec
const spec = {
  name: 'helper-ai/analyze-complexity',
  description: 'Analyze code complexity metrics',
  params: [
    { name: 'filePath', type: 'string', optional: false }
  ],
  results: [
    { name: 'complexity', type: 'number' }
  ],
  accessLevel: 'ai-private'
};

// 2. Generate command with tests
await Commands.execute('generate', {
  spec,
  outputDir: '/tmp/helper-ai-analyze-complexity'
});

// 3. Persona fills in tests (TDD)
// Edit: /tmp/helper-ai-analyze-complexity/test/unit/...
// Add test cases for required param validation

// 4. Persona implements command
// Edit: /tmp/helper-ai-analyze-complexity/server/...
// Implement logic to pass tests

// 5. Persona runs unit tests
const unitTestResult = await runUnitTest('/tmp/helper-ai-analyze-complexity/test/unit/...');
if (!unitTestResult.success) {
  throw new Error('Unit tests failed - not integrating');
}

// 6. Integrate to persona commands directory
await integrateToPAth(
  '/tmp/helper-ai-analyze-complexity',
  'system/user/server/personas/helper-ai/commands/analyze-complexity'
);

// 7. Persona runs integration tests
const integrationResult = await runIntegrationTest('helper-ai/analyze-complexity');
if (!integrationResult.success) {
  throw new Error('Integration tests failed - rolling back');
}

// 8. Success! Command is proven to work
```

**Result**: Personas develop with the same rigor as human developers. Tests prove their tools work before integration.

---

## Future: Daemons and Widgets

The same TDD template approach will extend to daemons and widgets:

### Daemon Tests

```
daemons/my-daemon/
├── shared/MyDaemon.ts
├── server/MyDaemonServer.ts
├── browser/MyDaemonBrowser.ts
├── test/
│   ├── unit/MyDaemon.test.ts         # Mock daemon behavior
│   └── integration/MyDaemonIntegration.test.ts  # Live daemon
└── README.md
```

### Widget Tests

```
widgets/my-widget/
├── MyWidget.ts
├── test/
│   ├── unit/MyWidget.test.ts         # Shadow DOM, mock interactions
│   └── integration/MyWidgetIntegration.test.ts  # Browser rendering
└── README.md
```

**Vision**: Every component type has TDD baked into its generator template.

---

## Summary: Why This Matters

### For Developers
- **Zero friction** - Tests are already there
- **Perfect patterns** - Learn by example
- **Safety net** - Catch regressions early
- **Confidence** - Refactor without fear

### For Personas
- **Self-validation** - Prove tools work before integration
- **Learning** - Understand testing through examples
- **Reliability** - Build trustworthy tools
- **Autonomy** - Don't need human QA

### For the System
- **Quality** - Every component is tested
- **Maintainability** - Tests document behavior
- **Stability** - Regressions caught immediately
- **Velocity** - Move fast without breaking things

**The Bottom Line**: TDD in templates means tested code is the default, not the exception. Every generated command, daemon, and widget comes with tests ready to go. No excuses, no friction, just quality.

---

**See Also:**
- [ZERO-DOWNTIME-DEVELOPMENT.md](ZERO-DOWNTIME-DEVELOPMENT.md) - Build-test-integrate workflow
- [UNIFIED-GENERATION-SYSTEM.md](UNIFIED-GENERATION-SYSTEM.md) - Generator architecture
- [CLAUDE.md](../CLAUDE.md) - Development workflow

**Last Updated:** 2025-12-06
