#!/usr/bin/env node
/**
 * COMPREHENSIVE UNIT TESTS FOR TOOL EXECUTION SYSTEM
 * 
 * Tests the tool execution functionality that parses AI responses
 * and executes WEBFETCH, FILE_READ, GIT_STATUS commands
 */

const Continuum = require('./continuum.cjs');
const fs = require('fs');
const path = require('path');

class ToolExecutionTester {
  constructor() {
    this.tests = [];
    this.passed = 0;
    this.failed = 0;
    this.continuum = null;
  }

  async setup() {
    console.log('🔧 Setting up test environment...');
    this.continuum = new Continuum();
    
    // Wait for initialization
    await new Promise(resolve => setTimeout(resolve, 2000));
    console.log('✅ Test environment ready');
  }

  addTest(name, testFn) {
    this.tests.push({ name, testFn });
  }

  async runTests() {
    console.log(`🧪 Running ${this.tests.length} tool execution tests...\n`);

    for (const test of this.tests) {
      try {
        console.log(`🔍 Testing: ${test.name}`);
        await test.testFn();
        console.log(`✅ PASSED: ${test.name}\n`);
        this.passed++;
      } catch (error) {
        console.log(`❌ FAILED: ${test.name}`);
        console.log(`   Error: ${error.message}\n`);
        this.failed++;
      }
    }

    this.printSummary();
  }

  printSummary() {
    console.log('📊 TEST SUMMARY');
    console.log('===============');
    console.log(`✅ Passed: ${this.passed}`);
    console.log(`❌ Failed: ${this.failed}`);
    console.log(`📈 Success Rate: ${((this.passed / this.tests.length) * 100).toFixed(1)}%`);
    
    if (this.failed === 0) {
      console.log('\n🎉 ALL TESTS PASSED! Tool execution system is working correctly.');
    } else {
      console.log('\n⚠️  Some tests failed. Tool execution system needs fixes.');
      process.exit(1);
    }
  }

  assertEqual(actual, expected, message = '') {
    if (actual !== expected) {
      throw new Error(`Expected "${expected}", got "${actual}". ${message}`);
    }
  }

  assertTrue(condition, message = '') {
    if (!condition) {
      throw new Error(`Expected true condition. ${message}`);
    }
  }

  assertContains(text, substring, message = '') {
    if (!text.includes(substring)) {
      throw new Error(`Expected text to contain "${substring}". ${message}`);
    }
  }
}

async function runAllTests() {
  const tester = new ToolExecutionTester();
  
  await tester.setup();

  // Test 1: WebFetch command parsing
  tester.addTest('WEBFETCH command parsing', async () => {
    const response = 'I need to check something. WEBFETCH: https://httpbin.org/json for testing.';
    const results = await tester.continuum.processToolCommands(response);
    
    tester.assertEqual(results.length, 1, 'Should find 1 WebFetch command');
    tester.assertEqual(results[0].tool, 'WEBFETCH', 'Tool should be WEBFETCH');
    tester.assertEqual(results[0].command, 'https://httpbin.org/json', 'URL should match');
    tester.assertTrue(results[0].result.length > 0, 'Should have fetched content');
  });

  // Test 2: Multiple WebFetch commands
  tester.addTest('Multiple WEBFETCH commands', async () => {
    const response = `Let me check two sites:
    WEBFETCH: https://httpbin.org/json 
    and also 
    WEBFETCH: https://httpbin.org/uuid`;
    
    const results = await tester.continuum.processToolCommands(response);
    tester.assertEqual(results.length, 2, 'Should find 2 WebFetch commands');
    
    for (const result of results) {
      tester.assertEqual(result.tool, 'WEBFETCH', 'All tools should be WEBFETCH');
      tester.assertTrue(result.result.length > 0, 'All should have content');
    }
  });

  // Test 3: FILE_READ command
  tester.addTest('FILE_READ command', async () => {
    // Create a test file
    const testFile = path.join(__dirname, 'test-file.txt');
    fs.writeFileSync(testFile, 'Test file content for unit testing');
    
    try {
      const response = `Let me read the file: FILE_READ: ${testFile}`;
      const results = await tester.continuum.processToolCommands(response);
      
      tester.assertEqual(results.length, 1, 'Should find 1 FILE_READ command');
      tester.assertEqual(results[0].tool, 'FILE_READ', 'Tool should be FILE_READ');
      tester.assertContains(results[0].result, 'Test file content', 'Should read file content');
    } finally {
      // Clean up test file
      fs.unlinkSync(testFile);
    }
  });

  // Test 4: GIT_STATUS command
  tester.addTest('GIT_STATUS command', async () => {
    const response = 'Let me check git status: GIT_STATUS';
    const results = await tester.continuum.processToolCommands(response);
    
    tester.assertEqual(results.length, 1, 'Should find 1 GIT_STATUS command');
    tester.assertEqual(results[0].tool, 'GIT_STATUS', 'Tool should be GIT_STATUS');
    // Result could be empty (clean repo) or contain status
    tester.assertTrue(results[0].result !== undefined, 'Should have git status result');
  });

  // Test 5: No tool commands
  tester.addTest('No tool commands in response', async () => {
    const response = 'This is just a regular response with no tool commands.';
    const results = await tester.continuum.processToolCommands(response);
    
    tester.assertEqual(results.length, 0, 'Should find no tool commands');
  });

  // Test 6: Mixed tool commands
  tester.addTest('Mixed tool commands', async () => {
    const testFile = path.join(__dirname, 'mixed-test.txt');
    fs.writeFileSync(testFile, 'Mixed test content');
    
    try {
      const response = `I need to do several things:
      WEBFETCH: https://httpbin.org/json
      FILE_READ: ${testFile}
      GIT_STATUS`;
      
      const results = await tester.continuum.processToolCommands(response);
      
      tester.assertEqual(results.length, 3, 'Should find 3 tool commands');
      
      const tools = results.map(r => r.tool).sort();
      tester.assertEqual(tools.join(','), 'FILE_READ,GIT_STATUS,WEBFETCH', 'Should have all 3 tool types');
    } finally {
      fs.unlinkSync(testFile);
    }
  });

  // Test 7: WebFetch error handling
  tester.addTest('WebFetch error handling', async () => {
    const response = 'WEBFETCH: https://invalid-url-that-does-not-exist.fake';
    const results = await tester.continuum.processToolCommands(response);
    
    tester.assertEqual(results.length, 1, 'Should attempt to fetch even invalid URLs');
    tester.assertEqual(results[0].tool, 'WEBFETCH', 'Tool should be WEBFETCH');
    tester.assertContains(results[0].result, 'Error:', 'Should contain error message');
  });

  // Test 8: FILE_READ error handling
  tester.addTest('FILE_READ error handling', async () => {
    const response = 'FILE_READ: /this/file/does/not/exist.txt';
    const results = await tester.continuum.processToolCommands(response);
    
    tester.assertEqual(results.length, 1, 'Should attempt to read even non-existent files');
    tester.assertEqual(results[0].tool, 'FILE_READ', 'Tool should be FILE_READ');
    tester.assertContains(results[0].result, 'Error:', 'Should contain error message');
  });

  // Test 9: Command parsing edge cases
  tester.addTest('Command parsing edge cases', async () => {
    const response = `
    This has WEBFETCH: https://httpbin.org/json in the middle.
    And webfetch: https://httpbin.org/uuid should also work (case insensitive).
    But this is not a command: "I like WEBFETCH as a concept"
    `;
    
    const results = await tester.continuum.processToolCommands(response);
    tester.assertEqual(results.length, 2, 'Should find 2 valid WebFetch commands despite edge cases');
  });

  // Test 10: Integration with AI response processing
  tester.addTest('Integration with PlannerAI response', async () => {
    // Test that PlannerAI can actually use tools through the system
    const mockAIResponse = `I'll help you by checking this information:
    
    WEBFETCH: https://httpbin.org/json
    
    Based on that data, I can provide better assistance.`;
    
    const results = await tester.continuum.processToolCommands(mockAIResponse);
    
    tester.assertEqual(results.length, 1, 'Should extract tool from AI response');
    tester.assertEqual(results[0].tool, 'WEBFETCH', 'Should be WebFetch tool');
    tester.assertTrue(results[0].result.includes('slideshow'), 'Should fetch actual content');
  });

  await tester.runTests();
  process.exit(0);
}

// Run tests if called directly
if (require.main === module) {
  runAllTests().catch(error => {
    console.error('❌ Test runner failed:', error.message);
    process.exit(1);
  });
}

module.exports = ToolExecutionTester;