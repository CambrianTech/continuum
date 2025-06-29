/**
 * Base Process Daemon Implementation
 * Abstract base class for all daemon processes
 */

import { IProcessDaemon } from '../interfaces/IProcessDaemon.js';
import { ProcessMessage, ProcessResult, ProcessHealth } from '../interfaces/IProcessCoordinator.js';

export abstract class BaseProcessDaemon implements IProcessDaemon {
  abstract readonly daemonType: string;
  abstract readonly capabilities: string[];
  
  public readonly processId: string;
  private startTime: Date;
  private lastHeartbeat: Date;
  private isRunning = false;
  private messageHandlers = new Map<string, (message: ProcessMessage) => Promise<ProcessResult>>();

  constructor() {
    this.processId = this.generateProcessId();
    this.startTime = new Date();
    this.lastHeartbeat = new Date();
    this.setupDefaultHandlers();
  }

  // Lifecycle methods (concrete implementations)
  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error(`Daemon ${this.daemonType} is already running`);
    }

    this.log('🚀 Starting daemon...');
    await this.onStart();
    this.isRunning = true;
    this.startHeartbeat();
    this.log('✅ Daemon started successfully');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.log('🛑 Stopping daemon...');
    await this.onStop();
    this.isRunning = false;
    this.log('✅ Daemon stopped');
  }

  async restart(): Promise<void> {
    this.log('🔄 Restarting daemon...');
    await this.stop();
    await new Promise(resolve => setTimeout(resolve, 1000));
    await this.start();
  }

  // Message handling
  async handleMessage(message: ProcessMessage): Promise<ProcessResult> {
    try {
      this.heartbeat();
      
      const handler = this.messageHandlers.get(message.type);
      if (handler) {
        return await handler(message);
      }

      // Delegate to subclass
      return await this.onMessage(message);
    } catch (error) {
      this.log(`❌ Message handling error: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message,
        processId: this.processId
      };
    }
  }

  async sendMessage(targetProcess: string, message: ProcessMessage): Promise<void> {
    // Delegate to transport layer (process.send, postMessage, etc.)
    await this.sendToTarget(targetProcess, message);
  }

  // Health monitoring
  getHealth(): ProcessHealth {
    const now = Date.now();
    return {
      processId: this.processId,
      status: this.isRunning ? 'healthy' : 'unhealthy',
      uptime: now - this.startTime.getTime(),
      memory: this.getMemoryUsage(),
      cpu: this.getCpuUsage(),
      lastHeartbeat: this.lastHeartbeat.getTime()
    };
  }

  heartbeat(): void {
    this.lastHeartbeat = new Date();
  }

  registerCapabilities(): string[] {
    return [...this.capabilities];
  }

  getConfiguration(): any {
    return {
      daemonType: this.daemonType,
      processId: this.processId,
      capabilities: this.capabilities,
      startTime: this.startTime,
      isRunning: this.isRunning
    };
  }

  // Abstract methods for subclasses
  protected abstract onStart(): Promise<void>;
  protected abstract onStop(): Promise<void>;
  protected abstract onMessage(message: ProcessMessage): Promise<ProcessResult>;
  protected abstract sendToTarget(targetProcess: string, message: ProcessMessage): Promise<void>;

  // Utility methods
  private generateProcessId(): string {
    return `${this.daemonType}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  private setupDefaultHandlers(): void {
    this.messageHandlers.set('ping', async () => ({
      success: true,
      data: { pong: true, timestamp: Date.now() },
      processId: this.processId
    }));

    this.messageHandlers.set('health', async () => ({
      success: true,
      data: this.getHealth(),
      processId: this.processId
    }));

    this.messageHandlers.set('config', async () => ({
      success: true,
      data: this.getConfiguration(),
      processId: this.processId
    }));
  }

  private startHeartbeat(): void {
    setInterval(() => {
      this.heartbeat();
    }, 5000); // Heartbeat every 5 seconds
  }

  private getMemoryUsage(): number {
    if (typeof process !== 'undefined' && process.memoryUsage) {
      return process.memoryUsage().heapUsed;
    }
    return 0; // Web worker fallback
  }

  private getCpuUsage(): number {
    if (typeof process !== 'undefined' && process.cpuUsage) {
      const usage = process.cpuUsage();
      return (usage.user + usage.system) / 1000000; // Convert to seconds
    }
    return 0; // Web worker fallback
  }

  protected log(message: string, level: 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [${this.daemonType}:${this.processId.slice(-8)}]`;
    
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