# Generator Next Steps: Environment-Aware Testing

**Status**: Foundation complete, need environment-aware template selection

---

## ✅ What's Done

### 1. Core Generator Works
- ✅ Generates command files from CommandSpec JSON
- ✅ Creates shared/browser/server files
- ✅ Creates unit and integration test templates
- ✅ Creates README with TOC

### 2. Conflict Handling
- ✅ Detects existing commands
- ✅ `--force` flag to overwrite
- ✅ `--backup` flag creates timestamped backup
- ✅ Helpful error messages with options

### 3. CommandSpec Enhanced
- ✅ Added `environment?: 'server' | 'browser' | 'both'` field
- ✅ Backward compatible with existing `implementation` field

### 4. Working Reference Example
- ✅ Hello command demonstrates best practices
- ✅ Unit tests are self-contained (no external dependencies)
- ✅ All tests pass (6/6 green)
- ✅ ValidationError for missing params
- ✅ Tool-agnostic error messages
- ✅ Optional params with defaults

### 5. Documentation
- ✅ [ENVIRONMENT-AWARE-TESTING.md](ENVIRONMENT-AWARE-TESTING.md) - Complete architecture
- ✅ [TDD-IN-TEMPLATES.md](TDD-IN-TEMPLATES.md) - TDD workflow
- ✅ [TDD-TRUST-MODEL.md](TDD-TRUST-MODEL.md) - Tests as proof of safety
- ✅ [ZERO-DOWNTIME-DEVELOPMENT.md](ZERO-DOWNTIME-DEVELOPMENT.md) - Build-test-integrate

---

## 🎯 What's Needed Now

### Priority 1: Environment-Aware Template Selection

**Problem**: Currently all commands get the same test templates, regardless of environment.

**Need**: Generator should select different templates based on `environment` field:

```typescript
// Current (all commands get same templates):
const rendered = TemplateLoader.renderCommand(spec);
// Always returns: { unitTest, integrationTest }

// Needed (environment-specific templates):
const rendered = TemplateLoader.renderCommand(spec);
// Returns different templates based on spec.environment:
// - 'server': server-focused templates
// - 'browser': browser-focused templates
// - 'both': universal templates
```

**Templates Needed**:

1. **Server-Focused Unit Test** (`unit-test-server.template.ts`)
   - Mock Node.js APIs (fs, path, etc.)
   - Test server-side logic
   - Run instructions: "No server required"

2. **Server-Focused Integration Test** (`integration-test-server.template.ts`)
   - Connect via WebSocket
   - Test server execution
   - Run instructions: "npm start required"

3. **Browser-Focused Unit Test** (`unit-test-browser.template.ts`)
   - Mock DOM APIs (getBoundingClientRect, etc.)
   - Test coordinate calculations
   - Run instructions: "No server required"

4. **Browser-Focused Integration Test** (`integration-test-browser.template.ts`)
   - ⚠️ Requires live browser warning
   - Create actual DOM elements
   - Test html2canvas, canvas APIs
   - Run instructions: "npm start + wait 90+ seconds"

5. **Universal Unit Test** (`unit-test-universal.template.ts`)
   - Test shared logic only
   - No environment-specific APIs
   - Run instructions: "No server required"

6. **Universal Integration Test** (`integration-test-universal.template.ts`)
   - Test in both environments
   - Cross-environment consistency checks
   - Run instructions: "npm start + test both"

**Implementation Plan**:

```typescript
// In TemplateLoader.ts
static renderCommand(spec: CommandSpec): {
  sharedTypes: string;
  browser: string;
  server: string;
  readme: string;
  unitTest: string;
  integrationTest: string;
  tokens: Record<string, string>;
} {
  const tokens = TokenBuilder.buildAllTokens(spec);

  // Determine environment (default to 'server' for backward compatibility)
  const environment = spec.environment ?? spec.implementation ?? 'server';

  // Select appropriate test templates
  const unitTestTemplate = this.selectUnitTestTemplate(environment);
  const integrationTestTemplate = this.selectIntegrationTestTemplate(environment);

  return {
    sharedTypes: this.renderTemplate('command/shared-types.template.ts', tokens),
    browser: this.renderTemplate('command/browser.template.ts', tokens),
    server: this.renderTemplate('command/server.template.ts', tokens),
    readme: this.renderTemplate('command/README.template.md', tokens),
    unitTest: this.renderTemplate(unitTestTemplate, tokens),
    integrationTest: this.renderTemplate(integrationTestTemplate, tokens),
    tokens
  };
}

private static selectUnitTestTemplate(environment: string): string {
  switch (environment) {
    case 'server': return 'command/unit-test-server.template.ts';
    case 'browser': return 'command/unit-test-browser.template.ts';
    case 'both': return 'command/unit-test-universal.template.ts';
    default: return 'command/unit-test.template.ts'; // Fallback
  }
}

private static selectIntegrationTestTemplate(environment: string): string {
  switch (environment) {
    case 'server': return 'command/integration-test-server.template.ts';
    case 'browser': return 'command/integration-test-browser.template.ts';
    case 'both': return 'command/integration-test-universal.template.ts';
    default: return 'command/integration-test.template.ts'; // Fallback
  }
}
```

---

### Priority 2: Browser Integration Test Template

**Problem**: Screenshot command requires live browser with html2canvas.

**Need**: Template that demonstrates browser testing patterns.

**Key Patterns to Show**:

1. **Clear warnings** about browser requirement:
```typescript
/**
 * ⚠️  CRITICAL: These tests require a LIVE BROWSER with deployed code!
 *
 * RUN INSTRUCTIONS:
 * ================
 * 1. Deploy to browser:
 *    npm start
 *    # Wait 90+ seconds for full deployment
 *
 * 2. Run tests:
 *    npx tsx commands/{{COMMAND_NAME}}/test/integration/{{CLASS_NAME}}Integration.test.ts
 */
```

2. **DOM element creation**:
```typescript
// Create test element in actual DOM
const testDiv = document.createElement('div');
testDiv.id = 'test-element';
testDiv.style.cssText = 'width:100px;height:200px;background:red;';
document.body.appendChild(testDiv);

try {
  // Execute command that uses html2canvas
  const result = await client.executeCommand('screenshot', {
    querySelector: '#test-element'
  });

  assert(result.imageData.startsWith('data:image/png'));

} finally {
  // Always cleanup
  document.body.removeChild(testDiv);
}
```

3. **Shadow DOM testing** (for widgets):
```typescript
// Create widget
const widget = document.createElement('my-widget');
document.body.appendChild(widget);

// Wait for initialization
await new Promise(resolve => setTimeout(resolve, 100));

// Access shadow root
const shadowRoot = widget.shadowRoot!;
const element = shadowRoot.querySelector('.target');

// Test widget command
const result = await client.executeCommand('widget/update', {
  widgetId: widget.id,
  state: { value: 'test' }
});

// Verify shadow DOM updated
assert(element.textContent === 'test');

// Cleanup
document.body.removeChild(widget);
```

**Template Location**: `generator/templates/command/integration-test-browser.template.ts`

**Must Include**:
- ⚠️ Browser requirement warnings
- Clear run instructions at top
- DOM creation/cleanup patterns
- Shadow DOM examples (commented out, for widgets)
- html2canvas usage example (commented out, for screenshot-like commands)
- Explanation of why browser is required

---

### Priority 3: Smart Merge on Regeneration

**Problem**: `--force` overwrites modified files, losing implementation.

**Current Workaround**: Backup before regeneration, manually restore working files.

**Need**: Don't overwrite files that have been modified.

**Strategy Options**:

1. **Check Git Status** (preferred):
```typescript
// In CommandGenerator.generate()
if (fs.existsSync(serverPath)) {
  const isModified = await isFileModified(serverPath);
  if (isModified && !options?.force) {
    console.log(`⚠️  Skipping ${serverPath} (has modifications)`);
    console.log(`   Use --force to overwrite anyway`);
    return; // Skip this file
  }
}

async function isFileModified(filepath: string): Promise<boolean> {
  // Check if file has uncommitted changes
  const result = await exec(`git diff --name-only ${filepath}`);
  return result.stdout.trim().length > 0;
}
```

2. **Timestamp Check** (fallback if no git):
```typescript
// Compare template timestamp vs file timestamp
const templateTime = fs.statSync(templatePath).mtime;
const fileTime = fs.existsSync(filePath) ? fs.statSync(filePath).mtime : 0;

if (fileTime > templateTime) {
  // File was modified after template last changed
  console.log(`⚠️  Skipping ${filePath} (modified since template)`);
}
```

3. **Marker Comments** (most reliable):
```typescript
// Add marker to generated files
const generatedMarker = '// GENERATED - DO NOT EDIT ABOVE THIS LINE';

// When regenerating, only replace content above marker
// Preserve content below marker (custom implementation)
```

**Preferred Approach**: Git status check + marker comments.

**Files to Protect**:
- `server/{{CLASS_NAME}}ServerCommand.ts` - Implementation often customized
- `test/unit/{{CLASS_NAME}}Command.test.ts` - Tests often enhanced
- `test/integration/{{CLASS_NAME}}Integration.test.ts` - Tests often enhanced

**Files Safe to Overwrite**:
- `shared/{{CLASS_NAME}}Types.ts` - Usually just interfaces
- `browser/{{CLASS_NAME}}BrowserCommand.ts` - Usually just delegation
- `README.md` - Can be regenerated (but maybe prompt user?)

---

### Priority 4: NPM Test Automation

**Problem**: Running tests requires manual steps (npm start, wait, run test).

**Need**: Automated test runner that handles deployment + testing.

**Scripts to Add** (`package.json`):

```json
{
  "scripts": {
    "test": "npx tsx scripts/run-all-tests.ts",
    "test:unit": "npx tsx scripts/run-unit-tests.ts",
    "test:integration": "npm run system:ensure && npx tsx scripts/run-integration-tests.ts",
    "test:command": "npm run system:ensure && npx tsx scripts/test-command.ts"
  }
}
```

**Script: `scripts/run-all-tests.ts`**:

```typescript
#!/usr/bin/env tsx
/**
 * Run all tests (unit + integration)
 *
 * Usage: npm test
 */

import { spawn } from 'child_process';
import { glob } from 'glob';

async function runAllTests() {
  console.log('🧪 Running all tests\n');

  // Find all test files
  const unitTests = await glob('commands/**/test/unit/*.test.ts');
  const integrationTests = await glob('commands/**/test/integration/*.test.ts');

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

async function runTest(testPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    console.log(`   Running: ${testPath}`);

    const proc = spawn('npx', ['tsx', testPath], {
      stdio: 'pipe',
      cwd: process.cwd()
    });

    let output = '';
    proc.stdout.on('data', (data) => { output += data; });
    proc.stderr.on('data', (data) => { output += data; });

    proc.on('close', (code) => {
      if (code === 0) {
        console.log(`   ✅ ${testPath}`);
        resolve();
      } else {
        console.error(`   ❌ ${testPath}`);
        console.error(output);
        reject(new Error(`Test failed: ${testPath}`));
      }
    });
  });
}

async function ensureServer(): Promise<void> {
  // Check if server is running
  // If not, start it and wait for ready signal
  // Implementation depends on system:ensure script
}

runAllTests().catch(err => {
  console.error('❌ Test suite failed:', err);
  process.exit(1);
});
```

**Script: `scripts/test-command.ts`**:

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

function capitalize(str: string): string {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

testCommand(commandName).catch(err => {
  console.error(`❌ ${commandName} tests failed:`, err);
  process.exit(1);
});
```

---

### Priority 5: Update Test Template Headers

**Problem**: Current templates don't have clear run instructions.

**Need**: Every test file should have this at the top:

**Server-Focused**:
```typescript
#!/usr/bin/env tsx
/**
 * {{CLASS_NAME}} Command Unit Tests
 *
 * ENVIRONMENT: Server-focused
 *
 * RUN INSTRUCTIONS:
 * ================
 * Unit tests (fast, no server):
 *   npx tsx commands/{{COMMAND_NAME}}/test/unit/{{CLASS_NAME}}Command.test.ts
 *
 * Integration tests (requires server):
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/{{COMMAND_NAME}}/test/integration/{{CLASS_NAME}}Integration.test.ts
 *
 * Or use npm scripts:
 *   npm run test:command -- {{COMMAND_NAME}}
 */
```

**Browser-Focused**:
```typescript
#!/usr/bin/env tsx
/**
 * {{CLASS_NAME}} Integration Tests
 *
 * ENVIRONMENT: Browser-focused
 *
 * RUN INSTRUCTIONS:
 * ================
 * ⚠️  CRITICAL: These tests require a LIVE BROWSER!
 *
 * 1. Deploy to browser:
 *    npm start
 *    # Wait 90+ seconds for full deployment
 *
 * 2. Run tests:
 *    npx tsx commands/{{COMMAND_NAME}}/test/integration/{{CLASS_NAME}}Integration.test.ts
 *
 * WHY BROWSER IS REQUIRED:
 * - html2canvas requires real DOM
 * - Canvas API not available in Node.js
 * - Element.getBoundingClientRect() needs real layout
 */
```

**Universal**:
```typescript
#!/usr/bin/env tsx
/**
 * {{CLASS_NAME}} Command Tests
 *
 * ENVIRONMENT: Universal (works in both)
 *
 * RUN INSTRUCTIONS:
 * ================
 * Unit tests (fast):
 *   npx tsx commands/{{COMMAND_NAME}}/test/unit/{{CLASS_NAME}}Command.test.ts
 *
 * Integration tests (requires server):
 *   npm start  # Wait 90+ seconds
 *   npx tsx commands/{{COMMAND_NAME}}/test/integration/{{CLASS_NAME}}Integration.test.ts
 *
 * Test both environments:
 *   npm run test:command -- {{COMMAND_NAME}}
 */
```

**Add to All Templates**:
- `unit-test-server.template.ts`
- `unit-test-browser.template.ts`
- `unit-test-universal.template.ts`
- `integration-test-server.template.ts`
- `integration-test-browser.template.ts`
- `integration-test-universal.template.ts`

---

## 📋 Summary Checklist

### Must Have (Priority 1):
- [ ] Create 6 environment-specific test templates
- [ ] Update `TemplateLoader.renderCommand()` to select templates based on `environment`
- [ ] Update `TokenBuilder.buildAllTokens()` to include environment tokens
- [ ] Test with hello (environment: 'both')
- [ ] Create screenshot spec with environment: 'browser'
- [ ] Generate screenshot and verify browser tests have warnings

### Should Have (Priority 2):
- [ ] Smart merge: Check git status before overwriting
- [ ] Smart merge: Add marker comments to protect custom code
- [ ] Prompt user when overwriting modified files
- [ ] Create `scripts/run-all-tests.ts`
- [ ] Create `scripts/test-command.ts`
- [ ] Add npm test scripts to package.json

### Nice to Have (Priority 3):
- [ ] Widget testing examples in browser template
- [ ] Shadow DOM manipulation patterns
- [ ] Performance testing helpers
- [ ] Coverage reporting
- [ ] Test result summary dashboard

---

## 🎯 Next Command

Once environment-aware templates are working, test with:

```bash
# Create screenshot spec with environment: 'browser'
cat > /tmp/screenshot-spec.json <<EOF
{
  "name": "screenshot",
  "description": "Capture screenshot of DOM elements",
  "environment": "browser",
  "params": [
    { "name": "querySelector", "type": "string", "optional": true },
    { "name": "filename", "type": "string", "optional": true }
  ],
  "results": [
    { "name": "imageData", "type": "string" },
    { "name": "filepath", "type": "string" },
    { "name": "width", "type": "number" },
    { "name": "height", "type": "number" }
  ],
  "accessLevel": "ai-safe"
}
EOF

# Generate (should use browser-focused templates)
npx tsx generator/CommandGenerator.ts /tmp/screenshot-spec.json /tmp/screenshot-test

# Verify integration test has browser warnings
cat /tmp/screenshot-test/test/integration/ScreenshotIntegration.test.ts | head -30

# Should see:
# ⚠️  CRITICAL: These tests require a LIVE BROWSER!
```

---

## 💡 Key Insights from Testing

1. **Backup works** - `--force --backup` creates timestamped backup successfully
2. **Manual restore needed** - Currently need to manually restore modified files from backup
3. **Tests are portable** - Unit tests work without any dependencies
4. **Environment matters** - Screenshot needs browser, hello works everywhere
5. **Clear instructions essential** - Every test file needs run instructions at top

---

**Last Updated:** 2025-12-06
**Status:** Ready for Priority 1 implementation
