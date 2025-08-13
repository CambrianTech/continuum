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

const SIGNAL_CONFIG = {
  VERSION: '1.0.0',
  EXPECTED_PORTS: [9001, 9002] as readonly number[],
  MIN_COMMAND_COUNT: 15,
  DEFAULT_TIMEOUT_MS: 60000
} as const;

class SystemReadySignaler {
  private signalDir = '.continuum/jtag/signals';
  private readyFile = path.join(this.signalDir, 'system-ready.json');
  private pidFile = path.join(this.signalDir, 'system.pid');

  async generateReadySignal(): Promise<SystemReadySignal> {
    console.log('🚦 Generating system ready signal...');

    try {
      // Ensure signal directory exists
      await fs.mkdir(this.signalDir, { recursive: true });

      // Gather system readiness metrics
      const signal = await this.collectSystemMetrics();
      
      // Write signal file atomically
      await this.writeSignalFile(signal);
      
      // Write PID file for system tracking
      await this.writePidFile();
      
      console.log(`✅ System ready signal generated: ${signal.systemHealth}`);
      console.log(`📊 Bootstrap complete: ${signal.bootstrapComplete}`);
      console.log(`🔧 Commands discovered: ${signal.commandCount}`);
      console.log(`🌐 Active ports: ${signal.portsActive.join(', ')}`);
      
      return signal;
      
    } catch (error: any) {
      console.error('❌ Failed to generate ready signal:', error.message);
      throw error; // Don't exit, let caller handle
    }
  }

  async checkSystemReady(timeoutMs = 10000): Promise<SystemReadySignal | null> {
    console.log('🔍 Checking system readiness...');
    
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
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
        
        if ((signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded') && signal.bootstrapComplete) {
          console.log('✅ System ready signal confirmed');
          return signal;
        } else {
          console.log(`⚠️ System not fully ready: ${signal.systemHealth}, bootstrap: ${signal.bootstrapComplete}`);
        }
        
      } catch (error) {
        // Signal file doesn't exist yet, keep waiting
      }
      
      console.log('⏳ Waiting for system ready signal...');
      await this.sleep(2000);
    }
    
    console.log('❌ Timeout waiting for system ready signal');
    return null;
  }

  private async collectSystemMetrics(): Promise<SystemReadySignal> {
    const metrics: {
      -readonly [K in keyof SystemReadySignal]?: SystemReadySignal[K]
    } = {
      timestamp: new Date().toISOString(),
      readySignalVersion: SIGNAL_CONFIG.VERSION,
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
      metrics.portsActive = await this.getActivePorts([...SIGNAL_CONFIG.EXPECTED_PORTS]);
      
      // Check browser readiness (can we reach the demo page?)
      metrics.browserReady = await this.checkBrowserReady();
      
      // Check build status
      metrics.buildStatus = await this.checkBuildStatus();
      
      // Collect any errors found
      metrics.errors = await this.collectSystemErrors();
      
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
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep -F "Bootstrap complete! Discovered" | grep "commands" 2>/dev/null || echo ""`);
      return stdout.trim().length > 0;
    } catch {
      return false;
    }
  }

  private async countCommands(): Promise<number> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      // Get all bootstrap messages and take the LAST one (most recent)
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep -F "Bootstrap complete! Discovered" | grep "commands" 2>/dev/null || echo ""`);
      
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
      const { stdout } = await execAsync(`tail -50 ${startupLog} 2>/dev/null | grep -E "(tsc|build:ts)" | tail -10 || echo ""`);
      
      if (stdout.includes('error') || stdout.includes('Error')) {
        return 'failed';
      } else if (stdout.includes('build:ts') && !stdout.includes('error')) {
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

  private async collectSystemErrors(): Promise<string[]> {
    const errors: string[] = [];
    
    try {
      // Check for tmux session errors
      const { stdout: tmuxCheck } = await execAsync(`tmux has-session -t jtag-test 2>&1 || echo "tmux session not found"`);
      if (tmuxCheck.includes('not found')) {
        errors.push('Tmux session jtag-test not running');
      }
      
      // Check for port conflicts
      const requiredPorts = SIGNAL_CONFIG.EXPECTED_PORTS;
      for (const port of requiredPorts) {
        try {
          await execAsync(`curl -s --connect-timeout 2 http://localhost:${port} >/dev/null`);
        } catch {
          errors.push(`Port ${port} not responding`);
        }
      }
      
    } catch (error: any) {
      errors.push(`System error check failed: ${error.message}`);
    }
    
    return errors;
  }

  private async checkBrowserReady(): Promise<boolean> {
    try {
      const { stdout } = await execAsync(`curl -s --max-time 3 http://localhost:9002 2>/dev/null | grep -i "jtag" || echo ""`);
      return stdout.includes('JTAG');
    } catch {
      return false;
    }
  }

  private async checkBuildStatus(): Promise<'success' | 'failed' | 'in-progress' | 'unknown'> {
    try {
      // Check if TypeScript build completed successfully
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -100 ${startupLog} 2>/dev/null | grep -E "(build|tsc|Build)" | tail -10 || echo ""`);
      
      if (stdout.includes('error') || stdout.includes('Error')) {
        return 'failed';
      } else if (stdout.includes('✅') && (stdout.includes('build') || stdout.includes('Build'))) {
        return 'success';
      } else if (stdout.includes('building') || stdout.includes('Building')) {
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
      guidance.push('🌐 BROWSER: Demo page not responding - check browser launch');
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
    const requiredPorts = SIGNAL_CONFIG.EXPECTED_PORTS;
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
    if (bootstrapComplete && browserReady && commandCount >= SIGNAL_CONFIG.MIN_COMMAND_COUNT && activePorts.length >= requiredPorts.length && !hasErrors) {
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