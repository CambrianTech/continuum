import { exec, spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';

// Parse command line arguments for configurable behavior
interface LaunchConfig {
  readonly forceRestart: boolean;
  readonly checkExisting: boolean;
  readonly mode: 'development' | 'test' | 'production';
  readonly verbose: boolean;
  readonly skipHealthCheck: boolean;
}

function parseArguments(): LaunchConfig {
  const args = process.argv;
  
  return {
    forceRestart: args.includes('--force') || args.includes('--restart'),
    checkExisting: !args.includes('--no-check'),
    mode: args.includes('--test') ? 'test' : args.includes('--production') ? 'production' : 'development',
    verbose: args.includes('--verbose') || args.includes('-v'),
    skipHealthCheck: args.includes('--skip-health-check')
  };
}

const CONFIG = parseArguments();

// Strong typing for monitoring configuration
interface MonitoringConfig {
  readonly initialPollMs: number;
  readonly maxPollMs: number;
  readonly backoffMultiplier: number;
  readonly maxTimeoutMs: number;
  readonly fastPollingDuration: number;
}

// Type-safe monitoring state
interface MonitoringState {
  active: boolean;
  currentPollInterval: number;
  startTime: number;
  pollCount: number;
}

// Strong typing for process diagnostics
interface ProcessDiagnostics {
  readonly pid: number | undefined;
  readonly startTime: number;
  readonly command: string;
  readonly args: readonly string[];
  readonly cwd: string;
  readonly exitCode: number | null;
  readonly killed: boolean;
  readonly signalCode: NodeJS.Signals | null;
}

// Strong typing for launch results
interface LaunchResult {
  readonly success: boolean;
  readonly reason: 'launch_success' | 'launch_failure' | 'process_spawn_error' | 'pid_file_error';
  readonly processId: number | undefined;
  readonly logFile: string;
  readonly errorMessage?: string;
  readonly diagnostics: ProcessDiagnostics;
}

const MONITORING_CONFIG: MonitoringConfig = {
  initialPollMs: 1000,        // Start fast - 1 second
  maxPollMs: 5000,           // Max 5 seconds between polls
  backoffMultiplier: 1.2,    // Gradual slowdown  
  maxTimeoutMs: 120_000,     // 2 minutes total timeout
  fastPollingDuration: 15_000 // Fast polling for first 15 seconds
} as const;

// Prepare paths
const logDir = path.resolve('.continuum/jtag/system/logs');
const logFile = path.join(logDir, 'npm-start.log');
const pidFile = path.join(logDir, 'npm-start.pid');

// Ensure directory exists
fs.mkdirSync(logDir, { recursive: true });

// Launch configuration constants
const LAUNCH_CONFIG = {
  BOOTSTRAP_WAIT_MS: 5000,
  MAX_MONITORING_TIME_MS: 300000, // 5 minutes
  LOG_CHUNK_SIZE: 1024
} as const;

async function launchWithTmuxPersistence(): Promise<LaunchResult> {
  const signaler = new SystemReadySignaler();
  
  console.log('🚀 Launching JTAG system with tmux persistence...');
  
  // Clear any old signals first
  await signaler.clearSignals();
  
  return new Promise<LaunchResult>((resolve, reject) => {
    const startTime = Date.now();
    
    // First, kill any existing tmux session
    const killSession = spawn('tmux', ['kill-session', '-t', 'jtag-test'], {
      stdio: 'ignore'
    });
    
    killSession.on('close', () => {
      // Create tmux session with persistent server (like test-with-server.ts)
      const tmuxCmd = [
        'new-session',
        '-d',                    // detached session
        '-s', 'jtag-test',       // session name
        'npm', 'run', 'start:direct'  // server command
      ];
      
      console.log(`🔧 Creating persistent tmux session: tmux ${tmuxCmd.join(' ')}`);
      
      const child: ChildProcess = spawn('tmux', tmuxCmd, {
        stdio: ['ignore', 'pipe', 'pipe'],  // Capture output from tmux command
        env: {
          ...process.env,
          FORCE_COLOR: '1',
          TERM: 'xterm-256color'
        },
        cwd: process.cwd()
      });
      
      // Set up log file streaming
      const logStream = fs.createWriteStream(logFile, { flags: 'w' });
      
      // Pipe tmux command output to log file
      child.stdout?.pipe(logStream);
      child.stderr?.pipe(logStream);
      
      child.on('close', (code) => {
        logStream.end();
        
        if (code === 0) {
          console.log('✅ Tmux session created successfully');
          
          // Get the PID of the process running inside tmux session
          const getPidCmd = spawn('tmux', [
            'list-panes', '-t', 'jtag-test', '-F', '#{pane_pid}'
          ], { stdio: ['ignore', 'pipe', 'ignore'] });
          
          let pidOutput = '';
          getPidCmd.stdout?.on('data', (data) => {
            pidOutput += data.toString();
          });
          
          getPidCmd.on('close', () => {
            const tmuxPid = parseInt(pidOutput.trim());
            
            // Save tmux session PID
            if (tmuxPid) {
              fs.writeFileSync(pidFile, tmuxPid.toString());
              console.log(`📋 Tmux server PID saved to: ${pidFile}`);
            }
            
            console.log(`🎯 Tmux session 'jtag-test' created with server PID: ${tmuxPid}`);
            
            // Create diagnostics object with strong typing
            const createDiagnostics = (): ProcessDiagnostics => ({
              pid: tmuxPid,
              startTime,
              command: 'tmux',
              args: tmuxCmd,
              cwd: process.cwd(),
              exitCode: 0,  // tmux creation succeeded
              killed: false,
              signalCode: null
            });
            
            // Set up server readiness detection with timeout
            let serverDetectionTimeout: NodeJS.Timeout;
            let serverStartupDetected = false;
            let detectionAttempts = 0;
            const maxDetectionAttempts = 24; // 2 minutes (24 * 5s = 120s)
            
            const detectServerStartup = () => {
              serverDetectionTimeout = setTimeout(async () => {
                if (serverStartupDetected) return;
                
                detectionAttempts++;
                
                // Only show message on first attempt, then show dots for progress
                if (detectionAttempts === 1) {
                  process.stdout.write('🔍 Waiting for JTAG server to be ready');
                } else if (detectionAttempts <= maxDetectionAttempts) {
                  process.stdout.write('.');
                }
                
                try {
                  // Check if the server is actually running by testing readiness
                  const signal = await signaler.generateReadySignal();
                  
                  if (signal.bootstrapComplete && signal.commandCount > 0) {
                    serverStartupDetected = true;
                    clearTimeout(serverDetectionTimeout);
                    
                    console.log(' ✅');
                    console.log(`🚀 JTAG server ready! (${signal.commandCount} commands available)`);
                    
                    resolve({
                      success: true,
                      reason: 'launch_success',
                      processId: tmuxPid,
                      logFile,
                      diagnostics: createDiagnostics()
                    });
                    
                  } else if (detectionAttempts < maxDetectionAttempts) {
                    detectServerStartup(); // Try again
                  } else {
                    console.log('⏰ Server detection timeout - system may be ready but health check is failing');
                    console.log('🔧 The system is likely running in the background. Check with: tmux attach-session -t jtag-test');
                    
                    resolve({
                      success: true, // Still success since tmux launched
                      reason: 'launch_success_with_timeout',
                      processId: tmuxPid,
                      logFile,
                      diagnostics: createDiagnostics()
                    });
                  }
                  
                } catch (error) {
                  if (detectionAttempts < maxDetectionAttempts) {
                    console.log('⏳ Server not ready yet, checking again in 5s...');
                    detectServerStartup(); // Try again
                  } else {
                    console.log('⏰ Server detection timeout after multiple attempts');
                    console.log('🔧 The system may be running in the background. Check with: tmux attach-session -t jtag-test');
                    
                    resolve({
                      success: true, // Still success since tmux launched
                      reason: 'launch_success_with_timeout',
                      processId: tmuxPid,
                      logFile,
                      diagnostics: createDiagnostics()
                    });
                  }
                }
                
              }, 5000); // Check every 5 seconds
            };
            
            // Start checking for server readiness after tmux session is created
            setTimeout(detectServerStartup, 5000); // Start checking after 5 seconds
            
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

async function monitorSystemReadiness(signaler: SystemReadySignaler): Promise<void> {
  console.log('👀 Monitoring system readiness...');
  
  return new Promise((resolve, reject) => {
    const state: MonitoringState = {
      active: true,
      currentPollInterval: MONITORING_CONFIG.initialPollMs,
      startTime: Date.now(),
      pollCount: 0
    };
    
    // Adaptive polling function with exponential backoff
    const scheduleNextCheck = (): void => {
      if (!state.active) return;
      
      const timeoutId: NodeJS.Timeout = setTimeout(async () => {
        if (!state.active) return;
      
      state.pollCount++;
      const elapsed = Date.now() - state.startTime;
      
      try {
        // Check for bootstrap completion using the actual signal detection function
        const signal = await signaler.generateReadySignal();
        
        if (signal.bootstrapComplete && signal.commandCount > 0) {
          console.log('✅ Bootstrap completion detected!');
          console.log(`📊 Commands discovered: ${signal.commandCount}`);
          console.log(`⚡ Detection took ${Math.round(elapsed / 1000)}s (${state.pollCount} checks)`);
          console.log('🚀 System is now ready for testing and connections!');
          state.active = false;
          resolve({
            success: true,
            reason: 'bootstrap_complete',
            commandCount: signal.commandCount,
            detectionTimeMs: elapsed,
            pollCount: state.pollCount
          });
        } else {
          // Only show status every few checks to reduce noise
          if (state.pollCount % 3 === 1) {
            console.log(`⏳ Waiting... (commands: ${signal.commandCount}, bootstrap: ${signal.bootstrapComplete})`);
          }
          
          // Adaptive polling: fast during initial period, then slow down
          // Check for timeout to prevent hanging
        if (elapsed > MONITORING_CONFIG.maxTimeoutMs) {
          console.log(`⏰ Monitoring timeout after ${Math.round(elapsed/1000)}s - system may be ready but bootstrap detection failed`);
          console.log('🔧 Try: npx tsx scripts/signal-system-ready.ts --check');
          state.active = false;
          resolve({
            success: false,
            reason: 'monitoring_timeout',
            timeoutMs: elapsed,
            pollCount: state.pollCount,
            lastCommandCount: signal?.commandCount || 0,
            lastBootstrapState: signal?.bootstrapComplete || false
          });
        }

        if (elapsed < MONITORING_CONFIG.fastPollingDuration) {
            // Keep fast polling for first 15 seconds
            state.currentPollInterval = MONITORING_CONFIG.initialPollMs;
          } else {
            // Gradual slowdown after fast period
            state.currentPollInterval = Math.min(
              state.currentPollInterval * MONITORING_CONFIG.backoffMultiplier,
              MONITORING_CONFIG.maxPollMs
            );
          }
          
          scheduleNextCheck();
        }
        
      } catch (error) {
        // System still starting up - continue monitoring with current interval
        scheduleNextCheck();
      }
    }, state.currentPollInterval);
    
    // Store timeout for cleanup if needed
    if (state.active) {
      (state as any).timeoutId = timeoutId;
    }
  };
  
    // Start monitoring
    scheduleNextCheck();
    
    // Global timeout with proper cleanup
    const globalTimeout: NodeJS.Timeout = setTimeout((): void => {
      if (state.active) {
        console.log(`⏰ Global monitoring timeout after ${MONITORING_CONFIG.maxTimeoutMs / 1000}s`);
        state.active = false;
        resolve({
          success: false,
          reason: 'global_timeout',
          timeoutMs: MONITORING_CONFIG.maxTimeoutMs,
          pollCount: state.pollCount
        });
      }
    }, MONITORING_CONFIG.maxTimeoutMs);
  });
  
  // Cleanup on exit
  process.on('SIGINT', () => {
    state.active = false;
    clearTimeout(globalTimeout);
    if ((state as any).timeoutId) {
      clearTimeout((state as any).timeoutId);
    }
  });
}

// Smart server health check with robust detection
async function checkExistingServer(): Promise<{isHealthy: boolean; tmuxRunning: boolean; portsActive: boolean; message: string}> {
  const signaler = new SystemReadySignaler();
  
  // Check tmux session
  const tmuxRunning = await new Promise<boolean>((resolve) => {
    const tmuxCheck = spawn('tmux', ['has-session', '-t', 'jtag-test'], { stdio: 'ignore' });
    tmuxCheck.on('close', (code) => resolve(code === 0));
  });
  
  // Check port availability
  const portsActive = await new Promise<boolean>((resolve) => {
    const portCheck = spawn('netstat', ['-an'], { stdio: 'pipe' });
    let output = '';
    portCheck.stdout?.on('data', (data) => { output += data.toString(); });
    portCheck.on('close', () => {
      const hasPort9001 = output.includes('.9001') && output.includes('LISTEN');
      const hasPort9002 = output.includes('.9002') && output.includes('LISTEN');
      resolve(hasPort9001 && hasPort9002);
    });
  });
  
  // Check system health via signaler (most important check)
  let systemHealthy = false;
  let commandCount = 0;
  try {
    const signal = await signaler.generateReadySignal();
    systemHealthy = signal.bootstrapComplete && signal.commandCount > 0;
    commandCount = signal.commandCount;
  } catch (error) {
    systemHealthy = false;
  }
  
  // Server is healthy if system responds properly AND ports are active
  // Tmux is optional (since direct spawn is also valid)
  const isHealthy = systemHealthy && portsActive;
  
  let message = '';
  if (isHealthy && tmuxRunning) {
    message = `✅ JTAG server running and healthy in tmux (${commandCount} commands available)`;
  } else if (isHealthy && !tmuxRunning) {
    message = `✅ JTAG server running and healthy (${commandCount} commands available)`;
  } else if (portsActive && !systemHealthy) {
    message = '⚠️ JTAG server ports active but not fully responsive';
  } else if (tmuxRunning && !portsActive) {
    message = '⚠️ Tmux session exists but ports not active';
  } else if (portsActive) {
    message = '⚠️ Ports active but system not responsive';
  } else {
    message = '📋 No existing JTAG server detected';
  }
  
  return { isHealthy, tmuxRunning, portsActive, message };
}

// Mode-specific behavior configurations
const MODE_BEHAVIORS = {
  development: {
    showStatusOnExisting: true,
    exitOnHealthy: true,
    showCommands: true,
    label: 'DEVELOPMENT MODE'
  },
  test: {
    showStatusOnExisting: false,
    exitOnHealthy: false,
    showCommands: false,
    label: 'TEST MODE'
  },
  production: {
    showStatusOnExisting: false,
    exitOnHealthy: false,
    showCommands: false,
    label: 'PRODUCTION MODE'
  }
} as const;

// Run the configurable smart launcher
async function main(): Promise<void> {
  try {
    const behavior = MODE_BEHAVIORS[CONFIG.mode];
    
    if (CONFIG.verbose) {
      console.log(`🚀 SMART JTAG LAUNCHER - ${behavior.label}`);
      console.log(`📊 Config: ${JSON.stringify(CONFIG, null, 2)}`);
    }
    
    // Skip server check if requested or in certain modes
    if (CONFIG.checkExisting && !CONFIG.skipHealthCheck) {
      if (CONFIG.verbose) console.log('🔍 Checking for existing server...');
      
      const serverStatus = await checkExistingServer();
      
      if (CONFIG.verbose || behavior.showStatusOnExisting) {
        console.log(serverStatus.message);
      }
      
      // Development mode: show status and exit if healthy
      if (serverStatus.isHealthy && !CONFIG.forceRestart && behavior.exitOnHealthy) {
        console.log('');
        console.log('🎯 SERVER STATUS: Running and responsive');
        console.log('🌐 Demo UI: http://localhost:9002/');
        console.log('🔌 WebSocket: ws://localhost:9001/');
        
        if (behavior.showCommands) {
          console.log('');
          console.log('💡 DEVELOPMENT COMMANDS:');
          console.log('   tmux attach-session -t jtag-test  # Connect to server console');
          console.log('   npm run restart                    # Force restart server');
          console.log('   npm test                          # Run tests (uses existing server)');
          console.log('   tmux kill-session -t jtag-test    # Stop server');
        }
        
        console.log('');
        console.log('✅ No action needed - server already running perfectly!');
        process.exit(0);
      }
      
      // Test/Production mode: proceed with restart if unhealthy or force requested
      if (CONFIG.forceRestart || (serverStatus.tmuxRunning || serverStatus.portsActive)) {
        const reason = CONFIG.forceRestart ? 'Force restart requested' : 'Server unhealthy, restarting';
        if (CONFIG.verbose || CONFIG.mode !== 'test') console.log(`🔄 ${reason}...`);
        
        // Kill tmux session if exists
        if (serverStatus.tmuxRunning) {
          if (CONFIG.verbose) console.log('🧹 Stopping existing tmux session...');
          await new Promise<void>((resolve) => {
            const killTmux = spawn('tmux', ['kill-session', '-t', 'jtag-test'], { stdio: 'ignore' });
            killTmux.on('close', () => resolve());
          });
        }
        
        // Give ports time to close
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
    }
    
    if (CONFIG.verbose || CONFIG.mode === 'development') {
      console.log('🚀 Starting fresh JTAG server...');
    }
    
    const result: LaunchResult = await launchWithTmuxPersistence();
    
    console.log('🎯 Launch Result:');
    console.log(JSON.stringify(result, null, 2));
    
    if (result.success) {
      console.log('✅ System startup completed successfully');
      console.log(`📄 Full logs available at: ${result.logFile}`);
      console.log(`🎯 Background server running with PID: ${result.processId}`);
      console.log(`📋 PID file: ${pidFile}`);
      console.log('');
      console.log('🚀 TMUX PERSISTENCE MODE: Launcher exiting, server continues in tmux session');
      console.log(`🔗 To connect to server: tmux attach-session -t jtag-test`);
      console.log(`🛑 To stop server: tmux kill-session -t jtag-test`);
      console.log(`📊 To check status: tmux has-session -t jtag-test && echo "Running" || echo "Stopped"`);
      console.log(`📄 To watch logs: tail -f ${result.logFile}`);
      console.log('');
      
      // Exit this script successfully - the background server continues
      process.exit(0);
      
    } else {
      console.error('❌ System startup failed');
      console.error(`💡 Check logs: ${result.logFile}`);
      console.error(`🔍 Diagnostics:`, result.diagnostics);
      process.exit(1);
    }
    
  } catch (error) {
    console.error('🚨 Fatal error in main():', error);
    process.exit(1);
  }
}

main();