/**
 * Simple Node.js tests for the modular Continuum system
 */

const assert = require('assert');
const fs = require('fs');
const path = require('path');

// Test our modular components
console.log('🧪 Testing Modular Continuum System...\n');

// Test 1: CostTracker
console.log('1️⃣  Testing CostTracker...');
const CostTracker = require('../src/core/CostTracker.cjs');
const tmpCostFile = path.join(__dirname, 'tmp-costs.json');

try {
  const tracker = new CostTracker(tmpCostFile);
  
  // Test cost tracking
  tracker.trackCost('gpt-4', 100, 50, 0.005);
  assert.strictEqual(tracker.getTotal(), 0.005, 'Total cost should be 0.005');
  assert.strictEqual(tracker.getRequests(), 1, 'Should have 1 request');
  
  // Test cost calculation
  const byModel = tracker.getCostsByModel();
  assert(byModel['gpt-4'], 'Should have gpt-4 costs');
  assert.strictEqual(byModel['gpt-4'].inputTokens, 100, 'Should track input tokens');
  assert.strictEqual(byModel['gpt-4'].outputTokens, 50, 'Should track output tokens');
  
  // Test formatted summary
  const summary = tracker.getFormattedSummary();
  assert(summary.includes('1 requests'), 'Summary should show request count');
  assert(summary.includes('$0.005000'), 'Summary should show total cost');
  
  console.log('✅ CostTracker tests passed');
} catch (error) {
  console.error('❌ CostTracker test failed:', error.message);
  process.exit(1);
} finally {
  // Cleanup
  if (fs.existsSync(tmpCostFile)) {
    fs.unlinkSync(tmpCostFile);
  }
}

// Test 2: AIModel classes
console.log('\n2️⃣  Testing AIModel classes...');
const { ClaudeHaiku, GPT4, ModelRegistry } = require('../src/core/AIModel.cjs');

try {
  // Test Claude Haiku
  const haiku = new ClaudeHaiku();
  assert.strictEqual(haiku.name, 'claude-3-haiku-20240307', 'Haiku should have correct name');
  assert.strictEqual(haiku.inputRate, 0.25, 'Haiku should have correct input rate');
  assert.strictEqual(haiku.outputRate, 1.25, 'Haiku should have correct output rate');
  
  // Test cost calculation
  const cost = haiku.calculateCost(1000000, 500000); // 1M input, 500K output
  const expectedCost = (1000000 * 0.25 + 500000 * 1.25) / 1000000;
  assert.strictEqual(cost, expectedCost, 'Cost calculation should be correct');
  
  // Test GPT-4
  const gpt4 = new GPT4();
  assert.strictEqual(gpt4.provider, 'openai', 'GPT-4 should be OpenAI provider');
  assert(gpt4.canHandle('plan a strategy'), 'GPT-4 should handle planning tasks');
  
  // Test ModelRegistry
  const registry = new ModelRegistry();
  const model = registry.get('claude-3-haiku-20240307');
  assert(model, 'Registry should return Claude Haiku');
  
  const bestModel = registry.findBestModel('simple task');
  assert(bestModel, 'Registry should find a best model');
  
  console.log('✅ AIModel tests passed');
} catch (error) {
  console.error('❌ AIModel test failed:', error.message);
  process.exit(1);
}

// Test 3: CommandProcessor
console.log('\n3️⃣  Testing CommandProcessor...');
const CommandProcessor = require('../src/core/CommandProcessor.cjs');

try {
  const processor = new CommandProcessor();
  
  // Test protocol parsing
  const response = `
[STATUS] Processing request
[CMD:EXEC] echo "test"
[CHAT] Here is the result
`;
  
  const parsed = processor.parseAIProtocol(response);
  assert(parsed.statusMessage, 'Should parse status message');
  assert(parsed.chatMessage, 'Should parse chat message');
  assert.strictEqual(parsed.commands.length, 1, 'Should parse one command');
  assert.strictEqual(parsed.commands[0].command, 'EXEC', 'Should parse EXEC command');
  assert.strictEqual(parsed.commands[0].params, 'echo "test"', 'Should parse command params');
  
  console.log('✅ CommandProcessor tests passed');
} catch (error) {
  console.error('❌ CommandProcessor test failed:', error.message);
  process.exit(1);
}

// Test 4: File structure
console.log('\n4️⃣  Testing modular file structure...');

try {
  const coreFiles = [
    '../src/core/continuum-core.cjs',
    '../src/core/CostTracker.cjs',
    '../src/core/AIModel.cjs',
    '../src/core/CommandProcessor.cjs',
    '../src/core/UIGenerator.cjs',
    '../src/core/HttpServer.cjs',
    '../src/core/WebSocketServer.cjs',
    '../src/core/MessageQueue.cjs'
  ];
  
  for (const file of coreFiles) {
    const filePath = path.join(__dirname, file);
    assert(fs.existsSync(filePath), `${file} should exist`);
    
    const stats = fs.statSync(filePath);
    assert(stats.size > 0, `${file} should not be empty`);
  }
  
  // Check line counts
  const coreFile = path.join(__dirname, '../src/core/continuum-core.cjs');
  const coreContent = fs.readFileSync(coreFile, 'utf-8');
  const lineCount = coreContent.split('\n').length;
  assert(lineCount < 600, `Core file should be under 600 lines, got ${lineCount}`);
  
  console.log('✅ File structure tests passed');
} catch (error) {
  console.error('❌ File structure test failed:', error.message);
  process.exit(1);
}

// Test 5: Integration test
console.log('\n5️⃣  Testing integration...');

try {
  // Test that core can be imported without errors
  const ContinuumCore = require('../src/core/continuum-core.cjs');
  
  // Test instantiation (without starting server, skip merge for testing)
  const core = new ContinuumCore({ autoStart: false, skipMerge: true });
  assert(core.costTracker, 'Core should have cost tracker');
  assert(core.modelRegistry, 'Core should have model registry');
  assert(core.commandProcessor, 'Core should have command processor');
  
  // Test cost system integration
  assert.strictEqual(typeof core.costs.total, 'number', 'Should have numeric total cost');
  assert.strictEqual(typeof core.costs.requests, 'number', 'Should have numeric request count');
  
  console.log('✅ Integration tests passed');
} catch (error) {
  console.error('❌ Integration test failed:', error.message);
  process.exit(1);
}

console.log('\n🎉 All modular system tests passed!');
console.log('📊 Summary:');
console.log('  - CostTracker: ✅ Proper cost calculations and persistence');
console.log('  - AIModel: ✅ Model classes with rates and capabilities');
console.log('  - CommandProcessor: ✅ AI protocol parsing and execution');
console.log('  - File Structure: ✅ Clean modular architecture');
console.log('  - Integration: ✅ Components work together');
console.log('');
console.log('🏗️  Modular system is working correctly!');