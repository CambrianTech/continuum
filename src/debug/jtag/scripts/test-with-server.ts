import { spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';

// Strong typing for server management
interface ServerProcess {
  readonly child: ChildProcess;
  readonly pid: number | undefined;
  readonly startTime: number;
}

interface TestResult {
  readonly success: boolean;
  readonly serverStarted: boolean;
  readonly testsRan: boolean;
  readonly errorMessage?: string;
}

const logDir = path.resolve('.continuum/jtag/system/logs');
const logFile = path.join(logDir, 'test-server.log');

// Ensure directory exists
fs.mkdirSync(logDir, { recursive: true });

async function startServerProcess(): Promise<ServerProcess> {
  console.log('🚀 Starting JTAG server process for testing...');
  
  return new Promise((resolve, reject) => {
    const child: ChildProcess = spawn('npm', ['run', 'start:direct'], {
      detached: false,  // Keep attached for proper control during testing
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        FORCE_COLOR: '1',
        TERM: 'xterm-256color',
        CI: ''
      },
      cwd: process.cwd()
    });
    
    if (!child.pid) {
      reject(new Error('Failed to spawn server process'));
      return;
    }
    
    console.log(`🎯 Server process started with PID: ${child.pid}`);
    
    const logStream = fs.createWriteStream(logFile, { flags: 'w' });
    
    // Pipe output to log file
    child.stdout?.pipe(logStream);
    child.stderr?.pipe(logStream);
    
    // Handle server startup
    const serverProcess: ServerProcess = {
      child,
      pid: child.pid,
      startTime: Date.now()
    };
    
    // Check for server death during startup
    child.on('exit', (code, signal) => {
      logStream.end();
      console.error(`💀 Server process died during startup: code=${code}, signal=${signal}`);
      reject(new Error(`Server died during startup: ${code}`));
    });
    
    resolve(serverProcess);
  });
}

async function waitForServerReady(signaler: SystemReadySignaler): Promise<boolean> {
  console.log('⏳ Waiting for server to be ready...');
  
  const maxAttempts = 30; // 30 attempts x 2s = 60s timeout
  
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      const signal = await signaler.generateReadySignal();
      
      if (signal.bootstrapComplete && signal.commandCount > 0) {
        console.log(`✅ Server is ready! (${signal.commandCount} commands, attempt ${attempt})`);
        return true;
      }
      
      console.log(`⏳ Attempt ${attempt}/${maxAttempts}: bootstrap=${signal.bootstrapComplete}, commands=${signal.commandCount}`);
      
    } catch (error) {
      console.log(`⏳ Attempt ${attempt}/${maxAttempts}: Server not responding yet`);
    }
    
    // Wait 2 seconds before next attempt
    await new Promise(resolve => setTimeout(resolve, 2000));
  }
  
  console.error('❌ Timeout waiting for server to be ready');
  return false;
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
  let serverProcess: ServerProcess | null = null;
  const signaler = new SystemReadySignaler();
  
  try {
    console.log('🎯 JTAG TEST WITH SERVER MANAGEMENT');
    console.log('📋 This will start server, run tests, then clean up');
    
    // Clear any old signals first
    await signaler.clearSignals();
    
    // Start server
    serverProcess = await startServerProcess();
    
    // Wait for server to be ready
    const serverReady = await waitForServerReady(signaler);
    
    if (!serverReady) {
      throw new Error('Server failed to become ready within timeout');
    }
    
    // Run tests
    const testsSucceeded = await runTests();
    
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
      process.exit(0);
    } else {
      console.error('💥 TESTS FAILED - npm test failed');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Fatal error:', error instanceof Error ? error.message : error);
    
    const result: TestResult = {
      success: false,
      serverStarted: !!serverProcess,
      testsRan: false,
      errorMessage: error instanceof Error ? error.message : String(error)
    };
    
    console.log('🎯 TEST RESULTS:');
    console.log(JSON.stringify(result, null, 2));
    
    process.exit(1);
    
  } finally {
    // Clean up server process
    if (serverProcess?.child) {
      console.log(`🧹 Cleaning up server process (PID: ${serverProcess.pid})`);
      
      try {
        serverProcess.child.kill('SIGTERM');
        
        // Give it 5 seconds to exit gracefully, then force kill
        setTimeout(() => {
          if (!serverProcess.child.killed) {
            console.log('🔨 Force killing server process');
            serverProcess.child.kill('SIGKILL');
          }
        }, 5000);
        
      } catch (killError) {
        console.warn('⚠️ Error killing server process:', killError);
      }
    }
  }
}

main();