import { spawn } from 'child_process';
import { startSystem } from './system-startup';

interface TestResult {
  readonly success: boolean;
  readonly serverStarted: boolean;
  readonly testsRan: boolean;
  readonly errorMessage?: string;
}

async function runTests(): Promise<{ success: boolean; summary?: string }> {
  console.log('🧪 Running test suite...');
  
  return new Promise((resolve) => {
    let capturedOutput = '';
    
    const testChild = spawn('npm', ['run', 'test:comprehensive'], {
      stdio: ['inherit', 'pipe', 'inherit'], // Capture stdout to extract summary
      cwd: process.cwd()
    });
    
    // Capture output while also showing it
    testChild.stdout?.on('data', (data) => {
      const text = data.toString();
      process.stdout.write(text); // Still show output in real-time
      capturedOutput += text;
    });
    
    testChild.on('exit', (code) => {
      // Extract the comprehensive summary from the output
      const summaryStart = capturedOutput.lastIndexOf('═══════════════════════════════════════════════════════════════════════════════');
      const summary = summaryStart !== -1 ? capturedOutput.substring(summaryStart) : '';
      
      if (code === 0) {
        resolve({ success: true, summary });
      } else {
        resolve({ success: false, summary });
      }
    });
    
    testChild.on('error', (error) => {
      console.error('❌ Test execution error:', error.message);
      resolve({ success: false });
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
    const testResult = await runTests();
    testsSuccessful = testResult.success; // Update tracking variable
    
    // Strategy: Let all output complete naturally, then show summary as true conclusion
    // Wait for any background daemon cleanup to finish
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Show the clean comprehensive summary as the final conclusion  
    if (testResult.summary) {
      // Clear any lingering output and show clean summary
      console.log('\n'); // Ensure clean separation
      console.log(testResult.summary.trim()); // Remove extra whitespace
    }
    
    if (testResult.success) {
      // Don't exit - let the keep-alive logic in system-startup.ts keep servers running
    } else {
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