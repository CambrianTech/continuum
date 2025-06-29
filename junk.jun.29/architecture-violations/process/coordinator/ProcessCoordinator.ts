/**
 * Process Coordinator Implementation
 * Coordinates multiple daemon processes using Node.js child_process
 */

import { IProcessCoordinator, ProcessMessage, ProcessResult, ProcessHealth, ProcessConfig } from '../interfaces/IProcessCoordinator.js';
import { ProcessRegistry } from '../registry/ProcessRegistry.js';
import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

interface ManagedProcess {
  processId: string;
  daemonType: string;
  childProcess: ChildProcess;
  config: ProcessConfig;
  startTime: Date;
  lastHeartbeat: Date;
  messageQueue: ProcessMessage[];
  pendingMessages: Map<string, { resolve: Function; reject: Function; timeout: NodeJS.Timeout }>;
}

export class ProcessCoordinator extends EventEmitter implements IProcessCoordinator {
  private registry: ProcessRegistry;
  private processes = new Map<string, ManagedProcess>();
  private isRunning = false;
  private healthCheckInterval?: NodeJS.Timeout;

  constructor(searchPaths?: string[]) {
    super();
    this.registry = new ProcessRegistry(searchPaths);
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('ProcessCoordinator is already running');
    }

    this.log('🚀 Starting Process Coordinator...');
    
    // Discover available daemon processes
    await this.registry.discoverProcesses('src/daemons');
    
    const available = this.registry.getAvailable();
    this.log(`📦 Discovered ${available.size} daemon types`);

    // Start health monitoring
    this.startHealthMonitoring();
    
    this.isRunning = true;
    this.log('✅ Process Coordinator started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.log('🛑 Stopping Process Coordinator...');
    
    // Stop health monitoring
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }

    // Gracefully stop all processes
    const stopPromises = Array.from(this.processes.keys()).map(processId => 
      this.kill(processId)
    );
    
    await Promise.allSettled(stopPromises);
    
    this.isRunning = false;
    this.log('✅ Process Coordinator stopped');
  }

  // Lifecycle management
  async spawn(daemonType: string): Promise<string> {
    const config = this.registry.getConfig(daemonType);
    if (!config) {
      throw new Error(`Unknown daemon type: ${daemonType}`);
    }

    const processId = this.generateProcessId(daemonType);
    
    this.log(`🚀 Spawning ${daemonType} process: ${processId}`);
    
    try {
      // Spawn the daemon process
      const childProcess = spawn('npx', ['tsx', config.entryPoint], {
        stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
        cwd: process.cwd(),
        env: { ...process.env, DAEMON_PROCESS_ID: processId }
      });

      const managedProcess: ManagedProcess = {
        processId,
        daemonType,
        childProcess,
        config,
        startTime: new Date(),
        lastHeartbeat: new Date(),
        messageQueue: [],
        pendingMessages: new Map()
      };

      this.processes.set(processId, managedProcess);
      
      // Set up process event handlers
      this.setupProcessHandlers(managedProcess);
      
      // Wait for process to be ready
      await this.waitForProcessReady(processId);
      
      this.log(`✅ ${daemonType} process spawned successfully: ${processId}`);
      this.emit('process:spawned', { processId, daemonType });
      
      return processId;
      
    } catch (error) {
      this.log(`❌ Failed to spawn ${daemonType}: ${error.message}`, 'error');
      this.processes.delete(processId);
      throw error;
    }
  }

  async kill(processId: string): Promise<void> {
    const managedProcess = this.processes.get(processId);
    if (!managedProcess) {
      this.log(`⚠️ Process not found: ${processId}`, 'warn');
      return;
    }

    this.log(`🔥 Killing process: ${processId}`);
    
    try {
      // Send graceful shutdown signal
      managedProcess.childProcess.send?.({ type: 'shutdown' });
      
      // Wait for graceful shutdown
      await new Promise((resolve) => {
        const timeout = setTimeout(() => {
          // Force kill if not graceful
          managedProcess.childProcess.kill('SIGKILL');
          resolve(undefined);
        }, 5000);
        
        managedProcess.childProcess.on('exit', () => {
          clearTimeout(timeout);
          resolve(undefined);
        });
      });
      
      this.processes.delete(processId);
      this.log(`✅ Process killed: ${processId}`);
      this.emit('process:killed', { processId });
      
    } catch (error) {
      this.log(`❌ Error killing process ${processId}: ${error.message}`, 'error');
    }
  }

  async restart(processId: string): Promise<void> {
    const managedProcess = this.processes.get(processId);
    if (!managedProcess) {
      throw new Error(`Process not found: ${processId}`);
    }

    const daemonType = managedProcess.daemonType;
    
    this.log(`🔄 Restarting process: ${processId}`);
    
    await this.kill(processId);
    await new Promise(resolve => setTimeout(resolve, 1000));
    await this.spawn(daemonType);
  }

  // Message routing
  async route(message: ProcessMessage): Promise<ProcessResult> {
    // Find process by target or capability
    const targetProcessId = message.targetProcess || this.findProcessByCapability(message.type);
    
    if (!targetProcessId) {
      return {
        success: false,
        error: `No process available for message type: ${message.type}`
      };
    }

    return await this.sendMessageToProcess(targetProcessId, message);
  }

  async broadcast(message: ProcessMessage): Promise<ProcessResult[]> {
    const results: ProcessResult[] = [];
    
    for (const processId of this.processes.keys()) {
      try {
        const result = await this.sendMessageToProcess(processId, message);
        results.push(result);
      } catch (error) {
        results.push({
          success: false,
          error: error.message,
          processId
        });
      }
    }
    
    return results;
  }

  // Registry management
  getAvailable(): string[] {
    return Array.from(this.registry.getAvailable().keys());
  }

  getProcessConfig(daemonType: string): ProcessConfig | null {
    return this.registry.getConfig(daemonType);
  }

  // Health monitoring
  async healthCheck(): Promise<ProcessHealth[]> {
    const healthChecks: ProcessHealth[] = [];
    
    for (const [processId, managedProcess] of this.processes) {
      try {
        const result = await this.sendMessageToProcess(processId, {
          id: `health-${Date.now()}`,
          type: 'health',
          data: {},
          timestamp: Date.now()
        });
        
        if (result.success && result.data) {
          healthChecks.push(result.data as ProcessHealth);
        } else {
          healthChecks.push({
            processId,
            status: 'unhealthy',
            uptime: Date.now() - managedProcess.startTime.getTime(),
            memory: 0,
            cpu: 0,
            lastHeartbeat: managedProcess.lastHeartbeat.getTime()
          });
        }
      } catch (error) {
        healthChecks.push({
          processId,
          status: 'unhealthy',
          uptime: Date.now() - managedProcess.startTime.getTime(),
          memory: 0,
          cpu: 0,
          lastHeartbeat: managedProcess.lastHeartbeat.getTime()
        });
      }
    }
    
    return healthChecks;
  }

  onProcessExit(callback: (processId: string) => void): void {
    this.on('process:exit', callback);
  }

  getSystemStatus(): any {
    return {
      coordinator: {
        isRunning: this.isRunning,
        processCount: this.processes.size,
        availableDaemons: this.getAvailable()
      },
      processes: Array.from(this.processes.entries()).map(([processId, process]) => ({
        processId,
        daemonType: process.daemonType,
        uptime: Date.now() - process.startTime.getTime(),
        messageQueueSize: process.messageQueue.length,
        pid: process.childProcess.pid
      }))
    };
  }

  // Private methods
  private generateProcessId(daemonType: string): string {
    return `${daemonType}-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`;
  }

  private setupProcessHandlers(managedProcess: ManagedProcess): void {
    const { processId, childProcess } = managedProcess;

    childProcess.on('message', (message: any) => {
      this.handleProcessMessage(processId, message);
    });

    childProcess.on('exit', (code, signal) => {
      this.log(`Process exited: ${processId} (code: ${code}, signal: ${signal})`);
      this.processes.delete(processId);
      this.emit('process:exit', processId);
    });

    childProcess.on('error', (error) => {
      this.log(`Process error: ${processId} - ${error.message}`, 'error');
    });

    childProcess.stdout?.on('data', (data) => {
      this.log(`[${processId}] ${data.toString().trim()}`);
    });

    childProcess.stderr?.on('data', (data) => {
      this.log(`[${processId}] ${data.toString().trim()}`, 'warn');
    });
  }

  private handleProcessMessage(processId: string, message: any): void {
    const managedProcess = this.processes.get(processId);
    if (!managedProcess) return;

    // Update last heartbeat
    managedProcess.lastHeartbeat = new Date();

    // Handle response to pending message
    if (message.responseId) {
      const pending = managedProcess.pendingMessages.get(message.responseId);
      if (pending) {
        clearTimeout(pending.timeout);
        managedProcess.pendingMessages.delete(message.responseId);
        pending.resolve(message);
        return;
      }
    }

    // Handle new message from process
    if (message.type === 'route_message') {
      this.route(message.message).catch(error => {
        this.log(`Message routing error: ${error.message}`, 'error');
      });
    }
  }

  private async sendMessageToProcess(processId: string, message: ProcessMessage): Promise<ProcessResult> {
    const managedProcess = this.processes.get(processId);
    if (!managedProcess) {
      throw new Error(`Process not found: ${processId}`);
    }

    return new Promise((resolve, reject) => {
      const messageId = `msg-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`;
      
      const timeout = setTimeout(() => {
        managedProcess.pendingMessages.delete(messageId);
        reject(new Error(`Message timeout: ${messageId}`));
      }, 10000);

      managedProcess.pendingMessages.set(messageId, { resolve, reject, timeout });

      const messageWithId = { ...message, responseId: messageId };
      managedProcess.childProcess.send?.(messageWithId);
    });
  }

  private findProcessByCapability(capability: string): string | null {
    for (const [processId, managedProcess] of this.processes) {
      if (managedProcess.config.capabilities.includes(capability)) {
        return processId;
      }
    }
    return null;
  }

  private async waitForProcessReady(processId: string, timeoutMs = 10000): Promise<void> {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      try {
        await this.sendMessageToProcess(processId, {
          id: `ready-check-${Date.now()}`,
          type: 'ping',
          data: {},
          timestamp: Date.now()
        });
        return; // Process is ready
      } catch {
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    }
    
    throw new Error(`Process not ready within timeout: ${processId}`);
  }

  private startHealthMonitoring(): void {
    this.healthCheckInterval = setInterval(async () => {
      try {
        const health = await this.healthCheck();
        const unhealthyProcesses = health.filter(h => h.status === 'unhealthy');
        
        for (const unhealthy of unhealthyProcesses) {
          this.log(`⚠️ Unhealthy process detected: ${unhealthy.processId}`, 'warn');
          
          const managedProcess = this.processes.get(unhealthy.processId);
          if (managedProcess?.config.restartOnCrash) {
            this.restart(unhealthy.processId).catch(error => {
              this.log(`Failed to restart unhealthy process: ${error.message}`, 'error');
            });
          }
        }
      } catch (error) {
        this.log(`Health check error: ${error.message}`, 'error');
      }
    }, 30000); // Check every 30 seconds
  }

  private log(message: string, level: 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [ProcessCoordinator]`;
    
    switch (level) {
      case 'error':
        console.error(`${prefix} ❌ ${message}`);
        break;
      case 'warn':
        console.warn(`${prefix} ⚠️ ${message}`);
        break;
      default:
        console.log(`${prefix} ${message}`);
    }
  }
}