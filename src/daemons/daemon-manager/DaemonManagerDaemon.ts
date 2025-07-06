/**
 * Daemon Manager Daemon - The master daemon that manages all other daemons
 * 
 * This is the ONLY daemon started by main.ts
 * It monitors health and restarts daemons that die
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';

interface DaemonControlRequest {
  daemon: string; // daemon name
}

interface DaemonConfig {
  name: string;
  module: string;
  critical: boolean;
  restartDelay?: number;
  maxRestarts?: number;
}

interface ManagedDaemon {
  config: DaemonConfig;
  process?: ChildProcess;
  restarts: number;
  lastRestart?: Date;
  status: 'stopped' | 'starting' | 'running' | 'failed';
}

export class DaemonManagerDaemon extends BaseDaemon {
  public readonly name = 'daemon-manager';
  public readonly version = '1.0.0';
  
  private daemons = new Map<string, ManagedDaemon>();
  private healthCheckInterval?: NodeJS.Timeout;
  
  // Daemon startup order and configuration
  private daemonConfigs: DaemonConfig[] = [
    // Core infrastructure
    { name: 'continuum-directory', module: '../continuum-directory/ContinuumDirectoryDaemon', critical: true },
    { name: 'session-manager', module: '../session-manager/SessionManagerDaemon', critical: true },
    { name: 'static-file', module: '../static-file/StaticFileDaemon', critical: true },
    { name: 'websocket', module: '../../integrations/websocket/WebSocketDaemon', critical: true },
    { name: 'renderer', module: '../renderer/RendererDaemon', critical: true },
    { name: 'command-processor', module: '../command-processor/CommandProcessorDaemon', critical: true },
    
    // Feature daemons
    { name: 'browser-manager', module: '../browser-manager/BrowserManagerDaemon', critical: false },
    { name: 'chatroom', module: '../chatroom/ChatRoomDaemon', critical: false },
    { name: 'academy', module: '../academy/AcademyDaemon', critical: false },
  ];
  
  constructor() {
    super();
    
    // Initialize daemon configurations
    for (const config of this.daemonConfigs) {
      this.daemons.set(config.name, {
        config,
        restarts: 0,
        status: 'stopped'
      });
    }
  }
  
  protected async onStart(): Promise<void> {
    this.log('🚀 Daemon Manager starting - I will manage all other daemons');
    
    // Start all daemons in order
    for (const [name, _daemon] of this.daemons) {
      await this.startDaemon(name);
    }
    
    // Start health monitoring
    this.startHealthMonitoring();
    
    this.log('✅ All daemons started, monitoring health');
  }
  
  protected async onStop(): Promise<void> {
    this.log('🛑 Daemon Manager stopping - shutting down all daemons');
    
    // Stop health monitoring
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }
    
    // Stop all daemons in reverse order
    const daemonNames = Array.from(this.daemons.keys()).reverse();
    for (const name of daemonNames) {
      await this.stopDaemon(name);
    }
  }
  
  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'status':
        return this.getDaemonStatuses();
      
      case 'restart_daemon':
        return this.restartDaemon((message.data as DaemonControlRequest).daemon);
      
      case 'stop_daemon':
        return this.stopDaemon((message.data as DaemonControlRequest).daemon);
      
      case 'start_daemon':
        return this.startDaemon((message.data as DaemonControlRequest).daemon);
      
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }
  
  private async startDaemon(name: string): Promise<DaemonResponse> {
    const daemon = this.daemons.get(name);
    if (!daemon) {
      return {
        success: false,
        error: `Unknown daemon: ${name}`
      };
    }
    
    if (daemon.status === 'running') {
      return {
        success: true,
        data: { message: `${name} already running` }
      };
    }
    
    daemon.status = 'starting';
    this.log(`🚀 Starting ${name} daemon...`);
    
    try {
      // Spawn daemon as a separate process
      const modulePath = path.join(__dirname, daemon.config.module);
      daemon.process = spawn('tsx', [modulePath], {
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: false,
        env: {
          ...process.env,
          DAEMON_NAME: name
        }
      });
      
      // Handle stdout
      daemon.process.stdout?.on('data', (data) => {
        this.log(`[${name}] ${data.toString().trim()}`);
      });
      
      // Handle stderr
      daemon.process.stderr?.on('data', (data) => {
        this.log(`[${name}] ERROR: ${data.toString().trim()}`, 'error');
      });
      
      // Handle exit
      daemon.process.on('exit', (code) => {
        daemon.status = 'stopped';
        delete daemon.process;
        
        if (code !== 0) {
          this.log(`❌ ${name} daemon crashed with code ${code}`, 'error');
          this.handleDaemonCrash(name);
        }
      });
      
      daemon.status = 'running';
      daemon.lastRestart = new Date();
      
      this.log(`✅ ${name} daemon started`);
      
      return {
        success: true,
        data: { pid: daemon.process.pid }
      };
      
    } catch (error) {
      daemon.status = 'failed';
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to start ${name}: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }
  
  private async stopDaemon(name: string): Promise<DaemonResponse> {
    const daemon = this.daemons.get(name);
    if (!daemon || !daemon.process) {
      return {
        success: true,
        data: { message: `${name} not running` }
      };
    }
    
    this.log(`🛑 Stopping ${name} daemon...`);
    
    // Send SIGTERM for graceful shutdown
    daemon.process.kill('SIGTERM');
    
    // Wait for process to exit
    await new Promise<void>((resolve) => {
      let timeout = setTimeout(() => {
        // Force kill if not stopped gracefully
        if (daemon.process) {
          this.log(`⚠️ Force killing ${name} daemon`, 'warn');
          daemon.process.kill('SIGKILL');
        }
        resolve();
      }, 5000);
      
      daemon.process!.on('exit', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
    
    daemon.status = 'stopped';
    delete daemon.process;
    
    this.log(`✅ ${name} daemon stopped`);
    
    return {
      success: true,
      data: { message: `${name} stopped` }
    };
  }
  
  private async restartDaemon(name: string): Promise<DaemonResponse> {
    await this.stopDaemon(name);
    return this.startDaemon(name);
  }
  
  private handleDaemonCrash(name: string): void {
    const daemon = this.daemons.get(name);
    if (!daemon) return;
    
    daemon.restarts++;
    
    const maxRestarts = daemon.config.maxRestarts || 5;
    if (daemon.restarts > maxRestarts) {
      daemon.status = 'failed';
      this.log(`❌ ${name} daemon exceeded max restarts (${maxRestarts}), giving up`, 'error');
      
      if (daemon.config.critical) {
        this.log(`🚨 CRITICAL DAEMON FAILED: ${name} - system may be unstable`, 'error');
      }
      return;
    }
    
    const restartDelay = daemon.config.restartDelay || 1000;
    this.log(`🔄 Restarting ${name} daemon in ${restartDelay}ms (attempt ${daemon.restarts}/${maxRestarts})`);
    
    setTimeout(() => {
      this.startDaemon(name);
    }, restartDelay);
  }
  
  private startHealthMonitoring(): void {
    // Check daemon health every 10 seconds
    this.healthCheckInterval = setInterval(() => {
      for (const [name, daemon] of this.daemons) {
        if (daemon.status === 'running' && daemon.process) {
          try {
            // Check if process is still alive
            process.kill(daemon.process.pid!, 0);
          } catch {
            // Process is dead
            this.log(`⚠️ ${name} daemon died unexpectedly`, 'warn');
            daemon.status = 'stopped';
            delete daemon.process;
            this.handleDaemonCrash(name);
          }
        }
      }
    }, 10000);
  }
  
  private getDaemonStatuses(): DaemonResponse {
    const status: Record<string, any> = {};
    
    for (const [name, daemon] of this.daemons) {
      status[name] = {
        status: daemon.status,
        pid: daemon.process?.pid,
        restarts: daemon.restarts,
        lastRestart: daemon.lastRestart,
        critical: daemon.config.critical
      };
    }
    
    return {
      success: true,
      data: {
        daemons: status,
        healthy: Array.from(this.daemons.values()).every(d => 
          !d.config.critical || d.status === 'running'
        )
      }
    };
  }
}