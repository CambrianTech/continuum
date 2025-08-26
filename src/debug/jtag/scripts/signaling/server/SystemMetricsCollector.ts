/**
 * System Metrics Collector (Server)
 * 
 * Server-side implementation for collecting system readiness metrics.
 * Handles file system operations, process detection, and health checks.
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import { exec } from 'child_process';
import { promisify } from 'util';
import { SystemReadySignal, SystemHealth, BuildStatus } from '../shared/SystemSignalingTypes';
import { getSignalConfig } from '../shared/MilestoneConfiguration';
import { JTAG_LOG_PATTERNS } from '../../../system/core/client/shared/JTAGClientConstants';
import { getActivePorts } from '../../../system/shared/ExampleConfig';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';

const execAsync = promisify(exec);

export class SystemMetricsCollector {
  private lastErrorKey?: string;

  async collectSystemMetrics(): Promise<SystemReadySignal> {
    const metrics: Partial<SystemReadySignal> = {
      timestamp: new Date().toISOString(),
      readySignalVersion: getSignalConfig().VERSION,
      errors: [],
      nodeErrors: [],
      startupLogs: 'No startup logs found\n',
      bootstrapComplete: false,
      commandCount: 0,
      portsActive: [],
      browserReady: false,
      buildStatus: 'unknown',
      autonomousGuidance: [],
      compilationStatus: 'unknown',
      systemHealth: 'unhealthy',
      generatorPid: process.pid,       // Mark who generated this signal
      consumerPids: []                 // Empty array for tracking consumers
    };

    try {
      // Capture node-level errors first
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
      // BOOTSTRAP COMPLETION REQUIRES ALL CRITICAL SYSTEMS READY:
      // 1. WebSocket server (9001) - checked via bootstrap logs
      // 2. HTTP server (9002) - checked via actual HTTP request
      // 3. System services running - checked via process registry
      // 4. Session management active - checked via session metadata
      
      console.log('🔍 Bootstrap milestone check: Starting comprehensive readiness verification...');
      
      // Step 1: Check WebSocket server bootstrap completion via logs
      const webSocketReady = await this.checkWebSocketBootstrap();
      if (!webSocketReady) {
        console.log('❌ Bootstrap milestone: WebSocket server not ready yet');
        return false;
      }
      console.log('✅ Bootstrap milestone: WebSocket server ready');
      
      // Step 2: Check HTTP server readiness via actual request
      const httpReady = await this.checkHTTPServerReady();
      if (!httpReady) {
        console.log('❌ Bootstrap milestone: HTTP server not ready yet');
        return false;
      }
      console.log('✅ Bootstrap milestone: HTTP server ready');
      
      // Step 3: Verify system services are running
      const servicesReady = await this.checkSystemServices();
      if (!servicesReady) {
        console.log('❌ Bootstrap milestone: System services not ready yet');
        return false;
      }
      console.log('✅ Bootstrap milestone: System services ready');
      
      // Step 4: Verify session management is active
      const sessionReady = await this.checkSessionManagement();
      if (!sessionReady) {
        console.log('❌ Bootstrap milestone: Session management not ready yet');
        return false;
      }
      console.log('✅ Bootstrap milestone: Session management ready');
      
      console.log('🎉 Bootstrap milestones: ALL SYSTEMS READY - Bootstrap completion confirmed!');
      return true;
    } catch (error) {
      console.log(`💥 Bootstrap milestone check failed: ${error}`);
      return false;
    }
  }

  private async checkWebSocketBootstrap(): Promise<boolean> {
    try {
      // Use WorkingDirConfig for per-project isolation
      const continuumPath = WorkingDirConfig.getContinuumPath();
      
      // Self-healing: Try multiple log locations in order of preference
      const logPatterns = [
        // 1. Try currentUser logs first (symlinked to active user session)
        path.join(continuumPath, 'jtag', 'currentUser', 'logs', 'browser-console-log.log'),
        // 2. Fallback to system session logs (use shell glob expansion)
        `"${continuumPath}/jtag/sessions/system/*/logs/browser-console-log.log"`,
        // 3. Fallback to any user session logs (use shell glob expansion)
        `"${continuumPath}/jtag/sessions/user/*/logs/browser-console-log.log"`
      ];
      
      for (const logPattern of logPatterns) {
        try {
          // Use shell expansion for glob patterns
          const { stdout } = await execAsync(`grep -F "Bootstrap complete! Discovered" ${logPattern} 2>/dev/null | grep "commands" || echo ""`);
          if (stdout.trim().length > 0) {
            console.log(`✅ Bootstrap check: Found WebSocket bootstrap in ${logPattern}`);
            return true;
          }
        } catch {
          // Try next location
        }
      }
      
      console.log('🔍 Bootstrap check: WebSocket bootstrap logs not found');
      return false;
    } catch {
      return false;
    }
  }

  private async checkHTTPServerReady(): Promise<boolean> {
    try {
      const activePorts = await getActivePorts();
      const httpUrl = `http://localhost:${activePorts.http_server}`;
      
      // Use native HTTP client for fast readiness check
      const http = await import('http');
      
      return new Promise<boolean>((resolve) => {
        const req = http.request(httpUrl, {
          method: 'GET',
          timeout: 2000, // Slightly longer timeout for bootstrap check
          headers: {
            'User-Agent': 'JTAG-BootstrapChecker/1.0'
          }
        }, (res) => {
          // Any successful response means HTTP server is ready
          resolve(res.statusCode !== undefined && res.statusCode < 500);
        });
        
        req.on('error', () => {
          resolve(false);
        });
        
        req.on('timeout', () => {
          req.destroy();
          resolve(false);
        });
        
        req.end();
      });
      
    } catch (error) {
      console.log(`⚠️ HTTP server readiness check failed: ${error}`);
      return false;
    }
  }

  private async checkSystemServices(): Promise<boolean> {
    try {
      // NO STATE NEEDED: Just look at what processes are actually running
      const wsPortCheck = await execAsync(`lsof -ti:9001 2>/dev/null | head -1 || echo ""`);
      const httpPortCheck = await execAsync(`lsof -ti:9002 2>/dev/null | head -1 || echo ""`);
      
      const wsProcessRunning = wsPortCheck.stdout.trim().length > 0;
      const httpProcessRunning = httpPortCheck.stdout.trim().length > 0;
      
      if (!wsProcessRunning) {
        console.log('⚠️ System services check: WebSocket server not running on port 9001');
        return false;
      }
      
      if (!httpProcessRunning) {
        console.log('⚠️ System services check: HTTP server not running on port 9002');
        return false;
      }
      
      console.log('✅ System services check: Both servers running on expected ports');
      return true;
      
    } catch (error) {
      console.log(`⚠️ System services readiness check failed: ${error}`);
      return false;
    }
  }

  private async checkSessionManagement(): Promise<boolean> {
    try {
      // NO STATE NEEDED: Just look at what session files actually exist
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const sessionMetadataPath = path.join(continuumPath, 'jtag', 'sessions', 'metadata.json');
      
      // Verify session metadata exists and is valid
      const { stdout } = await execAsync(`test -f "${sessionMetadataPath}" && cat "${sessionMetadataPath}" | grep -q "sessions" && echo "SESSION_OK" || echo "SESSION_MISSING"`);
      
      if (stdout.trim() !== 'SESSION_OK') {
        console.log('⚠️ Session management check: Session metadata not found or invalid');
        return false;
      }
      
      console.log('✅ Session management check: Session metadata exists and valid');
      return true;
      
    } catch (error) {
      console.log(`⚠️ Session management readiness check failed: ${error}`);
      return false;
    }
  }

  private async countCommands(): Promise<number> {
    try {
      // Use WorkingDirConfig for per-project isolation
      const continuumPath = WorkingDirConfig.getContinuumPath();
      
      // Self-healing: Try multiple log locations in order of preference  
      const logPatterns = [
        // 1. Try currentUser logs first (symlinked to active user session)
        path.join(continuumPath, 'jtag', 'currentUser', 'logs', 'browser-console-log.log'),
        // 2. Fallback to system session logs (use shell glob expansion)
        `"${continuumPath}/jtag/sessions/system/*/logs/browser-console-log.log"`,
        // 3. Fallback to any user session logs (use shell glob expansion)
        `"${continuumPath}/jtag/sessions/user/*/logs/browser-console-log.log"`
      ];
      
      for (const logPattern of logPatterns) {
        try {
          // Get all bootstrap messages and take the LAST one (most recent)
          const { stdout } = await execAsync(`grep -F "Bootstrap complete! Discovered" ${logPattern} 2>/dev/null | grep "commands" || echo ""`);
          
          if (stdout.trim()) {
            // Split lines and get the last match
            const lines = stdout.trim().split('\n');
            const lastLine = lines[lines.length - 1];
            const match = lastLine.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
            const count = match ? parseInt(match[1]) : 0;
            if (count > 0) {
              return count;
            }
          }
        } catch {
          // Try next location
        }
      }
      
      return 0;
    } catch {
      return 0;
    }
  }

  private async getActivePorts(expectedPorts: number[]): Promise<number[]> {
    try {
      const { stdout } = await execAsync(`lsof -i -P -n | grep LISTEN | grep -E ":${expectedPorts.join('|:')}" | awk '{print $9}' | grep -o ':[0-9]*' | grep -o '[0-9]*' | sort -u || echo ""`);
      return stdout.trim() ? stdout.trim().split('\n').map(Number).filter(port => !isNaN(port)) : [];
    } catch {
      return [];
    }
  }

  private async checkBrowserReady(): Promise<boolean> {
    try {
      // TIMEOUT ELIMINATION: Replace cascading curl timeouts with native HTTP client
      // This eliminates the progressive timeout pattern (1s, 2s, 4s, 6s) and external process overhead
      
      const activePorts = await getActivePorts();
      const httpUrl = `http://localhost:${activePorts.http_server}`;
      
      // Use native HTTP client instead of external curl command
      const http = await import('http');
      
      return new Promise<boolean>((resolve) => {
        // Single fast request instead of multiple cascading timeouts
        const req = http.request(httpUrl, {
          method: 'GET',
          timeout: 1000, // Single 1-second timeout instead of cascading 1s+2s+4s+6s
          headers: {
            'User-Agent': 'JTAG-SystemMetricsCollector/1.0'
          }
        }, (res) => {
          let data = '';
          res.on('data', (chunk) => {
            data += chunk;
          });
          
          res.on('end', () => {
            // Check for Continuum in response (same logic, but no external process)
            const isReady = data.toLowerCase().includes('continuum');
            if (isReady) {
              console.log('✅ Browser readiness confirmed via native HTTP (no curl cascading timeouts)');
            }
            resolve(isReady);
          });
        });
        
        req.on('error', () => {
          resolve(false);
        });
        
        req.on('timeout', () => {
          req.destroy();
          resolve(false);
        });
        
        req.end();
      });
      
    } catch (error) {
      console.log(`⚠️ Native HTTP browser check failed: ${error}`);
      return false;
    }
  }

  private async checkBuildStatus(): Promise<BuildStatus> {
    try {
      // Check if TypeScript build completed successfully
      const startupLog = '.continuum/jtag/system/logs/npm-start.log';
      const { stdout } = await execAsync(`tail -150 ${startupLog} 2>/dev/null | grep -E "(build|tsc|Build|esbuild|npm run|Error|error|Could not resolve)" | tail -15 || echo ""`);
      
      // Only flag as build failure if we have RECENT/FINAL build failures
      // Don't trigger on old log entries or intermediate build steps
      const hasActiveFailure = stdout.includes('npm ERR!') && (stdout.includes('Exit code 1') || stdout.includes('exit 1'));
      const hasRecentResolutionError = stdout.includes('Could not resolve') && stdout.includes('esbuild') && stdout.includes('error');
      
      if (hasActiveFailure || hasRecentResolutionError) {
        return 'failed';
      }
      
      // Success indicators - look for completion messages
      const hasBuildSuccess = stdout.includes('✅') && (stdout.includes('build') || stdout.includes('Build'));
      const hasCompletedBuild = stdout.includes('build:ts') && !hasActiveFailure;
      const hasCleanCompletion = stdout.includes('npm run') && !hasActiveFailure;
      
      if (hasBuildSuccess || hasCompletedBuild || hasCleanCompletion) {
        return 'success';
      }
      
      // In-progress indicators
      const isBuilding = stdout.includes('building') || stdout.includes('Building') || stdout.includes('Compiling');
      if (isBuilding) {
        return 'in-progress';
      }
      
      return 'unknown';
    } catch {
      return 'unknown';
    }
  }

  private async captureStartupLogs(): Promise<string> {
    try {
      const logFile = path.join('.continuum', 'jtag', 'system', 'logs', 'npm-start.log');
      const { stdout } = await execAsync(`tail -50 ${logFile} 2>/dev/null | grep -v "^$" | tail -20 || echo "No startup logs found"`);
      return stdout || 'No startup logs found\n';
    } catch {
      return 'No startup logs found\n';
    }
  }

  private async captureNodeErrors(): Promise<string[]> {
    try {
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const errorLog = path.join(continuumPath, 'jtag', 'system', 'logs', 'server-console-error.log');
      const { stdout } = await execAsync(`tail -10 ${errorLog} 2>/dev/null | grep -E "(Error|error|Failed|failed)" | tail -3 || echo ""`);
      return stdout.trim() ? stdout.split('\n').filter(line => line.trim()) : [];
    } catch {
      return [];
    }
  }

  private async collectSystemErrors(bootstrapComplete: boolean, commandCount: number): Promise<string[]> {
    const errors: string[] = [];
    
    try {
      // CRITICAL: Check server console errors for daemon creation failures
      if (!bootstrapComplete && commandCount === 0) {
        // Check server console error logs for daemon creation failures
        const continuumPath = WorkingDirConfig.getContinuumPath();
        const serverErrorLog = path.join(continuumPath, 'jtag', 'system', 'logs', 'server-console-error.log');
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
    } catch (error: any) {
      errors.push(`System error check failed: ${error.message}`);
    }
    
    return errors;
  }

  private generateAutonomousGuidance(metrics: Partial<SystemReadySignal>): string[] {
    const guidance: string[] = [];
    
    // Core guidance always available
    guidance.push('🤖 AI TIP: Use ./jtag [command] - it handles all complexity automatically');
    guidance.push('📊 AI TIP: Check npm run agent:quick for instant status');
    
    // Context-sensitive guidance based on current state
    if (!metrics.bootstrapComplete && metrics.commandCount === 0) {
      guidance.unshift('🔧 System starting up - bootstrap in progress...');
    } else if (metrics.errors && metrics.errors.length > 0) {
      guidance.unshift('⚠️ Check logs: npm run signal:logs | Restart: npm run system:restart');
    }
    
    return guidance;
  }

  private calculateSystemHealth(metrics: Partial<SystemReadySignal>): SystemHealth {
    const hasBootstrap = metrics.bootstrapComplete || false;
    const hasCommands = (metrics.commandCount || 0) > 0;
    const hasRequiredPorts = (metrics.portsActive?.length || 0) >= 2;
    const hasBrowser = metrics.browserReady || false;
    const hasErrors = (metrics.errors?.length || 0) > 0;

    if (hasErrors) {
      return 'error';
    }

    if (hasBootstrap && hasCommands && hasRequiredPorts && hasBrowser) {
      return 'healthy';
    }

    if (hasBootstrap && hasCommands && hasRequiredPorts) {
      return 'degraded'; // Functional but browser not ready
    }

    return 'unhealthy';
  }
}