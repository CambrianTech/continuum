import { exec, spawn } from 'child_process';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';

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
      console.log('📋 Monitor logs with: npm run logs:npm');
      console.log('🔍 Check readiness with: npm run signal:check');
      console.log('🛑 Stop with: npm run system:stop');
      
    } catch (err) {
      console.error(`⚠️ System started but couldn't read PID file`);
    }
  });
}

async function monitorSystemReadiness(signaler: SystemReadySignaler) {
  console.log('👀 Monitoring system logs for readiness indicators...');
  
  let monitoringActive = true;
  
  // Monitor the log file for readiness indicators
  const monitorInterval = setInterval(async () => {
    if (!monitoringActive) {
      clearInterval(monitorInterval);
      return;
    }
    
    try {
      // Check for bootstrap completion using the actual signal detection function
      const signal = await signaler.generateReadySignal();
      
      if (signal.bootstrapComplete && signal.commandCount > 0) {
        console.log('✅ Bootstrap completion detected via signal system!');
        console.log(`📊 Commands discovered: ${signal.commandCount}`);
        console.log('🎯 System ready signal generated successfully!');
        console.log('🚀 System is now ready for testing and AI connections!');
        monitoringActive = false;
        clearInterval(monitorInterval);
        process.exit(0);
      } else {
        // Still waiting for bootstrap to complete
        console.log(`⏳ Waiting for bootstrap... (commands: ${signal.commandCount}, bootstrap: ${signal.bootstrapComplete})`);
      }
      
    } catch (error) {
      // Silently continue monitoring - system may still be starting up
    }
  }, 3000); // Check every 3 seconds
  
  // Stop monitoring after 2 minutes if no signal generated
  setTimeout(() => {
    if (monitoringActive) {
      console.log('⏰ Monitoring timeout - generating final signal check...');
      signaler.generateReadySignal().then(() => {
        console.log('🔄 Final signal generated - check status with: npm run signal:check');
        process.exit(0);
      }).catch(() => {
        console.log('❌ Could not generate final signal');
        process.exit(1);
      });
      monitoringActive = false;
    }
  }, 120000); // 2 minute timeout
}

// Run the enhanced launcher
launchWithIntelligentMonitoring().catch(console.error);