// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command Integration Tests
 * 
 * Tests the test command from both browser and server environments
 * Demonstrates proper success/failure result handling
 */

import { EventTestUtils } from '../../../commands/test/utils/EventTestUtils';

export async function runTestCommandIntegrationTests(): Promise<void> {
  console.log('🧪 TEST COMMAND INTEGRATION TESTS');
  console.log('════════════════════════════════════════════════════════════');

  // Test 1: Server-side test execution with simple passing test
  await EventTestUtils.runTestWithCleanup(
    'Server Test Command - Simple Passing Test',
    async () => {
      const { ServerCommandTester } = await import('../../../commands/test/utils/ServerCommandTester');
      const tester = new ServerCommandTester();

      const result = await tester.executeCommand('test', {
        _: ['test-simple.ts']
      });

      console.log('📊 Server test result:', {
        success: result.success,
        command: result.command,
        duration: result.duration,
        outputLength: result.output.length
      });

      // Should succeed - test-simple.ts is a passing test
      if (!result.success) {
        throw new Error(`Server test command should succeed but failed: ${result.output}`);
      }

      // Should contain success indicators
      if (!result.output.includes('✅ Test 1: Basic operations - PASSED')) {
        throw new Error(`Server test output missing expected success indicators`);
      }

      if (!result.output.includes('🎉 SUCCESS: All tests passed!')) {
        throw new Error(`Server test output missing final success message`);
      }

      return { serverResult: result };
    },
    30000 // 30 seconds
  );

  // Test 2: Browser-side test execution (should delegate to server)
  await EventTestUtils.runTestWithCleanup(
    'Browser Test Command - Delegation to Server',
    async () => {
      const { BrowserCommandTester } = await import('../../../commands/test/utils/BrowserCommandTester');
      const tester = new BrowserCommandTester();

      const result = await tester.executeCommand('test', {
        _: ['test-simple.ts']
      });

      console.log('📊 Browser test result:', {
        success: result.success,
        command: result.command,
        duration: result.duration,
        outputLength: result.output.length
      });

      // Should succeed via delegation
      if (!result.success) {
        throw new Error(`Browser test command should succeed via delegation but failed: ${result.output}`);
      }

      // Should contain the same success indicators (delegated from server)
      if (!result.output.includes('✅ Test 1: Basic operations - PASSED')) {
        throw new Error(`Browser test output missing expected success indicators`);
      }

      return { browserResult: result };
    },
    30000 // 30 seconds
  );

  // Test 3: Test failure handling - non-existent test file
  await EventTestUtils.runTestWithCleanup(
    'Test Command - Proper Failure Handling',
    async () => {
      const { ServerCommandTester } = await import('../../../commands/test/utils/ServerCommandTester');
      const tester = new ServerCommandTester();

      const result = await tester.executeCommand('test', {
        _: ['non-existent-test.ts']
      });

      console.log('📊 Failure test result:', {
        success: result.success,
        command: result.command,
        duration: result.duration,
        hasError: !!result.error
      });

      // Should return TestResult with success: false (not reject)
      if (result.success) {
        throw new Error(`Test command should return success: false for non-existent file`);
      }

      // Should have proper command
      if (!result.command.includes('non-existent-test.ts')) {
        throw new Error(`Test command should show attempted command`);
      }

      // Should have error output
      if (!result.output || result.output.length === 0) {
        throw new Error(`Test command should return error output`);
      }

      return { failureResult: result };
    },
    15000 // 15 seconds
  );

  // Test 4: Cross-environment consistency 
  await EventTestUtils.runTestWithCleanup(
    'Cross-Environment Test Command Consistency',
    async () => {
      const { ServerCommandTester } = await import('../../../commands/test/utils/ServerCommandTester');
      const { BrowserCommandTester } = await import('../../../commands/test/utils/BrowserCommandTester');
      
      const serverTester = new ServerCommandTester();
      const browserTester = new BrowserCommandTester();

      // Run same test from both environments
      const serverResult = await serverTester.executeCommand('test', {
        _: ['test-simple.ts']
      });

      const browserResult = await browserTester.executeCommand('test', {
        _: ['test-simple.ts']
      });

      // Both should succeed
      if (!serverResult.success || !browserResult.success) {
        throw new Error(`Both server and browser test commands should succeed`);
      }

      // Both should execute the same command
      if (serverResult.command !== browserResult.command) {
        throw new Error(`Server and browser should execute the same underlying command`);
      }

      // Both should have similar output content (test results)
      const serverHasSuccess = serverResult.output.includes('🎉 SUCCESS: All tests passed!');
      const browserHasSuccess = browserResult.output.includes('🎉 SUCCESS: All tests passed!');

      if (!serverHasSuccess || !browserHasSuccess) {
        throw new Error(`Both environments should show test success in output`);
      }

      console.log('✅ Cross-environment consistency verified');
      console.log(`   Server command: ${serverResult.command}`);
      console.log(`   Browser command: ${browserResult.command}`);
      console.log(`   Both successful: ${serverResult.success && browserResult.success}`);

      return { serverResult, browserResult };
    },
    45000 // 45 seconds for both environments
  );

  console.log('');
  console.log('🎉 ALL TEST COMMAND INTEGRATION TESTS PASSED!');
  console.log('✅ Server-side test execution working');
  console.log('✅ Browser-side delegation working');
  console.log('✅ Proper failure handling (TestResult with success: false)');
  console.log('✅ Cross-environment consistency verified');
}

// Self-executing test runner
if (require.main === module) {
  runTestCommandIntegrationTests().catch(error => {
    console.error('❌ Test Command Integration Tests Failed:', error);
    process.exit(1);
  });
}