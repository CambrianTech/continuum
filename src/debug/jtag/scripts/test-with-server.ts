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
  console.log('🚀 Starting JTAG server in tmux session for persistence...');
  
  return new Promise((resolve, reject) => {
    // First, kill any existing tmux session
    const killSession = spawn('tmux', ['kill-session', '-t', 'jtag-test'], {
      stdio: 'ignore'
    });
    
    killSession.on('close', () => {
      // Create new tmux session with server
      const tmuxCmd = [
        'new-session',
        '-d',          // detached
        '-s', 'jtag-test',  // session name
        'npm', 'run', 'start:direct'  // direct start, no smart detection
      ];
      
      console.log(`🔧 Creating tmux session: tmux ${tmuxCmd.join(' ')}`);
      
      const child: ChildProcess = spawn('tmux', tmuxCmd, {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: {
          ...process.env,
          FORCE_COLOR: '1',
          TERM: 'xterm-256color'
        },
        cwd: process.cwd()
      });
      
      const logStream = fs.createWriteStream(logFile, { flags: 'w' });
      
      // Pipe output to log file
      child.stdout?.pipe(logStream);
      child.stderr?.pipe(logStream);
      
      child.on('close', (code) => {
        logStream.end();
        
        if (code === 0) {
          console.log('✅ Tmux session created successfully');
          
          // Get the PID of the process running inside tmux
          const getPidCmd = spawn('tmux', [
            'list-panes', '-t', 'jtag-test', '-F', '#{pane_pid}'
          ], { stdio: ['ignore', 'pipe', 'ignore'] });
          
          let pidOutput = '';
          getPidCmd.stdout?.on('data', (data) => {
            pidOutput += data.toString();
          });
          
          getPidCmd.on('close', () => {
            const tmuxPid = parseInt(pidOutput.trim());
            
            const serverProcess: ServerProcess = {
              child: child,  // This is the tmux command, not the actual server
              pid: tmuxPid,  // PID of process inside tmux
              startTime: Date.now()
            };
            
            console.log(`🎯 Tmux session 'jtag-test' created with server PID: ${tmuxPid}`);
            resolve(serverProcess);
          });
          
        } else {
          reject(new Error(`Failed to create tmux session: exit code ${code}`));
        }
      });
      
      child.on('error', (error) => {
        logStream.end();
        reject(new Error(`Tmux spawn error: ${error.message}`));
      });
    });
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
  let testsSuccessful = false; // Track success for cleanup decision
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
      serverStarted: !!serverProcess,
      testsRan: false,
      errorMessage: error instanceof Error ? error.message : String(error)
    };
    
    console.log('🎯 TEST RESULTS:');
    console.log(JSON.stringify(result, null, 2));
    
    process.exit(1);
    
  } finally {
    // Check if tmux session is actually running
    const checkTmux = spawn('tmux', ['has-session', '-t', 'jtag-test'], {
      stdio: 'ignore'
    });
    
    checkTmux.on('close', (code) => {
      if (code === 0) {
        console.log(`🚀 Server running in tmux session 'jtag-test' - survives script exit`);
        console.log(`📋 To check server: tmux attach-session -t jtag-test`);
        console.log(`📋 To stop server: tmux kill-session -t jtag-test`);
        console.log(`📋 To view logs: tail -f ${logFile}`);
      } else {
        if (!testsSuccessful) {
          console.log(`🧹 Tests failed - tmux session not running (expected)`);
        } else {
          console.log(`⚠️  Tmux session not detected - server may have exited`);
        }
      }
    });
  }
}

main();