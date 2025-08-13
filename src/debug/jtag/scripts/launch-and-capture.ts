import { exec, spawn } from 'child_process';
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

async function launchWithIntelligentMonitoring() {
  const signaler = new SystemReadySignaler();
  
  console.log('🚀 Launching JTAG system with intelligent readiness monitoring...');
  
  // Clear any old signals first
  await signaler.clearSignals();
  
  // Build the command with proper environment and output redirection  
  const cmd = `FORCE_COLOR=1 TERM=xterm-256color CI= nohup npm run start:direct > "${logFile}" 2>&1 & echo $! > "${pidFile}"`;

  // Execute the startup command
  exec(cmd, async (error, stdout, stderr) => {
    if (error) {
      console.error(`❌ Failed to start JTAG system: ${error.message}`);
      process.exit(1);
    }
    
    // Read the PID that was written
    try {
      const pid = fs.readFileSync(pidFile, 'utf8').trim();
      console.log(`🚀 JTAG system started in background (PID: ${pid})`);
      console.log(`📄 Full logging to: ${logFile}`);
      console.log(`🧠 Starting intelligent readiness monitoring...`);
      
      // Start monitoring for readiness in the background
      monitorSystemReadiness(signaler);
      
      console.log('');
      console.log('📋 Monitor logs: npm run logs:npm');
      console.log('🔍 Check status: npm run signal:check');
      console.log('🛑 Stop system: npm run system:stop');
      
    } catch (err) {
      console.error(`⚠️ System started but couldn't read PID file`);
    }
  });
}

async function monitorSystemReadiness(signaler: SystemReadySignaler): Promise<void> {
  console.log('👀 Monitoring system readiness...');
  
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
          process.exit(0);
        } else {
          // Only show status every few checks to reduce noise
          if (state.pollCount % 3 === 1) {
            console.log(`⏳ Waiting... (commands: ${signal.commandCount}, bootstrap: ${signal.bootstrapComplete})`);
          }
          
          // Adaptive polling: fast during initial period, then slow down
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
      console.log(`⏰ Monitoring timeout after ${MONITORING_CONFIG.maxTimeoutMs / 1000}s - generating final signal check...`);
      signaler.generateReadySignal().then(() => {
        console.log('🔄 Final signal generated - check status with: npm run signal:check');
        process.exit(0);
      }).catch(() => {
        console.log('❌ Could not generate final signal');
        console.log('🔍 Check system logs: .continuum/jtag/system/logs/npm-start.log');
        process.exit(1);
      });
      state.active = false;
    }
  }, MONITORING_CONFIG.maxTimeoutMs);
  
  // Cleanup on exit
  process.on('SIGINT', () => {
    state.active = false;
    clearTimeout(globalTimeout);
    if ((state as any).timeoutId) {
      clearTimeout((state as any).timeoutId);
    }
  });
}

// Run the enhanced launcher
launchWithIntelligentMonitoring().catch(console.error);