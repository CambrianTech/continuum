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

async function checkSystemReady(): Promise<boolean> {
  try {
    // SIMPLE APPROACH: Just check if both servers are running
    const { execAsync } = require('child_process').promisify;
    const exec = require('util').promisify(require('child_process').exec);
    
    const wsCheck = await exec('lsof -ti:9001 2>/dev/null | head -1 || echo ""');
    const httpCheck = await exec('lsof -ti:9002 2>/dev/null | head -1 || echo ""');
    
    const wsRunning = wsCheck.stdout.trim().length > 0;
    const httpRunning = httpCheck.stdout.trim().length > 0;
    
    return wsRunning && httpRunning;
  } catch (error) {
    return false;
  }
}

async function main(): Promise<void> {
  let testsSuccessful = false; // Track success for cleanup decision
  
  try {
    console.log('🎯 JTAG TEST WITH SERVER MANAGEMENT');
    console.log('📋 This will check for existing system or start fresh, run tests, then clean up');
    
    // Check if system is already running (likely from npm test System Ensure phase)
    const systemAlreadyRunning = await checkSystemReady();
    
    if (systemAlreadyRunning) {
      console.log('✅ System already running and healthy - reusing existing system');
    } else {
      console.log('🚀 No healthy system detected - starting fresh system');
      // Start the system using shared startup logic for testing
      await startSystem('npm-test');
    }
    
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
    
    // Generate comprehensive test report
    console.log('\n📄 Generating comprehensive test report...');
    try {
      const reportChild = spawn('npm', ['run', 'test:report'], {
        stdio: 'inherit',
        cwd: process.cwd()
      });

      await new Promise<void>((resolve) => {
        reportChild.on('exit', () => resolve());
        reportChild.on('error', () => resolve()); // Continue even if report fails
      });
    } catch (error) {
      console.warn('⚠️ Report generation failed (test results still valid):', error);
    }

    if (testsSucceeded) {
      console.log('🎉 ALL TESTS PASSED - npm test succeeded!');
      console.log('🚀 Server left running for development (as intended)');
      console.log('📡 Use Ctrl+C to stop the server, or run ./jtag commands in another terminal');
      // Don't exit - let the keep-alive logic in system-startup.ts keep servers running
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