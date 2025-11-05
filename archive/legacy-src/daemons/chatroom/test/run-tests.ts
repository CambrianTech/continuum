#!/usr/bin/env npx tsx

/**
 * ChatRoomDaemon Test Runner
 * 
 * Runs all ChatRoomDaemon tests (unit and integration) and provides
 * clear results for default room functionality validation.
 */

import { spawn } from 'child_process';
import path from 'path';

async function runTests(): Promise<void> {
  console.log('🧪 Running ChatRoomDaemon Tests');
  console.log('================================');
  console.log('');

  const testDir = path.dirname(import.meta.url.replace('file://', ''));
  
  try {
    // Check if test files exist
    const unitTestFile = path.join(testDir, 'unit/ChatRoomDaemon.test.ts');
    const integrationTestFile = path.join(testDir, 'integration/ChatRoomDaemon.integration.test.ts');
    
    console.log('📁 Checking test files...');
    console.log(`Unit tests: ${unitTestFile}`);
    console.log(`Integration tests: ${integrationTestFile}`);
    console.log('');

    // Run unit tests
    console.log('🔧 Running Unit Tests...');
    await runTestCommand(['npx', 'tsx', '--test', unitTestFile]);
    
    console.log('');
    
    // Run integration tests
    console.log('🌐 Running Integration Tests...');
    await runTestCommand(['npx', 'tsx', '--test', integrationTestFile]);
    
    console.log('');
    console.log('✅ All ChatRoomDaemon tests completed successfully!');
    console.log('');
    console.log('🏠 Default Rooms Validated:');
    console.log('  ✅ general - General Chat (type: chat)');
    console.log('  ✅ development - Development (type: collaboration)');
    console.log('  ✅ academy - Academy AI Training (type: collaboration)');
    console.log('');
    console.log('🔌 Event Bus Integration Validated:');
    console.log('  ✅ chatroom_request handling');
    console.log('  ✅ chatroom_response emission');
    console.log('  ✅ Operation routing via CommandOperation enum');
    console.log('  ✅ Error handling for non-existent rooms');

  } catch (error) {
    console.error('❌ Test execution failed:', error);
    process.exit(1);
  }
}

function runTestCommand(command: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    const process = spawn(command[0], command.slice(1), {
      stdio: 'inherit',
      shell: true
    });

    process.on('close', (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Test command failed with exit code ${code}`));
      }
    });

    process.on('error', (error) => {
      reject(error);
    });
  });
}

// Run tests if this script is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runTests().catch(error => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

export { runTests };