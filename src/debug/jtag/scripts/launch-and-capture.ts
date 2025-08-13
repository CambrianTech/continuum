import { exec, spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';

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

async function launchWithKernelLevelDiagnostics(): Promise<LaunchResult> {
  const signaler = new SystemReadySignaler();
  
  console.log('🚀 Launching JTAG system with kernel-level process diagnostics...');
  
  // Clear any old signals first
  await signaler.clearSignals();
  
  return new Promise<LaunchResult>((resolve, reject) => {
    const startTime = Date.now();
    
    // Use spawn() for proper process control instead of exec()
    const child: ChildProcess = spawn('npm', ['run', 'start:direct'], {
      detached: true,   // Detach so the process can continue after this script exits
      stdio: ['ignore', 'pipe', 'pipe'],  // Capture stdout/stderr
      env: {
        ...process.env,
        FORCE_COLOR: '1',
        TERM: 'xterm-256color',
        CI: ''
      },
      cwd: process.cwd()
    });
    
    // Unref the child process so this script can exit
    child.unref();
    
    console.log(`🎯 Process spawned with PID: ${child.pid}`);
    
    // Create diagnostics object with strong typing
    const createDiagnostics = (): ProcessDiagnostics => ({
      pid: child.pid,
      startTime,
      command: 'npm',
      args: ['run', 'start:direct'],
      cwd: process.cwd(),
      exitCode: child.exitCode,
      killed: child.killed,
      signalCode: child.signalCode
    });
    
    // Set up log file streaming
    const logStream = fs.createWriteStream(logFile, { flags: 'w' });
    
    // Pipe output to log file AND console for diagnostics
    child.stdout?.on('data', (data: Buffer) => {
      const text = data.toString();
      logStream.write(text);
      
      // Show key progress indicators
      if (text.includes('smart-build') || text.includes('system:deploy') || text.includes('system:run')) {
        console.log(`📊 Progress: ${text.trim()}`);
      }
    });
    
    child.stderr?.on('data', (data: Buffer) => {
      const text = data.toString();
      logStream.write(`STDERR: ${text}`);
      
      // Show errors immediately
      console.error(`🚨 Error: ${text.trim()}`);
    });
    
    // Set up timeout for server startup detection
    // The npm start process will NOT exit - it's a long-running server
    // We need to detect when the server is actually running
    let serverDetectionTimeout: NodeJS.Timeout;
    let serverStartupDetected = false;
    
    const detectServerStartup = () => {
      serverDetectionTimeout = setTimeout(async () => {
        if (serverStartupDetected) return;
        
        console.log('🔍 Checking if JTAG server is responsive...');
        
        try {
          // Check if the server is actually running by testing readiness
          const signal = await signaler.generateReadySignal();
          
          if (signal.bootstrapComplete && signal.commandCount > 0) {
            serverStartupDetected = true;
            clearTimeout(serverDetectionTimeout);
            
            console.log('✅ JTAG server is running and responsive!');
            console.log(`📊 Commands detected: ${signal.commandCount}`);
            
            resolve({
              success: true,
              reason: 'launch_success',
              processId: child.pid,
              logFile,
              diagnostics: createDiagnostics()
            });
            
            // Don't start monitoring - let the main script exit
            
          } else {
            console.log('⏳ Server still starting up, checking again in 5s...');
            detectServerStartup(); // Try again
          }
          
        } catch (error) {
          console.log('⏳ Server not ready yet, checking again in 5s...');
          detectServerStartup(); // Try again
        }
        
      }, 5000); // Check every 5 seconds
    };
    
    // Start checking for server readiness after initial startup
    setTimeout(detectServerStartup, 10000); // Start checking after 10 seconds
    
    // Handle process completion (this should NOT happen for successful server startup)
    child.on('exit', (code: number | null, signal: NodeJS.Signals | null) => {
      logStream.end();
      clearTimeout(serverDetectionTimeout);
      
      const diagnostics = createDiagnostics();
      
      console.log(`📊 Process exited unexpectedly with code: ${code}, signal: ${signal}`);
      console.log(`🔍 Process diagnostics:`, diagnostics);
      
      if (!serverStartupDetected) {
        // Process exited before server was detected as running - this is a failure
        console.error(`❌ Server process died before becoming responsive`);
        
        resolve({
          success: false,
          reason: 'launch_failure', 
          processId: child.pid,
          logFile,
          errorMessage: `Process exited with code ${code}, signal ${signal} before server became responsive`,
          diagnostics
        });
      }
      // If serverStartupDetected is true, we already resolved successfully
    });
    
    child.on('error', (error: Error) => {
      logStream.end();
      
      const diagnostics = createDiagnostics();
      
      console.error(`🚨 Process spawn error:`, error);
      
      resolve({
        success: false,
        reason: 'process_spawn_error',
        processId: child.pid,
        logFile,
        errorMessage: error.message,
        diagnostics
      });
    });
    
    // Kill process on SIGINT for clean shutdown
    process.on('SIGINT', () => {
      console.log('🛑 Received SIGINT - killing child process...');
      child.kill('SIGTERM');
      setTimeout(() => child.kill('SIGKILL'), 5000); // Force kill after 5s
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

// Run the kernel-level diagnostic launcher
async function main(): Promise<void> {
  try {
    console.log('🔍 KERNEL-LEVEL DIAGNOSTIC LAUNCH - npm test debug mode');
    console.log('📊 This will show exactly where the process hangs');
    
    const result: LaunchResult = await launchWithKernelLevelDiagnostics();
    
    console.log('🎯 Launch Result:');
    console.log(JSON.stringify(result, null, 2));
    
    if (result.success) {
      console.log('✅ System startup completed successfully');
      console.log(`📄 Full logs available at: ${result.logFile}`);
      console.log(`🎯 Background server running with PID: ${result.processId}`);
      console.log('🚀 Launcher script exiting - background server will continue running');
      
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