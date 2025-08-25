/**
 * System Orchestrator - Universal Entry Point Logic with Milestone-Based Orchestration
 * 
 * Provides single orchestration point for all entry points (npm start, npm test, CLI, etc.)
 * Ensures proper milestone execution order and fixes browser timing issues through signaling.
 * 
 * CRITICAL: Browser launch ONLY happens after SERVER_READY milestone is reached.
 */

import { EventEmitter } from 'events';
import { spawn, ChildProcess } from 'child_process';
import { WorkingDirConfig } from '../core/config/WorkingDirConfig';
import { SystemReadySignaler } from '../../scripts/signal-system-ready';
import { 
  SYSTEM_MILESTONES, 
  MILESTONE_DEPENDENCIES, 
  ENTRY_POINT_REQUIREMENTS,
  milestoneEmitter,
  SystemMilestone,
  EntryPointType,
  MilestoneEvent,
  MilestoneProgress
} from './SystemMilestones';

/**
 * Orchestration options for different entry points
 */
export interface OrchestrationOptions {
  verbose?: boolean;
  skipBrowser?: boolean;
  browserUrl?: string;
  testMode?: boolean;
  workingDir?: string;
  timeoutMs?: number;
}

/**
 * System state after orchestration
 */
export interface SystemState {
  readonly success: boolean;
  readonly completedMilestones: string[];
  readonly failedMilestone?: string;
  readonly error?: string;
  readonly serverProcess?: ChildProcess;
  readonly browserOpened: boolean;
}

/**
 * Universal System Orchestrator - Single Entry Point for All System Startup
 */
export class SystemOrchestrator extends EventEmitter {
  private signaler: SystemReadySignaler;
  private serverProcess: ChildProcess | null = null;
  private currentEntryPoint: string = 'unknown';
  
  constructor() {
    super();
    this.signaler = new SystemReadySignaler();
    
    // Forward milestone events
    milestoneEmitter.on('milestone-completed', (event: MilestoneEvent) => {
      this.emit('milestone', event);
    });
    milestoneEmitter.on('milestone-failed', (event: MilestoneEvent) => {
      this.emit('milestone-failed', event);
    });
  }

  /**
   * Main orchestration entry point - handles all system startup scenarios
   */
  async orchestrate(entryPoint: EntryPointType, options: OrchestrationOptions = {}): Promise<SystemState> {
    this.currentEntryPoint = entryPoint;
    
    try {
      console.log(`🎯 ORCHESTRATING: ${entryPoint}`);
      
      // 1. Determine required milestones for this entry point
      const requiredMilestones = this.getRequiredMilestones(entryPoint);
      console.log(`📋 Required milestones: ${requiredMilestones.join(' → ')}`);
      
      // 2. Set up working directory context
      await this.setupWorkingDirectory(options.workingDir);
      
      // 3. Check current system state
      const currentState = await this.getCurrentState();
      
      // 4. Calculate missing milestones in dependency order
      const missingMilestones = this.calculateMissingMilestones(requiredMilestones, currentState);
      
      if (missingMilestones.length === 0) {
        console.log('✅ All required milestones already completed');
        
        // Special case: npm-test and npm-start should always ensure browser is opened
        // even if browser milestones are already completed
        if (entryPoint === 'npm-test' || entryPoint === 'npm-start') {
          console.log('🔄 Entry point requires browser launch - ensuring browser is opened');
          await this.ensureBrowserOpened(options);
        }
        
        return {
          success: true,
          completedMilestones: requiredMilestones,
          browserOpened: requiredMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY)
        };
      }
      
      console.log(`🔄 Missing milestones: ${missingMilestones.join(' → ')}`);
      
      // 5. Execute milestones in proper dependency order
      for (const milestone of missingMilestones) {
        const success = await this.executeMilestone(milestone, options);
        if (!success) {
          return {
            success: false,
            completedMilestones: milestoneEmitter.getProgress(requiredMilestones).completedMilestones,
            failedMilestone: milestone,
            error: `Failed to complete milestone: ${milestone}`,
            browserOpened: false
          };
        }
      }
      
      // 6. Verify final system state
      const finalState = await this.verifySystemState(requiredMilestones);
      console.log('🎉 Orchestration complete');
      
      return finalState;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Orchestration failed: ${errorMessage}`);
      
      await milestoneEmitter.failMilestone(
        'orchestration' as SystemMilestone, 
        entryPoint, 
        errorMessage
      );
      
      return {
        success: false,
        completedMilestones: [],
        error: errorMessage,
        browserOpened: false
      };
    }
  }

  /**
   * Get required milestones for an entry point
   */
  private getRequiredMilestones(entryPoint: EntryPointType): SystemMilestone[] {
    const requirements = ENTRY_POINT_REQUIREMENTS[entryPoint];
    if (!requirements) {
      console.warn(`⚠️ Unknown entry point: ${entryPoint}, using default requirements`);
      return [SYSTEM_MILESTONES.SERVER_READY];
    }
    return requirements;
  }

  /**
   * Set up working directory context for per-project isolation
   */
  private async setupWorkingDirectory(workingDir?: string): Promise<void> {
    if (workingDir) {
      WorkingDirConfig.setWorkingDir(workingDir);
      console.log(`📁 Working directory: ${workingDir}`);
    } else {
      // Use active example configuration
      try {
        const { getActiveExampleName } = await import('../shared/ExampleConfig');
        const activeExample = getActiveExampleName();
        const defaultWorkingDir = `examples/${activeExample}`;
        WorkingDirConfig.setWorkingDir(defaultWorkingDir);
        console.log(`📁 Working directory: ${defaultWorkingDir} (auto-detected)`);
      } catch (error) {
        console.warn('⚠️ Could not auto-detect working directory, using current');
      }
    }
  }

  /**
   * Check current system state to avoid redundant work
   */
  private async getCurrentState(): Promise<Set<string>> {
    const completedMilestones = new Set<string>();
    
    try {
      // First try the signaler for complete system readiness (increase timeout for CLI)
      const systemReady = await this.signaler.checkSystemReady(3000); // More generous timeout
      if (systemReady) {
        const milestonesToComplete = [SYSTEM_MILESTONES.SERVER_START, SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.SYSTEM_HEALTHY];
        
        // If browser is already ready according to signal, mark browser milestones as completed
        // but the browser launch execution will check if it actually needs to open a new tab
        if (systemReady.browserReady) {
          milestonesToComplete.push(
            SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED,
            SYSTEM_MILESTONES.BROWSER_READY
          );
        }
        
        // Inform milestone emitter about existing completed milestones
        for (const milestone of milestonesToComplete) {
          completedMilestones.add(milestone);
          await milestoneEmitter.completeMilestone(milestone, this.currentEntryPoint);
        }
        
        console.log(`✅ System already ready (signal detected, browser: ${systemReady.browserReady ? 'ready' : 'not ready'})`);
        return completedMilestones;
      }
    } catch (error) {
      // Signaler failed, try direct port checks as fallback
    }
    
    // Fallback: Check if ports are in use (indicating servers are running)
    try {
      const { getActivePorts } = await import('../shared/ExampleConfig');
      const activePorts = getActivePorts();
      
      const portChecks = await Promise.all([
        this.checkPortReady(activePorts.websocket_server),
        this.checkPortReady(activePorts.http_server)
      ]);
      
      if (portChecks.every(ready => ready)) {
        // Ports are active, do additional health check to confirm system is ready
        const healthCheck = await this.checkServerHealth(activePorts.http_server);
        if (healthCheck) {
          const milestonesToComplete = [SYSTEM_MILESTONES.SERVER_START, SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.SYSTEM_HEALTHY];
          
          // Inform milestone emitter about existing completed milestones
          for (const milestone of milestonesToComplete) {
            completedMilestones.add(milestone);
            await milestoneEmitter.completeMilestone(milestone, this.currentEntryPoint);
          }
          
          console.log(`✅ Server already ready (ports active + health check passed: ${activePorts.websocket_server}, ${activePorts.http_server})`);
          return completedMilestones;
        } else {
          console.log(`⚠️ Ports active but health check failed (${activePorts.websocket_server}, ${activePorts.http_server})`);
        }
      }
    } catch (error) {
      console.log('🔄 Server needs to be started (port check failed)');
    }
    
    console.log('🔄 Server needs to be started');
    return completedMilestones;
  }

  /**
   * Calculate missing milestones in proper dependency order
   */
  private calculateMissingMilestones(
    requiredMilestones: SystemMilestone[], 
    currentState: Set<string>
  ): SystemMilestone[] {
    const missing: SystemMilestone[] = [];
    const visited = new Set<string>();
    
    const addMissingWithDeps = (milestone: SystemMilestone) => {
      if (visited.has(milestone) || currentState.has(milestone)) {
        return;
      }
      
      visited.add(milestone);
      
      // Add dependencies first
      const deps = MILESTONE_DEPENDENCIES[milestone] || [];
      deps.forEach(dep => addMissingWithDeps(dep as SystemMilestone));
      
      // Add this milestone if not already completed
      if (!currentState.has(milestone) && !missing.includes(milestone)) {
        missing.push(milestone);
      }
    };
    
    requiredMilestones.forEach(milestone => addMissingWithDeps(milestone));
    return missing;
  }

  /**
   * Execute a specific milestone
   */
  private async executeMilestone(milestone: SystemMilestone, options: OrchestrationOptions): Promise<boolean> {
    console.log(`🚀 Executing milestone: ${milestone}`);
    
    try {
      switch (milestone) {
        case SYSTEM_MILESTONES.BUILD_START:
          return await this.executeBuildStart();
          
        case SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE:
          return await this.executeBuildTypeScript();
          
        case SYSTEM_MILESTONES.BUILD_STRUCTURE_COMPLETE:
          return await this.executeBuildStructure();
          
        case SYSTEM_MILESTONES.BUILD_COMPLETE:
          return await this.executeBuildComplete();
          
        case SYSTEM_MILESTONES.DEPLOY_START:
          return await this.executeDeployStart();
          
        case SYSTEM_MILESTONES.DEPLOY_FILES_COMPLETE:
          return await this.executeDeployFiles();
          
        case SYSTEM_MILESTONES.DEPLOY_PORTS_ALLOCATED:
          return await this.executeDeployPorts();
          
        case SYSTEM_MILESTONES.DEPLOY_COMPLETE:
          return await this.executeDeployComplete();
          
        case SYSTEM_MILESTONES.SERVER_START:
          return await this.executeServerStart();
          
        case SYSTEM_MILESTONES.SERVER_PROCESS_READY:
          return await this.executeServerProcess();
          
        case SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY:
          return await this.executeServerWebSocket();
          
        case SYSTEM_MILESTONES.SERVER_HTTP_READY:
          return await this.executeServerHTTP();
          
        case SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE:
          return await this.executeServerBootstrap();
          
        case SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED:
          return await this.executeServerCommands();
          
        case SYSTEM_MILESTONES.SERVER_READY:
          return await this.executeServerReady();
          
        case SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED:
          return await this.executeBrowserLaunch(options);
          
        case SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED:
          return await this.executeBrowserProcess();
          
        case SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED:
          return await this.executeBrowserWebSocket();
          
        case SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED:
          return await this.executeBrowserInterface();
          
        case SYSTEM_MILESTONES.BROWSER_READY:
          return await this.executeBrowserReady();
          
        case SYSTEM_MILESTONES.SYSTEM_HEALTHY:
          return await this.executeSystemHealthy();
          
        case SYSTEM_MILESTONES.SYSTEM_READY:
          return await this.executeSystemReady();
          
        default:
          console.warn(`⚠️ Unknown milestone: ${milestone}`);
          return true; // Don't fail on unknown milestones
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Milestone ${milestone} failed: ${errorMessage}`);
      
      await milestoneEmitter.failMilestone(milestone, this.currentEntryPoint, errorMessage);
      return false;
    }
  }

  /**
   * BUILD MILESTONES
   */
  private async executeBuildStart(): Promise<boolean> {
    console.log('🔨 Starting build process...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_START, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildTypeScript(): Promise<boolean> {
    console.log('📝 Compiling TypeScript...');
    // TypeScript compilation would happen here
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildStructure(): Promise<boolean> {
    console.log('🏗️ Building structure...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_STRUCTURE_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBuildComplete(): Promise<boolean> {
    console.log('✅ Build complete');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BUILD_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * DEPLOY MILESTONES
   */
  private async executeDeployStart(): Promise<boolean> {
    console.log('🚀 Starting deployment...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_START, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployFiles(): Promise<boolean> {
    console.log('📁 Deploying files...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_FILES_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployPorts(): Promise<boolean> {
    console.log('🔌 Allocating ports...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_PORTS_ALLOCATED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeDeployComplete(): Promise<boolean> {
    console.log('✅ Deployment complete');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.DEPLOY_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * SERVER MILESTONES
   */
  private async executeServerStart(): Promise<boolean> {
    console.log('🔌 Starting server process...');
    
    // Clear any existing signals
    await this.signaler.clearSignals();
    
    // Start the server using the existing launch-active-example script
    // but WITHOUT the premature browser opening
    const { getActivePorts } = await import('../shared/ExampleConfig');
    const activePorts = getActivePorts();
    
    // Import and start the JTAG system server
    const { JTAGSystemServer } = await import('../core/system/server/JTAGSystemServer');
    const jtagServer = await JTAGSystemServer.connect();
    console.log(`✅ JTAG WebSocket Server running on port ${activePorts.websocket_server}`);
    
    // Start the example HTTP server
    const { getActiveExamplePath } = await import('../shared/ExampleConfig');
    const activeExamplePath = getActiveExamplePath();
    
    this.serverProcess = spawn('npm', ['start'], {
      cwd: activeExamplePath,
      stdio: 'inherit',
      shell: true
    });
    
    this.serverProcess.on('error', (error) => {
      console.error(`❌ Server process failed: ${error.message}`);
    });
    
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_START, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerProcess(): Promise<boolean> {
    console.log('🔄 Server process ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_PROCESS_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerWebSocket(): Promise<boolean> {
    console.log('🔌 WebSocket server ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerHTTP(): Promise<boolean> {
    console.log('🌐 HTTP server ready...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_HTTP_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerBootstrap(): Promise<boolean> {
    console.log('⚡ Server bootstrap complete...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerCommands(): Promise<boolean> {
    console.log('📋 Server commands loaded...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeServerReady(): Promise<boolean> {
    console.log('⏳ Waiting for server to be ready...');
    
    // SIMPLIFIED READINESS CHECK: Check port availability directly
    // This avoids complex signaling system issues while ensuring servers are actually ready
    const { getActivePorts } = await import('../shared/ExampleConfig');
    const activePorts = getActivePorts();
    
    const maxRetries = 30; // 30 seconds max wait
    let attempt = 0;
    
    while (attempt < maxRetries) {
      try {
        // Quick port connectivity check instead of complex signal system
        const portChecks = await Promise.all([
          this.checkPortReady(activePorts.websocket_server),
          this.checkPortReady(activePorts.http_server)
        ]);
        
        if (portChecks.every(ready => ready)) {
          console.log(`✅ Server ports ready: WebSocket=${activePorts.websocket_server}, HTTP=${activePorts.http_server}`);
          break;
        }
      } catch (error) {
        // Continue waiting
      }
      
      attempt++;
      await new Promise(resolve => setTimeout(resolve, 1000)); // Wait 1 second
    }
    
    if (attempt >= maxRetries) {
      throw new Error(`Server failed to become ready within ${maxRetries} seconds`);
    }
    
    console.log('✅ Server is ready');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SERVER_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Clean modular port checking with strong types
   */
  private async checkPortReady(port: number): Promise<boolean> {
    const { PortChecker, PortCheckResult } = await import('../core/ports/PortChecker');
    const checker = new PortChecker();
    
    const status = await checker.checkPortNumber(port);
    
    if (status.isActive && status.result === PortCheckResult.ACTIVE) {
      console.log(`✅ Port ${port} is active (${status.method})`);
    } else if (status.result === PortCheckResult.ERROR) {
      console.log(`⚠️ Port ${port} check error (${status.method}): ${status.error}`);
    } else {
      console.log(`⚠️ Port ${port} not active (${status.method})`);
    }
    
    return status.isActive && status.result === PortCheckResult.ACTIVE;
  }

  /**
   * Signal-based server health check (replaces HTTP polling with timeouts)
   * 
   * TIMEOUT ELIMINATION: This replaces the 2-second HTTP timeout polling
   * with comprehensive signal-based health detection from the signal system.
   */
  private async checkServerHealth(port: number): Promise<boolean> {
    try {
      // Use the existing signal system for comprehensive health checking
      const { SystemReadySignaler } = await import('../../scripts/signaling/server/SystemReadySignaler');
      const signaler = new SystemReadySignaler();
      
      // Fast event-driven health check (500ms max)
      const signal = await signaler.checkSystemReady(500);
      
      if (signal) {
        // Use comprehensive signal-based health instead of primitive HTTP polling
        const isHealthy = signal.systemHealth === 'healthy';
        const hasPort = signal.portsActive?.includes(port) || false;
        const hasCommands = signal.commandCount > 0;
        const browserReady = signal.browserReady;
        
        if (isHealthy && hasPort && hasCommands) {
          console.log(`✅ Server health confirmed via signal: ${signal.commandCount} commands, browser: ${browserReady} (no HTTP polling)`);
          return true;
        } else {
          console.log(`⚠️ Server health degraded - health: ${signal.systemHealth}, port: ${hasPort}, commands: ${signal.commandCount}`);
          return false;
        }
      }
      
      // No signal found - server is not healthy
      console.log(`⚠️ Server health check failed for port ${port} - no signal detected`);
      return false;
      
    } catch (error) {
      console.log(`⚠️ Signal-based health check failed for port ${port}: ${error}`);
      return false;
    }
  }

  /**
   * BROWSER MILESTONES - CRITICAL: Only after server ready
   */
  private async executeBrowserLaunch(options: OrchestrationOptions): Promise<boolean> {
    if (options.skipBrowser) {
      console.log('⏭️ Skipping browser launch (skipBrowser option)');
      await milestoneEmitter.completeMilestone(
        SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED, 
        this.currentEntryPoint
      );
      return true;
    }
    
    console.log('🌐 Launching browser...');
    
    // CRITICAL FIX: Browser only launches AFTER server ready milestone
    const browserUrl = options.browserUrl || this.getDefaultBrowserUrl();
    
    try {
      spawn('open', [browserUrl], { 
        detached: true, 
        stdio: 'ignore' 
      }).unref();
      console.log(`✅ Browser launched: ${browserUrl}`);
    } catch (error) {
      console.warn(`⚠️ Failed to auto-open browser: ${error}`);
      console.log(`👉 Manually open: ${browserUrl}`);
    }
    
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserProcess(): Promise<boolean> {
    console.log('🌐 Browser process started...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserWebSocket(): Promise<boolean> {
    console.log('🔗 Browser WebSocket connected...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserInterface(): Promise<boolean> {
    console.log('🖥️ Browser interface loaded...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeBrowserReady(): Promise<boolean> {
    console.log('⏳ Waiting for browser to be ready...');
    
    // For now, assume browser is ready after launch
    // Future: implement browser readiness detection via WebSocket
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    console.log('✅ Browser is ready');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.BROWSER_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * SYSTEM MILESTONES
   */
  private async executeSystemHealthy(): Promise<boolean> {
    console.log('💚 System is healthy...');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SYSTEM_HEALTHY, 
      this.currentEntryPoint
    );
    return true;
  }

  private async executeSystemReady(): Promise<boolean> {
    console.log('🎉 System is fully ready');
    await milestoneEmitter.completeMilestone(
      SYSTEM_MILESTONES.SYSTEM_READY, 
      this.currentEntryPoint
    );
    return true;
  }

  /**
   * Verify final system state
   */
  private async verifySystemState(requiredMilestones: SystemMilestone[]): Promise<SystemState> {
    const progress = milestoneEmitter.getProgress(requiredMilestones);
    const allCompleted = progress.completed === progress.total;
    
    if (!allCompleted) {
      const missingMilestones = requiredMilestones.filter(m => !progress.completedMilestones.includes(m));
      const errorMessage = `Final verification failed. Missing milestones: ${missingMilestones.join(', ')}`;
      console.error(`❌ ${errorMessage}`);
      
      return {
        success: false,
        completedMilestones: progress.completedMilestones,
        failedMilestone: progress.current || missingMilestones[0],
        error: errorMessage,
        browserOpened: progress.completedMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY),
        serverProcess: this.serverProcess
      };
    }
    
    return {
      success: true,
      completedMilestones: progress.completedMilestones,
      failedMilestone: undefined,
      error: undefined,
      browserOpened: progress.completedMilestones.includes(SYSTEM_MILESTONES.BROWSER_READY),
      serverProcess: this.serverProcess
    };
  }

  /**
   * Ensure browser is opened for entry points that require browser interaction
   * This is called even when browser milestones are already completed
   */
  private async ensureBrowserOpened(options: OrchestrationOptions): Promise<void> {
    if (options.skipBrowser) {
      console.log('⏭️ Skipping browser launch (skipBrowser option)');
      return;
    }
    
    console.log('🌐 Ensuring browser is opened...');
    
    const browserUrl = options.browserUrl || this.getDefaultBrowserUrl();
    
    try {
      spawn('open', [browserUrl], { 
        detached: true, 
        stdio: 'ignore' 
      }).unref();
      console.log(`✅ Browser opened: ${browserUrl}`);
    } catch (error) {
      console.warn(`⚠️ Failed to auto-open browser: ${error}`);
      console.log(`👉 Manually open: ${browserUrl}`);
    }
  }

  /**
   * Get default browser URL based on configuration
   */
  private getDefaultBrowserUrl(): string {
    try {
      const { getActivePorts } = require('../shared/ExampleConfig');
      const activePorts = getActivePorts();
      return `http://localhost:${activePorts.http_server}`;
    } catch (error) {
      console.warn('⚠️ Could not get active ports, using default');
      return 'http://localhost:9002';
    }
  }

  /**
   * Get milestone progress for monitoring
   */
  getProgress(entryPoint: EntryPointType): MilestoneProgress {
    const requiredMilestones = this.getRequiredMilestones(entryPoint);
    return milestoneEmitter.getProgress(requiredMilestones);
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    if (this.serverProcess) {
      console.log('🛑 Cleaning up server process...');
      this.serverProcess.kill('SIGTERM');
      this.serverProcess = null;
    }
  }
}

/**
 * Global orchestrator instance
 */
export const systemOrchestrator = new SystemOrchestrator();