/**
 * SystemManager - Resilient System Startup and Coordination
 * Manages daemon lifecycle, health monitoring, and graceful shutdown
 */

import { BaseDaemon } from '../daemons/base/BaseDaemon.js';
import { WebSocketDaemon } from '../integrations/websocket/WebSocketDaemon.js';
import { EventEmitter } from 'events';

export interface SystemHealthStatus {
  overall: 'healthy' | 'degraded' | 'critical' | 'starting' | 'stopping';
  daemons: Record<string, {
    status: 'running' | 'stopped' | 'error' | 'starting' | 'stopping';
    uptime: number;
    lastError?: string;
    restartCount: number;
  }>;
  startTime: string;
  systemUptime: number;
}

export interface DaemonConfig {
  name: string;
  daemonClass: typeof BaseDaemon;
  config?: any;
  autoRestart: boolean;
  maxRestarts: number;
  dependencies?: string[];
  healthCheckInterval: number;
}

export class SystemManager extends EventEmitter {
  private daemons = new Map<string, {
    instance: BaseDaemon;
    config: DaemonConfig;
    restartCount: number;
    lastStartTime: Date;
    healthCheckTimer?: NodeJS.Timeout;
  }>();
  
  private systemStartTime = new Date();
  private shutdownInProgress = false;
  private startupPromise?: Promise<void>;

  constructor() {
    super();
    this.setupSignalHandlers();
  }

  /**
   * Register a daemon for management
   */
  registerDaemon(config: DaemonConfig): void {
    if (this.daemons.has(config.name)) {
      throw new Error(`Daemon ${config.name} already registered`);
    }

    console.log(`📋 Registering daemon: ${config.name}`);
    
    const instance = new config.daemonClass(config.config);
    
    this.daemons.set(config.name, {
      instance,
      config,
      restartCount: 0,
      lastStartTime: new Date(),
      healthCheckTimer: undefined
    });

    // Setup daemon event handlers
    instance.on('error', (error) => this.handleDaemonError(config.name, error));
    instance.on('stopped', () => this.handleDaemonStopped(config.name));
  }

  /**
   * Start all registered daemons with dependency resolution
   */
  async startSystem(): Promise<void> {
    if (this.startupPromise) {
      return this.startupPromise;
    }

    this.startupPromise = this.performStartup();
    return this.startupPromise;
  }

  private async performStartup(): Promise<void> {
    console.log('🚀 Starting Continuum system...');
    this.emit('system:starting');

    try {
      // Start daemons in dependency order
      const startOrder = this.resolveDependencyOrder();
      
      for (const daemonName of startOrder) {
        await this.startDaemon(daemonName);
      }

      // Start health monitoring
      this.startHealthMonitoring();

      console.log('✅ System startup complete');
      this.emit('system:started');
    } catch (error) {
      console.error('❌ System startup failed:', error);
      this.emit('system:error', error);
      throw error;
    }
  }

  /**
   * Start a specific daemon with retry logic
   */
  private async startDaemon(daemonName: string): Promise<void> {
    const daemonInfo = this.daemons.get(daemonName);
    if (!daemonInfo) {
      throw new Error(`Daemon ${daemonName} not found`);
    }

    const { instance, config } = daemonInfo;
    
    console.log(`🔄 Starting daemon: ${daemonName}`);
    
    try {
      await instance.start();
      daemonInfo.lastStartTime = new Date();
      
      console.log(`✅ Daemon started: ${daemonName}`);
      this.emit('daemon:started', daemonName);
      
      // Register daemon with WebSocket daemon for inter-daemon communication
      await this.registerDaemonWithWebSocket(daemonName, instance);
      
      // Start health checks
      this.startDaemonHealthCheck(daemonName);
      
    } catch (error) {
      console.error(`❌ Failed to start daemon ${daemonName}:`, error);
      this.emit('daemon:error', daemonName, error);
      
      if (config.autoRestart && daemonInfo.restartCount < config.maxRestarts) {
        console.log(`🔄 Scheduling restart for ${daemonName} (attempt ${daemonInfo.restartCount + 1}/${config.maxRestarts})`);
        setTimeout(() => this.restartDaemon(daemonName), 5000);
      } else {
        throw error;
      }
    }
  }

  /**
   * Restart a daemon
   */
  private async restartDaemon(daemonName: string): Promise<void> {
    const daemonInfo = this.daemons.get(daemonName);
    if (!daemonInfo) return;

    daemonInfo.restartCount++;
    
    try {
      await daemonInfo.instance.stop();
      await this.startDaemon(daemonName);
    } catch (error) {
      console.error(`❌ Restart failed for ${daemonName}:`, error);
      this.emit('daemon:restart_failed', daemonName, error);
    }
  }

  /**
   * Stop all daemons gracefully
   */
  async stopSystem(): Promise<void> {
    if (this.shutdownInProgress) return;
    
    this.shutdownInProgress = true;
    console.log('🛑 Stopping Continuum system...');
    this.emit('system:stopping');

    // Stop health monitoring
    for (const [, daemonInfo] of this.daemons) {
      if (daemonInfo.healthCheckTimer) {
        clearInterval(daemonInfo.healthCheckTimer);
      }
    }

    // Stop daemons in reverse dependency order
    const stopOrder = this.resolveDependencyOrder().reverse();
    
    for (const daemonName of stopOrder) {
      try {
        const daemonInfo = this.daemons.get(daemonName);
        if (daemonInfo) {
          console.log(`🔄 Stopping daemon: ${daemonName}`);
          await daemonInfo.instance.stop();
          console.log(`✅ Daemon stopped: ${daemonName}`);
        }
      } catch (error) {
        console.error(`❌ Error stopping daemon ${daemonName}:`, error);
      }
    }

    console.log('✅ System shutdown complete');
    this.emit('system:stopped');
  }

  /**
   * Get comprehensive system health status
   */
  getSystemHealth(): SystemHealthStatus {
    const daemonStatuses: Record<string, any> = {};
    let hasError = false;
    let hasStarting = false;

    for (const [name, daemonInfo] of this.daemons) {
      const uptime = Date.now() - daemonInfo.lastStartTime.getTime();
      const status = daemonInfo.instance.getStatus();
      
      daemonStatuses[name] = {
        status,
        uptime: Math.floor(uptime / 1000),
        restartCount: daemonInfo.restartCount,
        lastError: undefined // Could be enhanced to track errors
      };

      if (status === 'error') hasError = true;
      if (status === 'starting') hasStarting = true;
    }

    let overall: SystemHealthStatus['overall'] = 'healthy';
    if (this.shutdownInProgress) overall = 'stopping';
    else if (this.startupPromise && hasStarting) overall = 'starting';
    else if (hasError) overall = 'critical';

    return {
      overall,
      daemons: daemonStatuses,
      startTime: this.systemStartTime.toISOString(),
      systemUptime: Math.floor((Date.now() - this.systemStartTime.getTime()) / 1000)
    };
  }

  /**
   * Resolve daemon startup order based on dependencies
   */
  private resolveDependencyOrder(): string[] {
    const visited = new Set<string>();
    const visiting = new Set<string>();
    const order: string[] = [];

    const visit = (daemonName: string) => {
      if (visiting.has(daemonName)) {
        throw new Error(`Circular dependency detected involving ${daemonName}`);
      }
      if (visited.has(daemonName)) return;

      visiting.add(daemonName);
      
      const daemonInfo = this.daemons.get(daemonName);
      if (daemonInfo?.config.dependencies) {
        for (const dep of daemonInfo.config.dependencies) {
          if (!this.daemons.has(dep)) {
            throw new Error(`Dependency ${dep} not found for daemon ${daemonName}`);
          }
          visit(dep);
        }
      }

      visiting.delete(daemonName);
      visited.add(daemonName);
      order.push(daemonName);
    };

    for (const daemonName of this.daemons.keys()) {
      visit(daemonName);
    }

    return order;
  }

  /**
   * Start health monitoring for all daemons
   */
  private startHealthMonitoring(): void {
    for (const [daemonName] of this.daemons) {
      this.startDaemonHealthCheck(daemonName);
    }
  }

  /**
   * Start health check for a specific daemon
   */
  private startDaemonHealthCheck(daemonName: string): void {
    const daemonInfo = this.daemons.get(daemonName);
    if (!daemonInfo) return;

    if (daemonInfo.healthCheckTimer) {
      clearInterval(daemonInfo.healthCheckTimer);
    }

    daemonInfo.healthCheckTimer = setInterval(async () => {
      try {
        const status = daemonInfo.instance.getStatus();
        if (status === 'error' && daemonInfo.config.autoRestart) {
          console.log(`🚨 Health check failed for ${daemonName}, restarting...`);
          await this.restartDaemon(daemonName);
        }
      } catch (error) {
        console.error(`❌ Health check error for ${daemonName}:`, error);
      }
    }, daemonInfo.config.healthCheckInterval);
  }

  /**
   * Handle daemon errors
   */
  private handleDaemonError(daemonName: string, error: Error): void {
    console.error(`❌ Daemon error ${daemonName}:`, error);
    this.emit('daemon:error', daemonName, error);
    
    const daemonInfo = this.daemons.get(daemonName);
    if (daemonInfo?.config.autoRestart && daemonInfo.restartCount < daemonInfo.config.maxRestarts) {
      setTimeout(() => this.restartDaemon(daemonName), 2000);
    }
  }

  /**
   * Register daemon with WebSocket daemon for inter-daemon communication
   */
  private async registerDaemonWithWebSocket(daemonName: string, daemon: BaseDaemon): Promise<void> {
    // Find WebSocket daemon instance
    const wsInfo = this.daemons.get('websocket-server');
    if (wsInfo && wsInfo.instance) {
      try {
        // Register this daemon with the WebSocket daemon  
        (wsInfo.instance as any).registerExternalDaemon(daemonName, daemon);
        console.log(`🔌 Registered ${daemonName} with WebSocket daemon`);
      } catch (error) {
        console.log(`⚠️ Failed to register ${daemonName} with WebSocket daemon: ${error.message}`);
      }
    } else {
      console.log(`⚠️ WebSocket daemon not available for registering ${daemonName}`);
    }
  }

  /**
   * Handle daemon stopped events
   */
  private handleDaemonStopped(daemonName: string): void {
    console.log(`📴 Daemon stopped: ${daemonName}`);
    this.emit('daemon:stopped', daemonName);
  }

  /**
   * Setup signal handlers for graceful shutdown
   */
  private setupSignalHandlers(): void {
    const signals = ['SIGINT', 'SIGTERM', 'SIGUSR2'];
    
    for (const signal of signals) {
      process.on(signal, async () => {
        console.log(`\n🛑 Received ${signal}, shutting down gracefully...`);
        try {
          await this.stopSystem();
          process.exit(0);
        } catch (error) {
          console.error('❌ Shutdown error:', error);
          process.exit(1);
        }
      });
    }

    process.on('uncaughtException', (error) => {
      console.error('❌ Uncaught exception:', error);
      this.emit('system:error', error);
    });

    process.on('unhandledRejection', (reason) => {
      console.error('❌ Unhandled rejection:', reason);
      this.emit('system:error', reason);
    });
  }
}