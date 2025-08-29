import { spawn } from 'child_process';
import { startSystem } from './system-startup';

interface OutputFilter {
  shouldShowLine(line: string): boolean;
}

class VerboseOutputFilter implements OutputFilter {
  shouldShowLine(): boolean {
    return true; // Show everything in verbose mode
  }
}

class TestOutputFilter implements OutputFilter {
  private readonly testStatusPatterns = [
    /^▶️/,  // Running test
    /^✅/,  // Test passed 
    /^❌/,  // Test failed
    /^💥/,  // Error details
  ];
  
  private readonly importantSystemPatterns = [
    /^🚀/,  // System startup
    /^📋/,  // Configuration info  
    /^⚠️/,  // Warnings
    /^🎯/,  // Main headers
    /^═══/, // Summary sections
  ];

  shouldShowLine(line: string): boolean {
    const trimmedLine = line.trim();
    if (!trimmedLine) return false;
    
    // Check test status patterns first (highest priority)
    for (const pattern of this.testStatusPatterns) {
      if (pattern.test(trimmedLine)) return true;
    }
    
    // Check important system patterns
    for (const pattern of this.importantSystemPatterns) {
      if (pattern.test(trimmedLine)) return true;
    }
    
    return false;
  }
}

class SilentOutputFilter implements OutputFilter {
  shouldShowLine(): boolean {
    return false; // Show nothing (for completely silent mode)
  }
}

// Factory for creating appropriate filters
class OutputFilterFactory {
  static create(mode: 'verbose' | 'normal' | 'silent'): OutputFilter {
    switch (mode) {
      case 'verbose': return new VerboseOutputFilter();
      case 'normal': return new TestOutputFilter();
      case 'silent': return new SilentOutputFilter();
      default: return new TestOutputFilter();
    }
  }
}

// Configuration for test execution
interface TestConfig {
  command: string[];
  verbose: boolean;
  summaryMarker: string;
}

// Result of test execution
interface TestExecutionResult {
  success: boolean;
  summary?: string;
  output: string;
}

// Helper function to create environment for test execution
function createTestEnvironment(verbose: boolean): NodeJS.ProcessEnv {
  return {
    ...process.env,
    JTAG_TEST_VERBOSE: verbose ? 'true' : 'false'
  };
}

// Helper function to process test output data
function processTestOutput(
  text: string,
  capturedOutput: { value: string },
  verbose: boolean,
  outputFilter: OutputFilter
): void {
  // Always capture the output for summary extraction
  capturedOutput.value += text;
  
  if (verbose) {
    // Verbose mode: show everything in real-time
    process.stdout.write(text);
  } else {
    // Filtered mode: show only lines that pass the filter
    const lines = text.split('\n');
    for (const line of lines) {
      if (outputFilter.shouldShowLine(line)) {
        console.log(line);
      }
    }
  }
}

// Generic command executor with output filtering
async function executeCommand(config: TestConfig): Promise<TestExecutionResult> {
  return new Promise((resolve) => {
    const capturedOutput = { value: '' };
    const outputFilter = OutputFilterFactory.create(config.verbose ? 'verbose' : 'normal');
    const env = createTestEnvironment(config.verbose);
    
    const child = spawn(config.command[0], config.command.slice(1), {
      stdio: ['inherit', 'pipe', 'inherit'],
      cwd: process.cwd(),
      env
    });
    
    child.stdout?.on('data', (data) => {
      processTestOutput(data.toString(), capturedOutput, config.verbose, outputFilter);
    });
    
    child.on('exit', (code) => {
      const summaryStart = capturedOutput.value.lastIndexOf(config.summaryMarker);
      const summary = summaryStart !== -1 ? capturedOutput.value.substring(summaryStart) : '';
      
      resolve({
        success: code === 0,
        summary,
        output: capturedOutput.value
      });
    });
    
    child.on('error', (error) => {
      console.error('❌ Command execution error:', error.message);
      resolve({ success: false, output: capturedOutput.value });
    });
  });
}

interface TestResult {
  readonly success: boolean;
  readonly serverStarted: boolean;
  readonly testsRan: boolean;
  readonly errorMessage?: string;
}

async function runTests(verbose: boolean = false): Promise<{ success: boolean; summary?: string }> {
  console.log('🧪 Running test suite...');
  
  const config: TestConfig = {
    command: ['npm', 'run', 'test:comprehensive'],
    verbose,
    summaryMarker: '═══════════════════════════════════════════════════════════════════════════════'
  };
  
  const result = await executeCommand(config);
  return {
    success: result.success,
    summary: result.summary
  };
}

async function teardownSystem(): Promise<void> {
  try {
    // Stop system daemons but preserve session directories for analysis
    const { spawn } = require('child_process');
    await new Promise<void>((resolve) => {
      const cleanup = spawn('npm', ['run', 'system:stop'], {
        stdio: 'inherit',
        shell: true
      });
      
      cleanup.on('exit', (code) => {
        console.log(`✅ System teardown completed (exit code: ${code})`);
        resolve();
      });
      
      cleanup.on('error', (error) => {
        console.warn(`⚠️ Teardown error (continuing anyway): ${error.message}`);
        resolve(); // Continue even if teardown fails
      });
    });
    
    // Brief wait for daemons to fully stop
    await new Promise(resolve => setTimeout(resolve, 500));
    
  } catch (error) {
    console.warn(`⚠️ Teardown failed (continuing): ${error}`);
  }
}

async function checkSystemReady(): Promise<boolean> {
  try {
    // Use dynamic port detection instead of hardcoded ports
    const { getActivePorts } = require('../examples/shared/ExampleConfig');
    const activePorts = await getActivePorts();
    const exec = require('util').promisify(require('child_process').exec);
    
    const wsCheck = await exec(`lsof -ti:${activePorts.websocket_server} 2>/dev/null | head -1 || echo ""`);
    const httpCheck = await exec(`lsof -ti:${activePorts.http_server} 2>/dev/null | head -1 || echo ""`);
    
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
    // Parse command line arguments for --verbose flag
    const verbose = process.argv.includes('--verbose');
    
    console.log('🎯 JTAG TEST WITH SERVER MANAGEMENT');
    console.log('📋 This will check for existing system or start fresh, run tests, then clean up');
    if (verbose) {
      console.log('🔊 Verbose mode enabled - showing detailed test output');
    }
    
    // Check if system is already running (likely from npm test System Ensure phase)
    const systemAlreadyRunning = await checkSystemReady();
    
    if (systemAlreadyRunning) {
      console.log('✅ System already running and healthy - reusing existing system');
    } else {
      console.log('🚀 No healthy system detected - starting fresh system');
      // Start the system using shared startup logic for testing
      await startSystem('npm-test');
    }
    
    // Run tests with verbose flag
    const testResult = await runTests(verbose);
    testsSuccessful = testResult.success; // Update tracking variable
    
    // Show the captured test results as final output (after all daemon messages)
    if (testResult.summary) {
      console.log(testResult.summary.trim());
    }
    
    // Exit gracefully with proper exit code
    process.exit(testResult.success ? 0 : 1);
    
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