import { spawn } from 'child_process';
import { startSystem } from './system-startup';

interface TestResult {
  readonly success: boolean;
  readonly serverStarted: boolean;
  readonly testsRan: boolean;
  readonly errorMessage?: string;
}

async function runTests(): Promise<boolean> {
  console.log('🧪 Running test suite...');
  
  return new Promise((resolve) => {
    const testChild = spawn('npm', ['run', 'test:comprehensive'], {
      stdio: 'inherit', // Show test output directly
      cwd: process.cwd()
    });
    
    testChild.on('exit', (code) => {
      if (code === 0) {
        console.log('✅ All tests passed!');
        resolve(true);
      } else {
        console.error(`❌ Tests failed with code: ${code}`);
        resolve(false);
      }
    });
    
    testChild.on('error', (error) => {
      console.error('❌ Test execution error:', error.message);
      resolve(false);
    });
  });
}

async function main(): Promise<void> {
  let testsSuccessful = false; // Track success for cleanup decision
  
  try {
    console.log('🎯 JTAG TEST WITH SERVER MANAGEMENT');
    console.log('📋 This will start server, run tests, then clean up');
    
    // Start the system using shared startup logic for testing
    await startSystem('npm-test');
    
    // Run tests
    const testsSucceeded = await runTests();
    testsSuccessful = testsSucceeded; // Update tracking variable
    
    // Report results
    const result: TestResult = {
      success: testsSucceeded,
      serverStarted: true,
      testsRan: true
    };
    
    console.log('🎯 TEST RESULTS:');
    console.log(JSON.stringify(result, null, 2));
    
    if (testsSucceeded) {
      console.log('🎉 ALL TESTS PASSED - npm test succeeded!');
      console.log('🚀 Server left running for development (as intended)');
      process.exit(0);
    } else {
      console.error('💥 TESTS FAILED - npm test failed');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Fatal error:', error instanceof Error ? error.message : error);
    
    const result: TestResult = {
      success: false,
      serverStarted: false,
      testsRan: false,
      errorMessage: error instanceof Error ? error.message : String(error)
    };
    
    console.log('🎯 TEST RESULTS:');
    console.log(JSON.stringify(result, null, 2));
    
    process.exit(1);
  }
}

main();