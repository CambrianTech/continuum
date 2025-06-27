/**
 * Daemon Manager - TypeScript Implementation
 * Clean daemon orchestration for Continuum OS
 */

import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

export interface DaemonConfig {
  readonly name: string;
  readonly path: string;
  readonly critical: boolean;
  readonly autoRestart: boolean;
  readonly dependencies: string[];
  readonly env?: Record<string, string>;
}

export interface DaemonProcess {
  readonly config: DaemonConfig;
  readonly process: ChildProcess;
  readonly startTime: Date;
  status: 'starting' | 'running' | 'stopping' | 'stopped' | 'failed';
  restartCount: number;
}

export class DaemonManager extends EventEmitter {
  private daemons = new Map<string, DaemonProcess>();
  private readonly configs: DaemonConfig[] = [
    {
      name: 'command-processor',
      path: 'src/daemons/command-processor/CommandProcessorDaemon.ts',
      critical: true,
      autoRestart: true,
      dependencies: []
    },
    {
      name: 'websocket-server',
      path: 'src/integrations/websocket/WebSocketDaemon.ts',
      critical: true,
      autoRestart: true,
      dependencies: ['command-processor']
    },
    {
      name: 'renderer',
      path: 'src/daemons/renderer/RendererDaemon.ts',
      critical: false, // Not critical - system can run without UI
      autoRestart: true,
      dependencies: ['websocket-server']
    },
    {
      name: 'browser-manager',
      path: 'src/daemons/browser-manager/BrowserManagerDaemon.ts',
      critical: false, // Not critical - system can run without browser management
      autoRestart: true,
      dependencies: ['websocket-server']
    }
    // Note: mesh-coordinator will be added later when needed
  ];

  async startAll(): Promise<void> {
    console.log('🚀 Starting TypeScript daemon ecosystem with dynamic routing...');
    
    // Start daemons in dependency order
    const started: string[] = [];
    let webSocketDaemon: any = null;
    
    for (const config of this.configs) {
      // Wait for dependencies
      for (const dep of config.dependencies) {
        if (!started.includes(dep)) {
          throw new Error(`Dependency ${dep} not started for ${config.name}`);
        }
      }
      
      try {
        await this.startDaemon(config);
        started.push(config.name);
      } catch (error) {
        if (config.critical) {
          throw error; // Re-throw for critical daemons
        } else {
          console.log(`⚠️  Non-critical daemon ${config.name} failed to start: ${error.message}`);
          console.log(`✅ Continuing without ${config.name}...`);
        }
      }
      
      // Keep reference to WebSocket daemon for dynamic registration
      const daemon = this.daemons.get(config.name);
      if (config.name === 'websocket-server' && daemon) {
        webSocketDaemon = daemon.process; // This needs to be the actual daemon instance
        console.log('📡 Found WebSocket daemon for dynamic registration');
      }
      
      // Register non-WebSocket daemons with the WebSocket daemon
      if (webSocketDaemon && config.name !== 'websocket-server' && daemon) {
        console.log(`🔌 Registering ${config.name} with WebSocket daemon...`);
        // TODO: Need to get actual daemon instance, not process
        // await webSocketDaemon.registerExternalDaemon(config.name, daemon.actualDaemon);
      }
    }
    
    console.log('✅ TypeScript daemon ecosystem started with dynamic routing');
    this.logSystemStatus();
  }

  async startDaemon(config: DaemonConfig): Promise<void> {
    if (this.daemons.has(config.name)) {
      throw new Error(`Daemon ${config.name} already exists`);
    }

    console.log(`🚀 Starting daemon: ${config.name}`);
    
    const childProcess = spawn('npx', ['tsx', config.path], {
      stdio: ['pipe', 'pipe', 'pipe'],
      cwd: process.cwd(),
      env: { ...process.env, ...config.env }
    });

    const daemonProcess: DaemonProcess = {
      config,
      process: childProcess,
      startTime: new Date(),
      status: 'starting',
      restartCount: 0
    };

    this.daemons.set(config.name, daemonProcess);
    this.setupProcessHandlers(daemonProcess);
    
    // Wait for startup
    await this.waitForStartup(daemonProcess);
    
    console.log(`✅ Daemon started: ${config.name}`);
  }

  async stopDaemon(name: string): Promise<void> {
    const daemon = this.daemons.get(name);
    if (!daemon) {
      return;
    }

    console.log(`🛑 Stopping daemon: ${name}`);
    daemon.status = 'stopping';
    
    daemon.process.kill('SIGTERM');
    
    // Wait for graceful shutdown
    await new Promise<void>((resolve) => {
      const timeout = setTimeout(() => {
        daemon.process.kill('SIGKILL');
        resolve();
      }, 5000);
      
      daemon.process.on('exit', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
    
    daemon.status = 'stopped';
    this.daemons.delete(name);
    
    console.log(`✅ Daemon stopped: ${name}`);
  }

  async stopAll(): Promise<void> {
    console.log('🛑 Stopping all daemons...');
    
    const stopPromises = Array.from(this.daemons.keys()).map(name => 
      this.stopDaemon(name)
    );
    
    await Promise.all(stopPromises);
    
    console.log('✅ All daemons stopped');
  }

  getDaemonStatus(name: string): string | null {
    const daemon = this.daemons.get(name);
    return daemon ? daemon.status : null;
  }

  getAllStatus(): Record<string, any> {
    const status: Record<string, any> = {};
    
    for (const [name, daemon] of this.daemons) {
      status[name] = {
        status: daemon.status,
        startTime: daemon.startTime,
        restartCount: daemon.restartCount,
        critical: daemon.config.critical,
        pid: daemon.process.pid
      };
    }
    
    return status;
  }

  /**
   * Log comprehensive system status for debugging
   */
  private logSystemStatus(): void {
    console.log('\n📊 DAEMON ECOSYSTEM STATUS:');
    console.log('='.repeat(50));
    
    for (const [name, daemon] of this.daemons) {
      console.log(`📋 ${name}:`);
      console.log(`   Status: ${daemon.status}`);
      console.log(`   PID: ${daemon.process.pid}`);
      console.log(`   Restarts: ${daemon.restartCount}`);
      console.log(`   Critical: ${daemon.config.critical}`);
      console.log(`   Dependencies: ${daemon.config.dependencies.join(', ') || 'none'}`);
    }
    
    console.log('='.repeat(50));
    console.log(`✅ Total Daemons: ${this.daemons.size}`);
    console.log(`📊 System Health: ${this.getSystemHealth()}`);
    console.log('');
  }

  /**
   * Get overall system health assessment
   */
  private getSystemHealth(): string {
    const allStatuses = Array.from(this.daemons.values()).map(d => d.status);
    const runningCount = allStatuses.filter(s => s === 'running').length;
    const totalCount = allStatuses.length;
    
    if (runningCount === totalCount) return 'HEALTHY';
    if (runningCount > totalCount / 2) return 'DEGRADED';
    return 'CRITICAL';
  }

  private setupProcessHandlers(daemon: DaemonProcess): void {
    const { name } = daemon.config;
    
    daemon.process.stdout?.on('data', (data) => {
      console.log(`[${name}] ${data.toString().trim()}`);
      
      // Listen for daemon events in stdout
      const output = data.toString();
      if (output.includes('ERROR:') || output.includes('❌')) {
        this.emit('daemon:error', { name, error: output.trim() });
      }
      if (output.includes('started') || output.includes('✅')) {
        this.emit('daemon:healthy', { name });
      }
    });

    daemon.process.stderr?.on('data', (data) => {
      console.error(`[${name}] ${data.toString().trim()}`);
      
      // All stderr is treated as potential error
      this.emit('daemon:warning', { name, warning: data.toString().trim() });
    });

    daemon.process.on('exit', (code) => {
      console.log(`[${name}] Exited with code ${code}`);
      daemon.status = code === 0 ? 'stopped' : 'failed';
      
      this.emit('daemon:exit', { name, code });
      
      // Self-healing: Auto-restart critical daemons that fail
      if (daemon.config.autoRestart && code !== 0 && daemon.config.critical) {
        console.log(`🔧 Self-healing: Restarting critical daemon ${name}`);
        this.restartDaemon(daemon);
      }
    });

    daemon.process.on('error', (error) => {
      console.error(`[${name}] Process error:`, error);
      daemon.status = 'failed';
      this.emit('daemon:error', { name, error });
      
      // Self-healing: Restart on process errors
      if (daemon.config.critical && daemon.config.autoRestart) {
        console.log(`🔧 Self-healing: Restarting daemon ${name} after process error`);
        setTimeout(() => this.restartDaemon(daemon), 2000);
      }
    });
  }

  private async waitForStartup(daemon: DaemonProcess): Promise<void> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Daemon ${daemon.config.name} startup timeout`));
      }, 30000);

      // Look for startup confirmation in logs
      const handleOutput = (data: Buffer) => {
        const output = data.toString();
        if (output.includes('INFO: Starting daemon') || output.includes('started') || output.includes('✅')) {
          clearTimeout(timeout);
          daemon.status = 'running';
          resolve();
        }
      };

      daemon.process.stdout?.on('data', handleOutput);
      daemon.process.stderr?.on('data', handleOutput);
    });
  }

  private async restartDaemon(daemon: DaemonProcess): Promise<void> {
    const { name } = daemon.config;
    
    if (daemon.restartCount >= 3) {
      console.error(`❌ Daemon ${name} failed too many times, not restarting`);
      return;
    }

    console.log(`🔄 Restarting daemon: ${name} (attempt ${daemon.restartCount + 1})`);
    
    daemon.restartCount++;
    this.daemons.delete(name);
    
    // Wait a bit before restart
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    try {
      await this.startDaemon(daemon.config);
    } catch (error) {
      console.error(`❌ Failed to restart daemon ${name}:`, error);
    }
  }
}

export default DaemonManager;