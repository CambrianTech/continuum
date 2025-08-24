import { spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';
import { getActivePorts } from '../system/shared/ExampleConfig';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';

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

// Use WorkingDirConfig for per-project isolation
const logDir = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'system', 'logs');
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
        'npx', 'tsx', 'scripts/launch-active-example.ts'  // direct intelligent startup
      ];
      
      console.log(`🔧 Creating tmux session: tmux ${tmuxCmd.join(' ')}`);
      
      const child: ChildProcess = spawn('tmux', tmuxCmd, {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: {
          ...process.env,
          FORCE_COLOR: '1',
          TERM: 'xterm-256color',
          JTAG_WORKING_DIR: 'examples/widget-ui'  // Set the working directory context
        },
        cwd: process.cwd()  // Run tmux from JTAG root directory
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
  // Get ports from examples.json ONLY
  let wsPort: number;
  let httpPort: number;
  
  try {
    const activePorts = getActivePorts();
    wsPort = activePorts.websocket_server;
    httpPort = activePorts.http_server;
    console.log(`🔧 Using ports from examples.json: WebSocket=${wsPort}, HTTP=${httpPort}`);
  } catch (error) {
    console.error('❌ CRITICAL FAILURE: Cannot load port configuration from examples.json!');
    console.error('❌ Error:', error.message);
    console.error('❌ This is a FATAL error - examples.json configuration system is broken!');
    console.error('❌ Expected: examples.json should contain port configuration for active example');
    throw new Error(`Port configuration failure: ${error.message}`);
  }
  
  console.log('⏳ Waiting for COMPLETE server system to be ready...');
  console.log(`🔍 Checking: WebSocket server (${wsPort}) + HTTP server (${httpPort}) + Bootstrap`);
  
  // Use event-driven signal detection with 60 second timeout
  const signal = await signaler.checkSystemReady(60000); // 60s timeout
  
  if (!signal) {
    console.error('❌ Timeout waiting for COMPLETE server system to be ready');
    console.error(`🔍 System needs: Bootstrap ✓ + Commands ✓ + WebSocket(${wsPort}) ✓ + HTTP(${httpPort}) ✓ + Healthy ✓`);
    return false;
  }
  
  // Check all requirements
  const requiredPorts = [wsPort, httpPort]; // WebSocket + HTTP servers
  const hasBootstrap = signal.bootstrapComplete;
  const hasCommands = signal.commandCount > 0;
  const hasAllPorts = requiredPorts.every(port => 
    signal.portsActive && signal.portsActive.includes(port)
  );
  // Accept intelligent signal detection: system is ready when core functions work
  // Browser readiness is nice-to-have but not required for core functionality
  const isHealthy = (signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded') && 
                    signal.bootstrapComplete && 
                    signal.commandCount > 0 &&
                    (signal.portsActive?.length || 0) >= 2;
  
  console.log(`📊 Final system check:`);
  console.log(`   Bootstrap: ${hasBootstrap ? '✅' : '❌'}`);
  console.log(`   Commands: ${hasCommands ? '✅' : '❌'} (${signal.commandCount})`);
  console.log(`   WebSocket (${wsPort}): ${signal.portsActive?.includes(wsPort) ? '✅' : '❌'}`);
  console.log(`   HTTP (${httpPort}): ${signal.portsActive?.includes(httpPort) ? '✅' : '❌'}`);
  console.log(`   Health: ${signal.systemHealth}`);
  
  // Show node errors if we have them so we're not debugging blind
  if (signal.nodeErrors && signal.nodeErrors.length > 0) {
    console.log(`   ⚠️ Node errors: ${signal.nodeErrors.slice(0, 2).join('; ')}`);
  }
  
  if (hasBootstrap && hasCommands && hasAllPorts && isHealthy) {
    console.log(`✅ COMPLETE server system ready! (${signal.commandCount} commands, ${signal.portsActive?.length} ports active)`);
    console.log(`🌐 Active ports: ${signal.portsActive?.join(', ')}`);
    return true;
  }
  
  // Show what's missing
  if (!hasAllPorts) {
    const activePorts = signal.portsActive || [];
    const missingPorts = requiredPorts.filter(port => !activePorts.includes(port));
    console.log(`   ⚠️ Missing critical ports: ${missingPorts.join(', ')}`);
  }
  
  // System responded but not fully ready
  console.error('❌ System responded but requirements not met');
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
  
  // Set up proper working directory context for per-project isolation
  // This ensures SystemReadySignaler uses the same context as the running system
  const { getActiveExampleName } = await import('../system/shared/ExampleConfig');
  const activeExample = getActiveExampleName();
  const workingDir = `examples/${activeExample}`;
  WorkingDirConfig.setWorkingDir(workingDir);
  console.log(`🎯 Test context set to: ${workingDir}`);
  
  // Initialize signaler AFTER setting working directory context
  const signaler = new SystemReadySignaler();
  
  // Reset logging state for clean test output
  (signaler as any).hasLoggedStaleFile = false;
  (signaler as any).hasLoggedNoFile = false;
  (signaler as any).hasLoggedNotReady = false;
  (signaler as any).hasLoggedError = false;
  
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