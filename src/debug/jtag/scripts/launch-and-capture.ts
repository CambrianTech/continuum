import { exec } from 'child_process';
import fs from 'fs';
import path from 'path';

// Prepare paths
const logDir = path.resolve('.continuum/jtag/system/logs');
const logFile = path.join(logDir, 'npm-start.log');
const pidFile = path.join(logDir, 'npm-start.pid');

// Ensure directory exists
fs.mkdirSync(logDir, { recursive: true });

// Build the command with proper environment and output redirection
const cmd = `FORCE_COLOR=1 TERM=xterm-256color CI= nohup npm start > "${logFile}" 2>&1 & echo $! > "${pidFile}"`;

// Execute the command
exec(cmd, (error, stdout, stderr) => {
  if (error) {
    console.error(`❌ Failed to start JTAG system: ${error.message}`);
    process.exit(1);
  }
  
  // Read the PID that was written
  try {
    const pid = fs.readFileSync(pidFile, 'utf8').trim();
    console.log(`🚀 JTAG system started in background (PID: ${pid})`);
    console.log(`📄 Full logging to: ${logFile}`);
    console.log(`📋 Monitor with: npm run logs:npm`);
    console.log(`🛑 Stop with: npm run system:stop`);
  } catch (err) {
    console.error(`⚠️  System started but couldn't read PID file`);
  }
});