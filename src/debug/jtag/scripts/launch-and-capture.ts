import { exec, spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';
import { TmuxSessionManager } from '../system/shared/TmuxSessionManager';
import { loadInstanceConfigForContext } from '../system/shared/BrowserSafeConfig';

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
const logDir = path.resolve('.continuum/jtag/logs/system');
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
  
  // Generate unique session name based on working directory
  const sessionName = TmuxSessionManager.getSessionName();
  console.log(`📋 Session name: ${sessionName} (workdir-specific to prevent conflicts)`);
  
  // Clear any old signals first
  await signaler.clearSignals();
  
  return new Promise<LaunchResult>((resolve, reject) => {
    const startTime = Date.now();
    
    // First, kill any existing tmux session
    const killSession = spawn('tmux', ['kill-session', '-t', sessionName], {
      stdio: 'ignore'
    });
    
    killSession.on('close', () => {
      // Create tmux session with persistent server and log redirection
      const tmuxCmd = [
        'new-session',
        '-d',                    // detached session
        '-s', sessionName,       // workdir-specific session name
        `npm run start:direct 2>&1 | tee ${logFile}`  // server command with log redirection
      ];
      
      console.log(`🔧 Creating persistent tmux session with logging: tmux ${tmuxCmd.join(' ')}`);
      
      const child: ChildProcess = spawn('tmux', tmuxCmd, {
        stdio: ['ignore', 'pipe', 'pipe'],  // Capture tmux command output
        env: {
          ...process.env,
          FORCE_COLOR: '1',
          TERM: 'xterm-256color',
          NODE_OPTIONS: '--disable-warning=MODULE_TYPELESS_PACKAGE_JSON'
        },
        cwd: process.cwd()
      });
      
      // Set up log file streaming for tmux command output (session creation)
      const logStream = fs.createWriteStream(logFile, { flags: 'w' });
      
      // Log tmux session creation output first
      child.stdout?.pipe(logStream);
      child.stderr?.pipe(logStream);
      
      child.on('close', (code) => {
        logStream.end();
        
        if (code === 0) {
          console.log('✅ Tmux session created successfully');
          
          // Get the PID of the process running inside tmux session
          const getPidCmd = spawn('tmux', [
            'list-panes', '-t', sessionName, '-F', '#{pane_pid}'
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
            
            console.log(`🎯 Tmux session '${sessionName}' created with server PID: ${tmuxPid}`);
            
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
            
            // Wait for PID file to appear (signal that server process started)
            console.log('⏳ Waiting for server process to start...');

            let pidCheckAttempts = 0;
            const maxPidCheckAttempts = 30; // 30 seconds max to wait for PID file

            const pidCheckInterval = setInterval(() => {
              pidCheckAttempts++;

              if (fs.existsSync(pidFile)) {
                clearInterval(pidCheckInterval);
                console.log('✅ Server process started (PID file detected)');
                console.log('🔍 Waiting for JTAG server to be ready');

                // Now start polling for readiness using setInterval (clean, predictable)
                let detectionAttempts = 0;
                const maxDetectionAttempts = 48; // 4 minutes (48 * 5s = 240s)

                const readinessCheckInterval = setInterval(async () => {
                  detectionAttempts++;
                  process.stdout.write('.');

                  try {
                    const signal = await signaler.generateReadySignal();

                    // Exit as soon as bootstrap completes - health may be "unhealthy" during command registration
                    // The ping inside generateReadySignal() already confirms server + browser are responsive
                    const isSystemReady = signal.bootstrapComplete;

                    if (isSystemReady) {
                      clearInterval(readinessCheckInterval);
                      console.log(' ✅');
                      console.log(`🚀 JTAG system fully ready! (${signal.commandCount} commands, health: ${signal.systemHealth})`);

                      resolve({
                        success: true,
                        reason: 'launch_success',
                        processId: tmuxPid,
                        logFile,
                        diagnostics: createDiagnostics()
                      });
                    } else if (detectionAttempts >= maxDetectionAttempts) {
                      clearInterval(readinessCheckInterval);
                      console.log('\n⏰ Server detection timeout - system not fully ready');
                      console.log(`📊 Final Status: bootstrap=${signal.bootstrapComplete}, commands=${signal.commandCount}, health=${signal.systemHealth}`);
                      console.log(`🔧 Check: tmux attach-session -t ${sessionName}`);

                      resolve({
                        success: false,
                        reason: 'launch_failure',
                        processId: tmuxPid,
                        logFile,
                        errorMessage: 'System launched but not fully ready after 4 minutes',
                        diagnostics: createDiagnostics()
                      });
                    } else if (detectionAttempts % 5 === 0) {
                      // Show status every 25 seconds
                      console.log(`\n📊 Status: bootstrap=${signal.bootstrapComplete}, commands=${signal.commandCount}, health=${signal.systemHealth}`);
                      process.stdout.write('🔍 Still waiting');
                    }
                  } catch (error) {
                    // Server not responding yet - continue polling
                    if (detectionAttempts >= maxDetectionAttempts) {
                      clearInterval(readinessCheckInterval);
                      console.log('\n⏰ Server detection timeout');
                      console.log(`💥 Error: ${error instanceof Error ? error.message : String(error)}`);

                      resolve({
                        success: false,
                        reason: 'launch_failure',
                        processId: tmuxPid,
                        logFile,
                        errorMessage: 'System launched but readiness check timed out',
                        diagnostics: createDiagnostics()
                      });
                    }
                  }
                }, 5000); // Check every 5 seconds

              } else if (pidCheckAttempts >= maxPidCheckAttempts) {
                clearInterval(pidCheckInterval);
                console.log('\n⏰ PID file never appeared - server process may have failed to start');
                console.log(`📄 Check build logs: tail -50 ${logFile}`);

                resolve({
                  success: false,
                  reason: 'launch_failure',
                  processId: tmuxPid,
                  logFile,
                  errorMessage: 'Server process did not start (PID file not created)',
                  diagnostics: createDiagnostics()
                });
              }
            }, 1000); // Check for PID file every second
            
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
  const sessionName = TmuxSessionManager.getSessionName();
  
  // Check tmux session
  const tmuxRunning = await new Promise<boolean>((resolve) => {
    const tmuxCheck = spawn('tmux', ['has-session', '-t', sessionName], { stdio: 'ignore' });
    tmuxCheck.on('close', (code) => resolve(code === 0));
  });
  
  const instanceConfig = loadInstanceConfigForContext();
  const wsPort = instanceConfig.ports.websocket_server;
  const httpPort = instanceConfig.ports.http_server;
  
  console.log(`🔍 Checking configured ports: WS=${wsPort}, HTTP=${httpPort}`);
  
  // Check port availability using configured ports (not hardcoded)
  const portsActive = await new Promise<boolean>((resolve) => {
    const portCheck = spawn('netstat', ['-an'], { stdio: 'pipe' });
    let output = '';
    portCheck.stdout?.on('data', (data) => { output += data.toString(); });
    portCheck.on('close', () => {
      const hasWSPort = output.includes(`.${wsPort}`) && output.includes('LISTEN');
      const hasHTTPPort = output.includes(`.${httpPort}`) && output.includes('LISTEN');
      console.log(`🔍 Port status: WS=${wsPort} ${hasWSPort ? '✅' : '❌'}, HTTP=${httpPort} ${hasHTTPPort ? '✅' : '❌'}`);
      resolve(hasWSPort && hasHTTPPort);
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

/**
 * Kill old npm/node processes from previous runs
 * Only kills processes that match our patterns (npm, node, tsx running our scripts)
 */
async function killOurOwnProcesses(): Promise<void> {
  const ourWorkDir = process.cwd();
  console.log(`🔧 Finding and killing old npm/node processes from: ${ourWorkDir}`);

  // Use ps to find npm/node/tsx processes in our directory
  await new Promise<void>((resolve) => {
    const psCheck = spawn('ps', ['aux'], { stdio: 'pipe' });
    let output = '';
    psCheck.stdout?.on('data', (data) => { output += data.toString(); });
    psCheck.on('close', () => {
      const lines = output.split('\n');
      const pids = new Set<number>();

      for (const line of lines) {
        // Skip if not our working directory
        if (!line.includes(ourWorkDir)) continue;

        // Only kill npm, node, tsx processes (not bash, not other scripts)
        if (!line.match(/\b(npm|node|tsx)\b/)) continue;

        // Parse PID (second column)
        const parts = line.trim().split(/\s+/);
        if (parts.length > 1) {
          const pid = parseInt(parts[1]);

          // Don't kill ourselves or our parent
          if (pid && pid !== process.pid && pid !== process.ppid) {
            pids.add(pid);
          }
        }
      }

      if (pids.size > 0) {
        console.log(`💀 Killing ${pids.size} old npm/node processes: ${Array.from(pids).join(', ')}`);
        for (const pid of pids) {
          try {
            process.kill(pid, 'SIGKILL');
          } catch (error) {
            // Process might have already died, that's fine
          }
        }
      } else {
        console.log(`✅ No old npm/node processes found`);
      }
      resolve();
    });

    // If ps fails or times out, continue anyway
    setTimeout(() => resolve(), 5000);
  });

  // Give processes time to die
  await new Promise(resolve => setTimeout(resolve, 2000));
  console.log(`✅ Process cleanup complete`);
}

/**
 * Force kill processes using the configured ports to prevent conflicts
 */
async function forcePortTakeover(): Promise<void> {
  const instanceConfig = loadInstanceConfigForContext();
  const wsPort = instanceConfig.ports.websocket_server;
  const httpPort = instanceConfig.ports.http_server;

  console.log(`🔧 Force taking over configured ports: WS=${wsPort}, HTTP=${httpPort}`);

  // Use lsof to find and kill processes using our ports
  for (const port of [wsPort, httpPort]) {
    await new Promise<void>((resolve) => {
      const lsofCheck = spawn('lsof', ['-ti', `:${port}`], { stdio: 'pipe' });
      let pids = '';
      lsofCheck.stdout?.on('data', (data) => { pids += data.toString(); });
      lsofCheck.on('close', () => {
        const pidList = pids.trim().split('\n').filter(pid => pid);
        if (pidList.length > 0) {
          console.log(`💀 Killing processes on port ${port}: ${pidList.join(', ')}`);
          for (const pid of pidList) {
            try {
              process.kill(parseInt(pid), 'SIGKILL');
            } catch (error) {
              console.warn(`⚠️ Could not kill PID ${pid}: ${error}`);
            }
          }
        }
        resolve();
      });
    });
  }

  // Give processes time to die
  await new Promise(resolve => setTimeout(resolve, 2000));
  console.log(`✅ Port takeover complete`);
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
    // CRITICAL: Initialize SecretManager to load config.env into process.env
    const { SecretManager } = await import('../system/secrets/SecretManager');
    await SecretManager.getInstance().initialize();

    const behavior = MODE_BEHAVIORS[CONFIG.mode];
    const sessionName = TmuxSessionManager.getSessionName(); // Generate session name for this workdir

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
        const instanceConfig = loadInstanceConfigForContext();
        const wsPort = instanceConfig.ports.websocket_server;
        const httpPort = instanceConfig.ports.http_server;

        console.log('');
        console.log('🎯 SERVER STATUS: Running and responsive');
        console.log(`🌐 ${instanceConfig.name}: http://localhost:${httpPort}/`);
        console.log(`🔌 WebSocket: ws://localhost:${wsPort}/`);

        // Check if browser is connected via ping, then refresh AND open
        console.log('🔄 Checking browser connection...');
        try {
          const browserUrl = `http://localhost:${httpPort}/`;

          // Check ping to see if browser is connected
          const pingResult = await new Promise<{ browserConnected: boolean; browserUrl?: string }>((resolve) => {
            exec('./jtag ping', { timeout: 5000 }, (error, stdout) => {
              if (error) {
                resolve({ browserConnected: false });
              } else {
                try {
                  const result = JSON.parse(stdout);
                  // Browser is connected if ping returns browser info
                  const connected = result.browser && result.browser.type === 'browser';
                  resolve({
                    browserConnected: connected,
                    browserUrl: result.browser?.url
                  });
                } catch {
                  resolve({ browserConnected: false });
                }
              }
            });
          });

          if (pingResult.browserConnected) {
            // Browser is connected - refresh it
            console.log('🔄 Browser connected, refreshing...');
            exec('./jtag interface/navigate', { timeout: 5000 }, () => {});
          }

          // ALWAYS open browser to ensure user sees something
          // Opening the URL will focus existing tab or open new one
          console.log('🌐 Opening browser...');
          spawn('open', [browserUrl], { detached: true, stdio: 'ignore' }).unref();
          console.log(`✅ Browser opened: ${browserUrl}`);
        } catch {
          // Browser sync is best-effort, don't fail startup
        }

        if (behavior.showCommands) {
          console.log('');
          console.log('💡 DEVELOPMENT COMMANDS:');
          console.log(`   tmux attach-session -t ${sessionName}  # Connect to server console`);
          console.log('   npm run restart                    # Force restart server');
          console.log('   npm test                          # Run tests (uses existing server)');
          console.log(`   tmux kill-session -t ${sessionName}    # Stop server`);
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
            const killTmux = spawn('tmux', ['kill-session', '-t', sessionName], { stdio: 'ignore' });
            killTmux.on('close', () => resolve());
          });
        }

        // CRITICAL: Kill ALL our own processes, not just those on ports
        await killOurOwnProcesses();

        if (serverStatus.portsActive) {
          await forcePortTakeover();
        } else {
          // Give ports time to close naturally if no force needed
          await new Promise(resolve => setTimeout(resolve, 2000));
        }
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
      console.log(`🔗 To connect to server: tmux attach-session -t ${sessionName}`);
      console.log(`🛑 To stop server: tmux kill-session -t ${sessionName}`);
      console.log(`📊 To check status: tmux has-session -t ${sessionName} && echo "Running" || echo "Stopped"`);
      console.log(`📄 To watch logs: tail -f ${result.logFile}`);
      console.log('');
      console.log('📊 INTELLIGENT LOG DASHBOARD:');
      console.log('   npm run logs:dashboard                   # Smart AI/Human detection');
      console.log('   npm run logs:ai                          # AI-friendly structured output');
      console.log('   npm run logs:human                       # Human tmux dashboard');  
      console.log('   npm run logs:status                      # Current status');
      console.log('');
      console.log('💡 PRO TIP: Log dashboard will auto-launch after npm start completes!');
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