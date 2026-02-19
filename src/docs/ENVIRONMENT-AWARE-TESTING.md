# Environment-Aware Testing Architecture

**Commands run in different environments - tests should match their execution context.**

> "For something like screenshot, which involves the browser environment, has challenges. We should demonstrate patterns and potentially utilize our own param to the generate command, the environment focus could dictate the testing strategy: 'server', 'browser', or 'both'." - Joel

---

## Table of Contents

- [The Problem](#the-problem)
- [Three Environment Patterns](#three-environment-patterns)
- [CommandSpec Environment Field](#commandspec-environment-field)
- [Test Strategy by Environment](#test-strategy-by-environment)
- [Template Selection](#template-selection)
- [Browser Testing Patterns](#browser-testing-patterns)
- [NPM Test Automation](#npm-test-automation)
- [Run Instructions](#run-instructions)
- [Examples](#examples)

---

## The Problem

**Commands execute in different environments with different capabilities:**

1. **Server-only commands** (e.g., `data/list`) - Node.js, file system, no DOM
2. **Browser-only commands** (e.g., `screenshot`) - DOM, html2canvas, visual rendering
3. **Universal commands** (e.g., `ping`) - Work everywhere, delegate appropriately

**Tests must match the execution environment** - you can't test DOM manipulation without a browser, and you can't test file system access in a browser.

---

## Three Environment Patterns

### Pattern 1: Server-Focused Commands

**Execution**: Server does the work, browser just delegates

**Examples**:
- `data/list` - Query database (server-only)
- `data/store` - Store entity (server-only)
- `compile-typescript` - Run TypeScript compiler (Node.js only)

**Testing Strategy**:
- **Unit tests**: Mock Node.js APIs, test business logic
- **Integration tests**: Connect via WebSocket, verify server execution
- **Browser side**: Minimal (just delegation)

### Pattern 2: Browser-Focused Commands

**Execution**: Browser does the work, may send result to server for storage

**Examples**:
- `screenshot` - html2canvas capture (browser DOM required)
- `click` - Simulate DOM click event (browser required)
- Widget commands - Manipulate shadow DOM (browser required)

**Testing Strategy**:
- **Unit tests**: Mock DOM APIs, test coordinate calculations
- **Integration tests**: **Require live browser**, test actual DOM capture
- **Server side**: Receives and stores result

### Pattern 3: Universal Commands

**Execution**: Logic is environment-agnostic, each environment implements differently

**Examples**:
- `ping` - Collects environment info (different per environment)
- `hello` - Simple logic (works anywhere)
- `help` - Reads documentation (file system or fetch)

**Testing Strategy**:
- **Unit tests**: Test shared logic with mocks
- **Integration tests**: Test both environments
- **Cross-environment**: Verify consistency

---

## CommandSpec Environment Field

Add `environment` field to CommandSpec to dictate testing strategy:

```typescript
interface CommandSpec {
  name: string;
  description: string;
  params: ParamSpec[];
  results: ResultSpec[];
  accessLevel: string;
  environment?: 'server' | 'browser' | 'both'; // NEW!
  examples?: ExampleSpec[];
}
```

### Usage in Generator

```json
{
  "name": "screenshot",
  "description": "Capture screenshot of DOM elements",
  "environment": "browser",
  "params": [
    { "name": "querySelector", "type": "string", "optional": true }
  ],
  "results": [
    { "name": "imageData", "type": "string" },
    { "name": "filepath", "type": "string" }
  ],
  "accessLevel": "ai-safe"
}
```

Generator uses `environment` to:
1. Select appropriate test template (browser tests need DOM)
2. Generate environment-specific instructions
3. Include environment-specific test utilities

---

## Test Strategy by Environment

### Server-Focused (`environment: 'server'`)

**Unit Tests**:
```typescript
// Test server logic with Node.js mocks
import * as fs from 'fs';
import { mockFs } from 'mock-fs';

// Test file system operations
mockFs({
  '/tmp/test.txt': 'content'
});

const result = await command.execute({ filepath: '/tmp/test.txt' });
assert(result.content === 'content');
```

**Integration Tests**:
```typescript
// Connect via WebSocket, verify server execution
const { jtag } = await import('../../../../browser-index');
const client = await jtag.connect();

const result = await client.executeCommand('data/list', {
  collection: 'users'
});

assert(result.success === true);
assert(Array.isArray(result.entities));
```

### Browser-Focused (`environment: 'browser'`)

**Unit Tests**:
```typescript
// Test DOM calculations with mocks
const mockElement = {
  getBoundingClientRect: () => ({ x: 10, y: 20, width: 100, height: 200 }),
  offsetWidth: 100,
  offsetHeight: 200
};

const coords = calculateCropCoordinates(mockElement);
assert(coords.x === 10);
assert(coords.y === 20);
```

**Integration Tests** (CRITICAL - requires live browser):
```typescript
// PREREQUISITES: npm start (deploys code to browser)
// Browser must be running with actual DOM

const { jtag } = await import('../../../../browser-index');
const client = await jtag.connect();

// Create test element in DOM
document.body.innerHTML = '<div id="test" style="width:100px;height:200px">Test</div>';

// Execute screenshot command (uses html2canvas)
const result = await client.executeCommand('screenshot', {
  querySelector: '#test'
});

assert(result.success === true);
assert(result.imageData.startsWith('data:image/png'));
assert(result.width === 100);
assert(result.height === 200);
```

### Universal (`environment: 'both'`)

**Unit Tests**:
```typescript
// Test shared logic
const result = await mockCommand({ name: 'test' });
assert(result.greeting.includes('test'));
```

**Integration Tests**:
```typescript
// Test in both environments
const { jtag } = await import('../../../../browser-index');
const client = await jtag.connect();

// Test browser version
const browserResult = await client.executeCommand('hello', { name: 'Browser' });
assert(browserResult.environment === 'browser');

// Test server version (via server-index)
const { jtag: serverJtag } = await import('../../../../server-index');
const serverClient = await serverJtag.connect();

const serverResult = await serverClient.executeCommand('hello', { name: 'Server' });
assert(serverResult.environment === 'server');
```

---

## Template Selection

Generator selects test templates based on `environment`:

### Server-Focused Template

```typescript
// test/unit/MyCommandCommand.test.ts
/**
 * RUN INSTRUCTIONS:
 *
 * Unit tests (no server required):
 *   npx tsx commands/my-command/test/unit/MyCommandCommand.test.ts
 *
 * Integration tests (server must be running):
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
 */

// Mock Node.js APIs
import * as fs from 'fs';

// Test server-side logic
```

### Browser-Focused Template

```typescript
// test/integration/MyCommandIntegration.test.ts
/**
 * RUN INSTRUCTIONS:
 *
 * ⚠️  BROWSER-FOCUSED COMMAND - Integration tests require live browser
 *
 * 1. Deploy to browser:
 *    npm start  # Wait 90+ seconds for deployment
 *
 * 2. Run integration tests:
 *    npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
 *
 * Unit tests (mock DOM):
 *    npx tsx commands/my-command/test/unit/MyCommandCommand.test.ts
 *
 * NOTE: Integration tests create actual DOM elements and use html2canvas.
 * They CANNOT run without a live browser environment.
 */

// Test with actual DOM
const testElement = document.createElement('div');
document.body.appendChild(testElement);
```

### Universal Template

```typescript
// test/integration/MyCommandIntegration.test.ts
/**
 * RUN INSTRUCTIONS:
 *
 * ⚠️  UNIVERSAL COMMAND - Tests run in both environments
 *
 * 1. Deploy to both:
 *    npm start  # Wait 90+ seconds
 *
 * 2. Run cross-environment tests:
 *    npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
 *
 * This will test the command in BOTH browser and server environments.
 */

// Test in both environments
```

---

## Browser Testing Patterns

### Pattern: Screenshot Command

**The Challenge**: Screenshot uses `html2canvas` which requires a real browser DOM.

**Unit Tests** (Mock DOM calculations):
```typescript
#!/usr/bin/env tsx
/**
 * Screenshot Command Unit Tests
 *
 * RUN: npx tsx commands/screenshot/test/unit/ScreenshotCommand.test.ts
 *
 * Tests coordinate calculations and DOM utilities without requiring html2canvas.
 */

import { calculateCropCoordinates } from '../../shared/browser-utils/BrowserElementUtils';

function testCoordinateCalculation() {
  // Mock element with getBoundingClientRect
  const mockElement = {
    getBoundingClientRect: () => ({
      x: 100,
      y: 200,
      width: 300,
      height: 400,
      top: 200,
      left: 100,
      bottom: 600,
      right: 400
    })
  } as Element;

  const coords = calculateCropCoordinates(mockElement);

  assert(coords.x === 100, 'X coordinate correct');
  assert(coords.y === 200, 'Y coordinate correct');
  assert(coords.width === 300, 'Width correct');
  assert(coords.height === 400, 'Height correct');
}
```

**Integration Tests** (Real browser, real html2canvas):
```typescript
#!/usr/bin/env tsx
/**
 * Screenshot Command Integration Tests
 *
 * ⚠️  REQUIRES LIVE BROWSER - Deploy first!
 *
 * RUN:
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/screenshot/test/integration/ScreenshotIntegration.test.ts
 *
 * Tests actual screenshot capture using html2canvas in live browser.
 */

import { jtag } from '../../../../browser-index';

async function testRealScreenshotCapture() {
  // Connect to live browser
  const client = await jtag.connect();

  // Create test element in actual DOM
  const testDiv = document.createElement('div');
  testDiv.id = 'screenshot-test';
  testDiv.style.cssText = 'width:100px;height:200px;background:red;';
  testDiv.textContent = 'Test Element';
  document.body.appendChild(testDiv);

  try {
    // Execute screenshot command (uses real html2canvas)
    const result = await client.executeCommand('screenshot', {
      querySelector: '#screenshot-test'
    });

    // Verify result
    assert(result.success === true, 'Screenshot captured');
    assert(result.imageData.startsWith('data:image/png'), 'Got PNG data');
    assert(result.width === 100, 'Width correct');
    assert(result.height === 200, 'Height correct');

    // Verify image was saved to agent memory (if agent context)
    if (result.filepath) {
      assert(result.filepath.endsWith('.png'), 'Filepath has .png extension');
    }

  } finally {
    // Cleanup
    document.body.removeChild(testDiv);
  }
}
```

### Pattern: Widget Commands

Widgets follow the same pattern:

**Unit Tests**: Mock shadow DOM, test state management
**Integration Tests**: Create actual widget, test rendering

```typescript
// Integration test for widget command
async function testWidgetCommand() {
  // Create actual widget
  const widget = document.createElement('my-widget');
  document.body.appendChild(widget);

  // Wait for widget to initialize
  await new Promise(resolve => setTimeout(resolve, 100));

  // Execute widget command
  const result = await client.executeCommand('widget/update', {
    widgetId: widget.id,
    state: { value: 'test' }
  });

  // Verify widget state changed
  const shadowRoot = widget.shadowRoot!;
  const display = shadowRoot.querySelector('.display');
  assert(display.textContent === 'test', 'Widget updated');

  // Cleanup
  document.body.removeChild(widget);
}
```

---

## NPM Test Automation

Create `npm test` script that handles deployment + testing:

### package.json Scripts

```json
{
  "scripts": {
    "test": "npm run test:deploy && npm run test:run",
    "test:deploy": "npm start",
    "test:run": "npx tsx scripts/run-all-tests.ts",
    "test:unit": "npx tsx scripts/run-unit-tests.ts",
    "test:integration": "npm run system:ensure && npx tsx scripts/run-integration-tests.ts",
    "test:command": "npm run system:ensure && npx tsx scripts/test-command.ts"
  }
}
```

### scripts/run-all-tests.ts

```typescript
#!/usr/bin/env tsx
/**
 * Run all tests (unit + integration)
 *
 * Usage: npm test
 */

import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

async function runAllTests() {
  console.log('🧪 Running all tests\n');

  // Find all test files
  const unitTests = findTests('commands', 'test/unit');
  const integrationTests = findTests('commands', 'test/integration');

  console.log(`Found ${unitTests.length} unit tests`);
  console.log(`Found ${integrationTests.length} integration tests\n`);

  // Run unit tests (fast, no server)
  console.log('📋 Running unit tests...\n');
  for (const test of unitTests) {
    await runTest(test);
  }

  // Ensure server is running
  console.log('\n🚀 Ensuring server is running...');
  await ensureServer();

  // Run integration tests (slow, requires server)
  console.log('\n⚡ Running integration tests...\n');
  for (const test of integrationTests) {
    await runTest(test);
  }

  console.log('\n🎉 All tests complete!');
}

function findTests(baseDir: string, testDir: string): string[] {
  const tests: string[] = [];
  // Walk commands directory, find test files
  // ... implementation
  return tests;
}

async function runTest(testPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const proc = spawn('npx', ['tsx', testPath], {
      stdio: 'inherit',
      cwd: process.cwd()
    });

    proc.on('close', (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Test failed: ${testPath}`));
      }
    });
  });
}

async function ensureServer(): Promise<void> {
  // Check if server is running via signal file or ping
  // If not running, start it
  // Wait for server to be ready
}

runAllTests().catch(err => {
  console.error('❌ Test suite failed:', err);
  process.exit(1);
});
```

### scripts/test-command.ts

```typescript
#!/usr/bin/env tsx
/**
 * Test a specific command
 *
 * Usage: npm run test:command -- hello
 */

const commandName = process.argv[2];

if (!commandName) {
  console.error('Usage: npm run test:command -- <command-name>');
  process.exit(1);
}

async function testCommand(name: string) {
  console.log(`🧪 Testing command: ${name}\n`);

  const unitTest = `commands/${name}/test/unit/${capitalize(name)}Command.test.ts`;
  const integrationTest = `commands/${name}/test/integration/${capitalize(name)}Integration.test.ts`;

  // Run unit tests
  if (fs.existsSync(unitTest)) {
    console.log('📋 Running unit tests...');
    await runTest(unitTest);
  }

  // Ensure server
  console.log('\n🚀 Ensuring server is running...');
  await ensureServer();

  // Run integration tests
  if (fs.existsSync(integrationTest)) {
    console.log('\n⚡ Running integration tests...');
    await runTest(integrationTest);
  }

  console.log(`\n🎉 ${name} tests complete!`);
}

testCommand(commandName).catch(err => {
  console.error(`❌ ${commandName} tests failed:`, err);
  process.exit(1);
});
```

---

## Run Instructions

### Template Header Format

Every test file should have clear run instructions at the top:

#### Server-Focused Command

```typescript
#!/usr/bin/env tsx
/**
 * MyCommand Unit Tests
 *
 * ENVIRONMENT: Server-focused
 *
 * RUN INSTRUCTIONS:
 * ================
 *
 * Unit tests (fast, no server):
 *   npx tsx commands/my-command/test/unit/MyCommandCommand.test.ts
 *
 * Integration tests (requires server):
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/my-command/test/integration/MyCommandIntegration.test.ts
 *
 * Or use npm scripts:
 *   npm run test:command -- my-command
 *
 * WHAT'S TESTED:
 * =============
 * - Server-side logic (Node.js APIs)
 * - File system operations
 * - Database queries
 * - Parameter validation
 */
```

#### Browser-Focused Command

```typescript
#!/usr/bin/env tsx
/**
 * Screenshot Integration Tests
 *
 * ENVIRONMENT: Browser-focused
 *
 * RUN INSTRUCTIONS:
 * ================
 *
 * ⚠️  CRITICAL: These tests require a LIVE BROWSER with deployed code!
 *
 * 1. Deploy to browser:
 *    npm start
 *    # Wait 90+ seconds for full deployment
 *
 * 2. Run tests:
 *    npx tsx commands/screenshot/test/integration/ScreenshotIntegration.test.ts
 *
 * Or use npm scripts:
 *    npm run test:command -- screenshot
 *
 * WHAT'S TESTED:
 * =============
 * - Real html2canvas capture
 * - Actual DOM element selection
 * - Image data generation
 * - File saving to agent memory
 * - Coordinate calculations on live elements
 *
 * WHY BROWSER IS REQUIRED:
 * =======================
 * - html2canvas requires real DOM
 * - Canvas API not available in Node.js
 * - Element.getBoundingClientRect() needs real layout
 * - Shadow DOM manipulation needs real browser
 */
```

#### Universal Command

```typescript
#!/usr/bin/env tsx
/**
 * Hello Command Tests
 *
 * ENVIRONMENT: Universal (works in both)
 *
 * RUN INSTRUCTIONS:
 * ================
 *
 * Unit tests (fast, no server):
 *   npx tsx commands/hello/test/unit/HelloCommand.test.ts
 *
 * Integration tests (requires server):
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/hello/test/integration/HelloIntegration.test.ts
 *
 * Test both environments:
 *   npm run test:command -- hello
 *
 * WHAT'S TESTED:
 * =============
 * - Command logic (environment-agnostic)
 * - Parameter validation
 * - Result structure
 * - Browser execution (via WebSocket)
 * - Server execution (via server client)
 * - Cross-environment consistency
 */
```

---

## Examples

### Example 1: Generating Server-Focused Command

```bash
# Create spec with server environment
cat > /tmp/data-export-spec.json <<EOF
{
  "name": "data/export",
  "description": "Export database collection to JSON file",
  "environment": "server",
  "params": [
    { "name": "collection", "type": "string", "optional": false },
    { "name": "filepath", "type": "string", "optional": false }
  ],
  "results": [
    { "name": "exported", "type": "number" },
    { "name": "filepath", "type": "string" }
  ],
  "accessLevel": "ai-safe"
}
EOF

# Generate with server-focused templates
./jtag generate --spec=/tmp/data-export-spec.json

# Generated tests include:
# - Unit tests with Node.js fs mocks
# - Integration tests with WebSocket connection
# - Clear instructions: "npm start" required for integration
```

### Example 2: Generating Browser-Focused Command

```bash
# Create spec with browser environment
cat > /tmp/click-spec.json <<EOF
{
  "name": "click",
  "description": "Simulate click event on DOM element",
  "environment": "browser",
  "params": [
    { "name": "querySelector", "type": "string", "optional": false }
  ],
  "results": [
    { "name": "clicked", "type": "boolean" },
    { "name": "element", "type": "string" }
  ],
  "accessLevel": "ai-safe"
}
EOF

# Generate with browser-focused templates
./jtag generate --spec=/tmp/click-spec.json

# Generated tests include:
# - Unit tests with mock DOM element
# - Integration tests with WARNING about browser requirement
# - Instructions emphasize: "npm start" + "wait 90+ seconds"
# - Tests create actual DOM elements
```

### Example 3: Generating Universal Command

```bash
# Create spec with universal environment
cat > /tmp/validate-spec.json <<EOF
{
  "name": "validate",
  "description": "Validate data structure against schema",
  "environment": "both",
  "params": [
    { "name": "data", "type": "any", "optional": false },
    { "name": "schema", "type": "any", "optional": false }
  ],
  "results": [
    { "name": "valid", "type": "boolean" },
    { "name": "errors", "type": "string[]" }
  ],
  "accessLevel": "ai-safe"
}
EOF

# Generate with universal templates
./jtag generate --spec=/tmp/validate-spec.json

# Generated tests include:
# - Unit tests for shared logic
# - Integration tests for both environments
# - Cross-environment consistency checks
```

---

## Summary

**The Key Insight**: Environment dictates testing strategy.

### Server-Focused
- Unit: Mock Node.js APIs
- Integration: WebSocket connection
- Fast, straightforward

### Browser-Focused
- Unit: Mock DOM calculations
- Integration: **Requires live browser**
- Slower, more complex
- Uses actual html2canvas, actual DOM

### Universal
- Unit: Test shared logic
- Integration: Test both environments
- Verify consistency

**Template Selection**: `environment` field in CommandSpec determines which test templates to generate.

**Clear Instructions**: Every test file has run instructions at the top - no guessing!

**NPM Automation**: `npm test` handles deployment + testing automatically.

---

**See Also:**
- [TDD-IN-TEMPLATES.md](TDD-IN-TEMPLATES.md) - TDD workflow
- [ZERO-DOWNTIME-DEVELOPMENT.md](ZERO-DOWNTIME-DEVELOPMENT.md) - Build-test-integrate
- [TDD-TRUST-MODEL.md](TDD-TRUST-MODEL.md) - Tests as proof of safety

**Last Updated:** 2025-12-06
