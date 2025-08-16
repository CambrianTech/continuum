#!/usr/bin/env tsx
/**
 * System Ready Signal Generator
 * 
 * This script is called by the system startup process when all components are ready.
 * It creates signal files that tests can monitor instead of using fixed sleep timers.
 * 
 * Usage:
 *   npm run signal:ready          # Called by system startup
 *   npx tsx scripts/signal-system-ready.ts --check  # Check if system is ready
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import { exec } from 'child_process';
import { promisify } from 'util';
import { JTAG_LOG_PATTERNS } from '../system/core/client/shared/JTAGClientConstants';
import { getSystemConfig } from '../system/core/config/SystemConfiguration';

const execAsync = promisify(exec);

type SystemHealth = 'healthy' | 'degraded' | 'unhealthy' | 'error';
type BuildStatus = 'success' | 'failed' | 'in-progress' | 'unknown';

interface SystemReadySignal {
  readonly timestamp: string;
  readonly bootstrapComplete: boolean;
  readonly commandCount: number;
  readonly portsActive: readonly number[];
  readonly systemHealth: SystemHealth;
  readonly readySignalVersion: string;
  readonly errors: readonly string[];
  readonly startupLogs: string;
  readonly nodeErrors: readonly string[];
  readonly compilationStatus: BuildStatus;
  readonly browserReady: boolean;
  readonly buildStatus: BuildStatus;
  readonly autonomousGuidance: readonly string[];
}

function getSignalConfig() {
  const systemConfig = getSystemConfig();
  return {
    VERSION: '1.0.0',
    EXPECTED_PORTS: [
      systemConfig.getWebSocketPort(),
      systemConfig.getHTTPPort()
    ].filter(port => port > 0) as readonly number[], // Filter out disabled ports
    MIN_COMMAND_COUNT: 1, // Reduced - we now use process registry for readiness
    DEFAULT_TIMEOUT_MS: 30000 // Reduced timeout since registry-based detection is faster
  } as const;
}

class SystemReadySignaler {
  private signalDir = '.continuum/jtag/signals';
  private readyFile = path.join(this.signalDir, 'system-ready.json');
  private pidFile = path.join(this.signalDir, 'system.pid');

  async generateReadySignal(): Promise<SystemReadySignal> {
    // Don't log "Generating" - it's confusing since we're checking, not generating
    try {
      // Ensure signal directory exists
      await fs.mkdir(this.signalDir, { recursive: true });

      // Gather system readiness metrics
      const signal = await this.collectSystemMetrics();
      
      // Write signal file atomically
      await this.writeSignalFile(signal);
      
      // Write PID file for system tracking
      await this.writePidFile();
      
      // Only log meaningful state changes, not every check
      const isHealthy = signal.systemHealth === 'healthy';
      const hasBootstrap = signal.bootstrapComplete;
      const hasCommands = signal.commandCount > 0;
      
      // Only log on first healthy detection or errors
      if (isHealthy && hasBootstrap && hasCommands) {
        console.log(`✅ System healthy: ${signal.commandCount} commands ready`);
        console.log(`🌐 Active ports: ${signal.portsActive.join(', ')}`);
      } else if (signal.systemHealth === 'error' && signal.errors.length > 0) {
        // Only log errors once to avoid spam
        const errorKey = signal.errors.join('|');
        if (!this.lastErrorKey || this.lastErrorKey !== errorKey) {
          console.log(`❌ System error: ${signal.errors[0]}`);
          this.lastErrorKey = errorKey;
        }
      }
      
      return signal;
      
    } catch (error: any) {
      console.error('❌ System check failed:', error.message);
      throw error; // Don't exit, let caller handle
    }
  }

  private lastErrorKey?: string;

  async checkSystemReady(timeoutMs = 10000): Promise<SystemReadySignal | null> {
    console.log(`🔍 Checking system readiness (timeout: ${timeoutMs}ms)...`);
    
    const startTime = Date.now();
    let attemptCount = 0;
    const maxAttempts = Math.max(5, Math.floor(timeoutMs / 2000)); // At least 5 attempts, more for longer timeouts
    
    while (Date.now() - startTime < timeoutMs && attemptCount < maxAttempts) {
      attemptCount++;
      const elapsed = Date.now() - startTime;
      
      console.log(`🔄 Attempt ${attemptCount}/${maxAttempts} (${elapsed}ms elapsed)`);
      
      try {
        // Check if signal file exists and is recent
        const stats = await fs.stat(this.readyFile);
        const fileAge = Date.now() - stats.mtime.getTime();
        
        // Signal file should be recent (within last 5 minutes)
        if (fileAge > 5 * 60 * 1000) {
          console.log('⚠️ Signal file exists but is stale, waiting...');
          await this.sleep(2000);
          continue;
        }
        
        // Read and validate signal
        const signalData = await fs.readFile(this.readyFile, 'utf-8');
        const signal: SystemReadySignal = JSON.parse(signalData);
        
        console.log(`📊 Signal status: health=${signal.systemHealth}, bootstrap=${signal.bootstrapComplete}, commands=${signal.commandCount}`);
        
        // Enhanced readiness check - system must be healthy AND have bootstrap complete
        if ((signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded') && 
            signal.bootstrapComplete && 
            signal.commandCount > 0) {
          console.log('✅ System ready signal confirmed');
          return signal;
        } else if (signal.systemHealth === 'error') {
          // If system is in error state, don't keep waiting indefinitely
          console.log(`❌ System in error state: ${signal.errors?.join(', ')}`);
          console.log('🔍 Check logs: npm run signal:logs');
          console.log('🔧 Try restart: npm run system:restart');
          return signal; // Return error signal so caller can handle
        } else {
          console.log(`⚠️ System not fully ready: ${signal.systemHealth}, bootstrap: ${signal.bootstrapComplete}, commands: ${signal.commandCount}`);
          if (signal.autonomousGuidance?.length) {
            console.log('💡 Guidance:', signal.autonomousGuidance[0]);
          }
        }
        
      } catch (error) {
        // Signal file doesn't exist yet or parsing failed
        console.log(`⚠️ No valid signal file yet (attempt ${attemptCount})`);
      }
      
      // Avoid infinite loops with escalating sleep times and max attempts
      const sleepTime = Math.min(2000 + (attemptCount * 500), 5000); // Start at 2s, escalate to max 5s
      console.log(`⏳ Waiting ${sleepTime}ms before next check...`);
      await this.sleep(sleepTime);
    }
    
    // Timeout or max attempts reached
    if (attemptCount >= maxAttempts) {
      console.log(`❌ Max attempts (${maxAttempts}) reached waiting for system ready signal`);
    } else {
      console.log(`❌ Timeout (${timeoutMs}ms) waiting for system ready signal`);
    }
    
    console.log('🔍 Check system status: npm run system:debug');
    console.log('🔧 Try manual restart: npm run system:restart');
    return null;
  }

  private async collectSystemMetrics(): Promise<SystemReadySignal> {
    const metrics: {
      -readonly [K in keyof SystemReadySignal]?: SystemReadySignal[K]
    } = {
      timestamp: new Date().toISOString(),
      readySignalVersion: getSignalConfig().VERSION,
      errors: []
    };

    try {
      // Check compilation status first (critical for startup)
      metrics.compilationStatus = await this.checkCompilation();
      
      // Check for node/startup errors
      metrics.nodeErrors = await this.captureNodeErrors();
      
      // Capture startup logs for diagnostics
      metrics.startupLogs = await this.captureStartupLogs();
      
      // Check bootstrap completion
      metrics.bootstrapComplete = await this.checkBootstrap();
      
      // Count discovered commands
      metrics.commandCount = await this.countCommands();
      
      // Check active ports
      metrics.portsActive = await this.getActivePorts([...getSignalConfig().EXPECTED_PORTS]);
      
      // Check browser readiness (can we reach the demo page?)
      metrics.browserReady = await this.checkBrowserReady();
      
      // Check build status
      metrics.buildStatus = await this.checkBuildStatus();
      
      // Collect any errors found
      metrics.errors = await this.collectSystemErrors(metrics.bootstrapComplete, metrics.commandCount);
      
      // Generate autonomous guidance
      metrics.autonomousGuidance = this.generateAutonomousGuidance(metrics);
      
      // Determine overall health
      metrics.systemHealth = this.calculateSystemHealth(metrics);
      
    } catch (error: any) {
      console.warn('⚠️ Error collecting metrics:', error.message);
      metrics.systemHealth = 'error';
      metrics.bootstrapComplete = false;
      metrics.commandCount = 0;
      metrics.portsActive = [];
      metrics.errors = [error.message];
    }

    return metrics as SystemReadySignal;
  }

  private async checkBootstrap(): Promise<boolean> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      // Use fixed string pattern to avoid shell escaping issues
      const { stdout } = await execAsync(`grep -F "Bootstrap complete! Discovered" ${logPath} | grep "commands" 2>/dev/null || echo ""`);
      return stdout.trim().length > 0;
    } catch {
      return false;
    }
  }

  private async countCommands(): Promise<number> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      // Get all bootstrap messages and take the LAST one (most recent)
      const { stdout } = await execAsync(`grep -F "Bootstrap complete! Discovered" ${logPath} | grep "commands" 2>/dev/null || echo ""`);
      
      if (!stdout.trim()) {
        return 0;
      }
      
      // Split lines and get the last match
      const lines = stdout.trim().split('\n');
      const lastLine = lines[lines.length - 1];
      const match = lastLine.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
      return match ? parseInt(match[1]) : 0;
    } catch {
      return 0;
    }
  }

  private async getActivePorts(ports: number[]): Promise<number[]> {
    const activePorts: number[] = [];
    
    for (const port of ports) {
      try {
        const { stdout } = await execAsync(`lsof -ti:${port} 2>/dev/null || echo ""`);
        if (stdout.trim()) {
          activePorts.push(port);
        }
      } catch {
        // Port not active
      }
    }
    
    return activePorts;
  }

  private async checkCompilation(): Promise<'success' | 'failed' | 'unknown'> {
    try {
      // Check npm-start.log for TypeScript compilation results
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -100 ${startupLog} 2>/dev/null | grep -E "(tsc|build:ts|esbuild|Error|error|Could not resolve)" | tail -20 || echo ""`);
      
      // Only flag actual TERMINAL compilation errors, not transient ones
      const hasFinalBuildFailure = stdout.includes('npm ERR!') && stdout.includes('build:ts');
      const hasFinalResolutionError = stdout.includes('Could not resolve') && stdout.includes('esbuild') && stdout.includes('error') && !stdout.includes('✅');
      
      // Success indicators
      const hasSuccessfulBuild = stdout.includes('build:ts') && !hasFinalBuildFailure;
      const hasCleanCompletion = stdout.includes('✅') || (stdout.includes('tsc') && !hasFinalBuildFailure);
      
      if (hasFinalBuildFailure || hasFinalResolutionError) {
        console.log('🚨 Compilation failure detected:');
        if (hasFinalResolutionError) console.log('  - Import resolution failure');
        if (hasFinalBuildFailure) console.log('  - Build process failure');
        return 'failed';
      } else if (hasSuccessfulBuild || hasCleanCompletion) {
        return 'success';
      }
      return 'unknown';
    } catch {
      return 'unknown';
    }
  }

  private async captureNodeErrors(): Promise<string[]> {
    const errors: string[] = [];
    
    try {
      // Check system startup logs for node errors
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -100 ${startupLog} 2>/dev/null | grep -i -E "(error|exception|failed|cannot|unable)" | tail -10 || echo ""`);
      
      if (stdout.trim()) {
        errors.push(...stdout.split('\n').filter(line => line.trim()));
      }
      
      // Check server console logs for runtime errors
      const serverLog = 'examples/test-bench/.continuum/jtag/sessions/system/*/logs/server-console-log.log';
      const { stdout: serverErrors } = await execAsync(`tail -50 ${serverLog} 2>/dev/null | grep -i "error" | tail -5 || echo ""`);
      
      if (serverErrors.trim()) {
        errors.push(...serverErrors.split('\n').filter(line => line.trim()));
      }
      
    } catch (error: any) {
      errors.push(`Error capturing node logs: ${error.message}`);
    }
    
    return errors;
  }

  private async captureStartupLogs(): Promise<string> {
    try {
      // Capture recent startup logs for diagnostic purposes
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -50 ${startupLog} 2>/dev/null || echo "No startup logs found"`);
      return stdout;
    } catch (error: any) {
      return `Error capturing startup logs: ${error.message}`;
    }
  }

  private async collectSystemErrors(bootstrapComplete: boolean, commandCount: number): Promise<string[]> {
    const errors: string[] = [];
    
    try {
      // CRITICAL: Check server console errors for daemon creation failures
      if (!bootstrapComplete && commandCount === 0) {
        // Check server console error logs for daemon creation failures
        const serverErrorLog = 'examples/test-bench/.continuum/jtag/system/logs/server-console-error.log';
        const { stdout: daemonErrors } = await execAsync(`tail -20 ${serverErrorLog} 2>/dev/null | grep "Failed to create.*daemon\\|Unknown storage adapter type" | tail -3 || echo ""`);
        
        if (daemonErrors.trim()) {
          errors.push('🚨 DAEMON CREATION FAILED:');
          errors.push(...daemonErrors.split('\n').filter(line => line.trim()).map(line => `   ${line.split('] ')[1] || line}`));
        }
        
        // First check if ports are active - if ports work, system is fine regardless of tmux
        const activePorts = await this.getActivePorts([...getSignalConfig().EXPECTED_PORTS]);
        if (activePorts.length === 0) {
          // Only check tmux if ports are not active
          const { stdout: tmuxCheck } = await execAsync(`tmux has-session -t jtag-test 2>&1 || echo "tmux session not found"`);
          if (tmuxCheck.includes('not found')) {
            errors.push('Tmux session jtag-test not running - try: npm run system:start');
          }
        }
      }
      
      // Don't check port connectivity via curl - lsof is more reliable
      // If ports are listening via lsof check but curl fails, that's a different issue
      
    } catch (error: any) {
      errors.push(`System error check failed: ${error.message}`);
    }
    
    return errors;
  }

  private async checkBrowserReady(): Promise<boolean> {
    try {
      const systemConfig = getSystemConfig();
      const httpUrl = systemConfig.getHTTPBaseUrl();
      
      // Try multiple times with increasing timeouts for browser warmup
      const attempts = [
        { timeout: 3, name: 'quick' },
        { timeout: 8, name: 'patient' },
        { timeout: 15, name: 'generous' }
      ];
      
      for (const attempt of attempts) {
        try {
          const { stdout } = await execAsync(`curl -s --max-time ${attempt.timeout} ${httpUrl} 2>/dev/null | grep -i "jtag" || echo ""`);
          if (stdout.includes('JTAG')) {
            return true;
          }
        } catch {
          // Continue to next attempt
        }
        
        // Small delay between attempts to let browser stabilize
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
      
      return false;
    } catch {
      return false;
    }
  }

  private async checkBuildStatus(): Promise<'success' | 'failed' | 'in-progress' | 'unknown'> {
    try {
      // Check if TypeScript build completed successfully
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -150 ${startupLog} 2>/dev/null | grep -E "(build|tsc|Build|esbuild|npm run|Error|error|Could not resolve)" | tail -15 || echo ""`);
      
      // Only flag as build failure if we have RECENT/FINAL build failures
      // Don't trigger on old log entries or intermediate build steps
      const hasActiveFailure = stdout.includes('npm ERR!') && (stdout.includes('Exit code 1') || stdout.includes('exit 1'));
      const hasRecentResolutionError = stdout.includes('Could not resolve') && stdout.includes('esbuild') && stdout.includes('error');
      
      // Success indicators - look for completion messages
      const hasBuildSuccess = stdout.includes('✅') && (stdout.includes('build') || stdout.includes('Build'));
      const hasCompletedBuild = stdout.includes('build:ts') && !hasActiveFailure;
      const hasCleanCompletion = stdout.includes('npm run') && !hasActiveFailure;
      
      // In-progress indicators
      const isBuilding = stdout.includes('building') || stdout.includes('Building') || stdout.includes('Compiling');
      
      // Only report failure for ACTIVE build failures, not transient states
      if (hasActiveFailure || hasRecentResolutionError) {
        console.log('🚨 Build failure indicators found:');
        if (hasRecentResolutionError) console.log('  - Import/alias resolution failed');
        if (hasActiveFailure) console.log('  - NPM process failed with exit code');
        return 'failed';
      } else if (hasBuildSuccess || hasCompletedBuild || hasCleanCompletion) {
        return 'success';
      } else if (isBuilding) {
        return 'in-progress';
      }
      
      return 'unknown';
    } catch {
      return 'unknown';
    }
  }

  private generateAutonomousGuidance(metrics: Partial<SystemReadySignal>): string[] {
    const guidance: string[] = [];
    
    // System state-specific guidance
    if (!metrics.bootstrapComplete) {
      guidance.push('🚀 WAITING: Browser connecting to system...');
    }
    
    if (!metrics.browserReady) {
      guidance.push('🌐 BROWSER: Starting up - demo page loading...');
    }
    
    if (metrics.buildStatus === 'failed') {
      guidance.push('💻 BUILD: TypeScript compilation failed - check: npm run signal:logs');
    } else if (metrics.buildStatus === 'in-progress') {
      guidance.push('⏳ BUILD: TypeScript compilation in progress...');
    }
    
    if ((metrics.portsActive?.length || 0) < 2) {
      guidance.push('🔌 PORTS: WebSocket/HTTP servers not fully active');
    }
    
    // Autonomous development specific guidance
    guidance.push('🤖 AI TIP: Use ./jtag [command] - it handles all complexity automatically');
    guidance.push('📊 AI TIP: Check npm run agent:quick for instant status');
    
    return guidance;
  }

  private calculateSystemHealth(metrics: Partial<SystemReadySignal>): 'healthy' | 'degraded' | 'unhealthy' | 'error' {
    const requiredPorts = getSignalConfig().EXPECTED_PORTS;
    const activePorts = metrics.portsActive || [];
    const commandCount = metrics.commandCount || 0;
    const bootstrapComplete = metrics.bootstrapComplete || false;
    const browserReady = metrics.browserReady || false;
    const hasErrors = (metrics.errors?.length || 0) > 0 || (metrics.nodeErrors?.length || 0) > 0;
    const compilationFailed = metrics.compilationStatus === 'failed';
    const buildFailed = metrics.buildStatus === 'failed';

    // Error: Critical failures
    if (compilationFailed || buildFailed || (hasErrors && activePorts.length === 0)) {
      return 'error';
    }
    
    // Healthy: All systems fully operational
    if (bootstrapComplete && browserReady && commandCount >= getSignalConfig().MIN_COMMAND_COUNT && activePorts.length >= requiredPorts.length && !hasErrors) {
      return 'healthy';
    }
    
    // Degraded: Core systems working but some issues
    if (bootstrapComplete && activePorts.length > 0 && commandCount > 0) {
      return 'degraded';
    }
    
    // Unhealthy: Critical systems not working
    return 'unhealthy';
  }

  private async writeSignalFile(signal: SystemReadySignal): Promise<void> {
    // Ensure directory exists before atomic write
    const dir = path.dirname(this.readyFile);
    await fs.mkdir(dir, { recursive: true });
    
    const tempFile = `${this.readyFile}.tmp`;
    await fs.writeFile(tempFile, JSON.stringify(signal, null, 2));
    await fs.rename(tempFile, this.readyFile);
  }

  private async writePidFile(): Promise<void> {
    await fs.writeFile(this.pidFile, process.pid.toString());
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Clear signal files (called on system shutdown)
   */
  async clearSignals(): Promise<void> {
    console.log('🧹 Clearing system signals...');
    
    try {
      await fs.unlink(this.readyFile).catch(() => {});
      await fs.unlink(this.pidFile).catch(() => {});
      console.log('✅ Signals cleared');
    } catch (error: any) {
      console.warn('⚠️ Failed to clear signals:', error.message);
    }
  }
}

// CLI interface
async function main() {
  const signaler = new SystemReadySignaler();
  const args = process.argv.slice(2);

  if (args.includes('--check')) {
    // Fast check for autonomous testing - only 5 seconds
    const signal = await signaler.checkSystemReady(5000);
    if (signal) {
      console.log('✅ System is ready');
      console.log(JSON.stringify(signal, null, 2));
      process.exit(0);
    } else {
      console.log('❌ System is not ready');
      process.exit(1);
    }
  } else if (args.includes('--clear')) {
    await signaler.clearSignals();
    process.exit(0);
  } else {
    // Generate ready signal (default)
    await signaler.generateReadySignal();
    process.exit(0);
  }
}

if (require.main === module) {
  main().catch(console.error);
}

export { SystemReadySignaler, type SystemReadySignal };