#!/usr/bin/env tsx
/**
 * Wait for Tmux Session to be Ready
 * 
 * Waits for the JTAG tmux session to be fully established with:
 * 1. Tmux session 'jtag-test' running
 * 2. WebSocket server active on port 9001  
 * 3. HTTP server active on port 9002
 * 4. Browser interface accessible and responding
 * 
 * This replaces timeout-based approaches with proper startup dependency checking.
 * Tests should only start after the entire system (including browser) is ready.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { getActivePorts } from '../system/shared/ExampleConfig';

const execAsync = promisify(exec);

interface SystemReadinessCheck {
  tmuxRunning: boolean;
  websocketActive: boolean;
  httpActive: boolean;
  browserReady: boolean;
  errors: string[];
  failures: string[];
  systemCrashed: boolean;
}

/**
 * Check if tmux session is running and active
 * Now supports dynamic session names (jtag-test-bench-*, jtag-widget-ui-*)
 */
async function checkTmuxSession(): Promise<{ running: boolean; sessionName?: string }> {
  try {
    // First try exact match for backward compatibility
    const { stdout: exactMatch } = await execAsync('tmux has-session -t jtag-test 2>/dev/null');
    return { running: true, sessionName: 'jtag-test' };
  } catch {
    // Then try dynamic session name matching
    try {
      const { stdout: sessions } = await execAsync('tmux list-sessions -F "#{session_name}" 2>/dev/null');
      const sessionLines = sessions.split('\n').filter(line => line.trim());
      
      // Look for JTAG sessions (test-bench or widget-ui)
      const jtagSession = sessionLines.find(name => 
        name.startsWith('jtag-test-bench-') || 
        name.startsWith('jtag-widget-ui-') ||
        name === 'jtag-test'
      );
      
      if (jtagSession) {
        return { running: true, sessionName: jtagSession };
      }
    } catch {
      // No tmux server running
    }
    
    return { running: false };
  }
}

/**
 * Check if specific port is active and listening
 */
async function checkPort(port: number): Promise<boolean> {
  try {
    const { stdout } = await execAsync(`lsof -i :${port} -t 2>/dev/null`);
    return stdout.trim().length > 0;
  } catch {
    return false;
  }
}

/**
 * Check if browser interface is ready and responding
 */
async function checkBrowserInterface(httpPort: number): Promise<boolean> {
  try {
    const { stdout } = await execAsync(`curl -s --max-time 2 http://localhost:${httpPort} 2>/dev/null`);
    return stdout.includes('Continuum') || stdout.includes('JTAG');
  } catch {
    return false;
  }
}

/**
 * Check for system failures and crashes using dynamic session name
 */
async function checkSystemFailures(sessionName?: string): Promise<{ failures: string[]; systemCrashed: boolean }> {
  const failures: string[] = [];
  let systemCrashed = false;
  
  try {
    // Check tmux session output for critical errors (using dynamic session name)
    const tmuxCmd = sessionName ? `tmux capture-pane -t ${sessionName} -p` : 'echo "No session"';
    const { stdout: tmuxOutput } = await execAsync(`${tmuxCmd} 2>/dev/null || echo ""`);
    
    if (tmuxOutput.includes('ERROR') || tmuxOutput.includes('EADDRINUSE')) {
      failures.push('Port conflict detected in tmux session');
    }
    
    if (tmuxOutput.includes('Failed to create') || tmuxOutput.includes('Cannot connect')) {
      failures.push('System component creation failed');
    }
    
    if (tmuxOutput.includes('exit code 1') || tmuxOutput.includes('Process terminated')) {
      systemCrashed = true;
      failures.push('System process crashed');
    }
    
    // Check system logs for critical failures
    try {
      const { stdout: errorLogs } = await execAsync('tail -20 .continuum/jtag/system/logs/npm-start.log 2>/dev/null | grep -i "error\\|failed\\|crash" || echo ""');
      if (errorLogs.trim().length > 0) {
        const errorLines = errorLogs.split('\n').filter(line => line.trim());
        if (errorLines.length > 3) { // More than 3 recent errors suggests system failure
          systemCrashed = true;
          failures.push('Multiple critical errors in system logs');
        }
      }
    } catch {
      // Log file may not exist yet during startup
    }
    
  } catch {
    // tmux session might not exist yet
  }
  
  return { failures, systemCrashed };
}

/**
 * Perform comprehensive system readiness check
 */
async function checkSystemReadiness(): Promise<SystemReadinessCheck & { sessionName?: string }> {
  const activePorts = getActivePorts();
  const websocketPort = activePorts.websocket_server;
  const httpPort = activePorts.http_server;
  
  const tmuxCheck = await checkTmuxSession();
  
  const [websocketActive, httpActive, browserReady, failureCheck] = await Promise.all([
    checkPort(websocketPort),
    checkPort(httpPort),
    checkBrowserInterface(httpPort),
    checkSystemFailures(tmuxCheck.sessionName)
  ]);
  
  const errors: string[] = [];
  if (!tmuxCheck.running) {
    errors.push(`Tmux session not running (looking for jtag-test-bench-* or jtag-widget-ui-*)`);
  }
  if (!websocketActive) errors.push(`WebSocket server not active on port ${websocketPort}`);
  if (!httpActive) errors.push(`HTTP server not active on port ${httpPort}`);
  if (!browserReady) errors.push(`Browser interface not responding on http://localhost:${httpPort}`);
  
  return {
    tmuxRunning: tmuxCheck.running,
    websocketActive, 
    httpActive,
    browserReady,
    errors,
    failures: failureCheck.failures,
    systemCrashed: failureCheck.systemCrashed,
    sessionName: tmuxCheck.sessionName
  };
}

/**
 * Wait for system to become ready with proper timeout and progress reporting
 */
async function waitForSystemReady(timeoutSeconds: number = 45): Promise<void> {
  console.log('🔄 Waiting for JTAG system to be fully ready (tmux + browser + servers)...');
  
  const startTime = Date.now();
  const timeoutMs = timeoutSeconds * 1000;
  let lastErrorCount = -1;
  
  while (Date.now() - startTime < timeoutMs) {
    const readiness = await checkSystemReadiness();
    
    // Check for critical system failures - exit immediately
    if (readiness.systemCrashed) {
      console.log('💥 System crashed - aborting startup wait');
      console.log('🔍 Failure reasons:');
      readiness.failures.forEach(failure => console.log(`  - ${failure}`));
      console.log('🛠️  Check logs: .continuum/jtag/system/logs/npm-start.log');
      console.log('🛠️  Try: npm run system:stop && npm run system:start');
      process.exit(1);
    }
    
    // Report failures but continue waiting (might be temporary)
    if (readiness.failures.length > 0) {
      console.log(`⚠️  Detected ${readiness.failures.length} system issues (continuing to wait):`);
      readiness.failures.forEach(failure => console.log(`  - ${failure}`));
    }
    
    // Report progress only when errors change to reduce noise
    if (readiness.errors.length !== lastErrorCount) {
      if (readiness.errors.length === 0) {
        console.log('✅ System fully ready - all dependencies satisfied');
        console.log(`  ✅ Tmux session: ${readiness.sessionName || 'running'}`);
        console.log(`  ✅ WebSocket server: active on port ${getActivePorts().websocket_server}`);
        console.log(`  ✅ HTTP server: active on port ${getActivePorts().http_server}`);
        console.log(`  ✅ Browser interface: responding`);
        return;
      } else {
        console.log(`⏳ Waiting for ${readiness.errors.length} dependencies:`);
        readiness.errors.forEach(error => console.log(`  - ${error}`));
        if (readiness.sessionName) {
          console.log(`  ℹ️ Found session: ${readiness.sessionName}`);
        }
      }
      lastErrorCount = readiness.errors.length;
    }
    
    // Wait before next check (exponential backoff for responsiveness)
    const elapsedSeconds = (Date.now() - startTime) / 1000;
    const delay = elapsedSeconds < 10 ? 500 : elapsedSeconds < 30 ? 1000 : 2000;
    await new Promise(resolve => setTimeout(resolve, delay));
  }
  
  // Final check and error reporting
  const finalReadiness = await checkSystemReadiness();
  console.log(`❌ System failed to become ready within ${timeoutSeconds} seconds`);
  console.log('🔍 Final status:');
  console.log(`  Tmux session: ${finalReadiness.tmuxRunning ? '✅' : '❌'}`);
  console.log(`  WebSocket server: ${finalReadiness.websocketActive ? '✅' : '❌'}`);
  console.log(`  HTTP server: ${finalReadiness.httpActive ? '✅' : '❌'}`);
  console.log(`  Browser interface: ${finalReadiness.browserReady ? '✅' : '❌'}`);
  
  if (finalReadiness.errors.length > 0) {
    console.log('🔧 Remaining issues:');
    finalReadiness.errors.forEach(error => console.log(`  - ${error}`));
  }
  
  process.exit(1);
}

// Run when called directly
if (require.main === module) {
  waitForSystemReady().catch(error => {
    console.error('❌ Failed to wait for system readiness:', error.message);
    process.exit(1);
  });
}