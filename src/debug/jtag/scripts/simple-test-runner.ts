#!/usr/bin/env tsx
/**
 * Simple Test Runner - Just run the existing test:start-and-test command
 * 
 * This is a simplified approach that uses the existing test infrastructure
 * instead of complex orchestration.
 */

import { spawn } from 'child_process';

async function runSimpleTest(): Promise<boolean> {
  console.log('🧪 SIMPLE TEST RUNNER: Starting npm test chain');
  console.log('📋 This will use existing test:start-and-test infrastructure');
  
  return new Promise<boolean>((resolve) => {
    const testProcess = spawn('npm', ['run', 'test:start-and-test'], {
      stdio: 'inherit',
      shell: true,
      env: {
        ...process.env,
        JTAG_ACTIVE_EXAMPLE: 'test-bench'  // Force test-bench example
      }
    });
    
    // Handle interruption
    process.on('SIGINT', () => {
      console.log('\\n🛑 Test interrupted - cleaning up...');
      testProcess.kill('SIGINT');
      setTimeout(() => {
        process.exit(130);
      }, 1000);
    });
    
    testProcess.on('exit', (code) => {
      const success = code === 0;
      if (success) {
        console.log('\\n🎉 All tests completed successfully!');
      } else {
        console.log(`\\n❌ Tests failed with exit code: ${code}`);
      }
      resolve(success);
    });
    
    testProcess.on('error', (error) => {
      console.error(`\\n💥 Test execution error: ${error.message}`);
      resolve(false);
    });
  });
}

// Main execution
async function main() {
  try {
    const success = await runSimpleTest();
    process.exit(success ? 0 : 1);
  } catch (error: any) {
    console.error('\\n💥 Simple test runner failed:', error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}