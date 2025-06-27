/**
 * ContinuumSystem - Main System Entry Point
 * Configures and starts all required daemons for the Continuum OS
 */

import { SystemManager } from './SystemManager.js';
import { WebSocketDaemon } from '../integrations/websocket/WebSocketDaemon.js';
import { RendererDaemon } from '../daemons/renderer/RendererDaemon.js';
import { CommandProcessorDaemon } from '../daemons/command-processor/CommandProcessorDaemon.js';

export class ContinuumSystem {
  private systemManager: SystemManager;
  private config: any;

  constructor(config: any = {}) {
    this.config = {
      // WebSocket Server Configuration
      websocket: {
        port: config.websocket?.port || 9000,
        host: config.websocket?.host || 'localhost',
        maxClients: config.websocket?.maxClients || 100,
        enableHeartbeat: config.websocket?.enableHeartbeat ?? true,
        enableAuth: config.websocket?.enableAuth ?? false,
        ...config.websocket
      },
      
      // System Configuration
      system: {
        autoRestart: config.system?.autoRestart ?? true,
        maxRestarts: config.system?.maxRestarts || 3,
        healthCheckInterval: config.system?.healthCheckInterval || 30000,
        ...config.system
      },
      
      ...config
    };

    this.systemManager = new SystemManager();
    this.setupEventHandlers();
    this.registerDaemons();
  }

  /**
   * Register all system daemons
   */
  private registerDaemons(): void {
    console.log('📋 Registering system daemons...');

    // WebSocket Server Daemon (Foundation)
    this.systemManager.registerDaemon({
      name: 'websocket-server',
      daemonClass: WebSocketDaemon,
      config: this.config.websocket,
      autoRestart: this.config.system.autoRestart,
      maxRestarts: this.config.system.maxRestarts,
      dependencies: [], // No dependencies - foundation service
      healthCheckInterval: this.config.system.healthCheckInterval
    });

    // Renderer Daemon (UI Generation)
    this.systemManager.registerDaemon({
      name: 'renderer',
      daemonClass: RendererDaemon,
      config: this.config.renderer || {},
      autoRestart: this.config.system.autoRestart,
      maxRestarts: this.config.system.maxRestarts,
      dependencies: [], // Independent of WebSocket
      healthCheckInterval: this.config.system.healthCheckInterval
    });

    // Command Processor Daemon (Command Routing)
    this.systemManager.registerDaemon({
      name: 'command-processor',
      daemonClass: CommandProcessorDaemon,
      config: this.config.commandProcessor || {},
      autoRestart: this.config.system.autoRestart,
      maxRestarts: this.config.system.maxRestarts,
      dependencies: ['websocket-server'], // Needs WebSocket for command routing
      healthCheckInterval: this.config.system.healthCheckInterval
    });

    console.log('✅ All daemons registered');
  }

  /**
   * Start the complete Continuum system
   */
  async start(): Promise<void> {
    console.log('🌐 Starting Continuum OS...');
    
    try {
      // SMART COORDINATION: Check if service already running
      const existingService = await this.checkExistingService();
      if (existingService.healthy) {
        console.log('✅ Healthy Continuum service already running');
        console.log(`👥 Active browser connections: ${existingService.browserConnections}`);
        console.log('🌐 Opening browser to existing service...');
        await this.launchBrowser();
        return;
      } else if (existingService.exists) {
        console.log('🧹 Found unhealthy service, cleaning up...');
        await this.cleanupExistingService();
      }
      
      await this.systemManager.startSystem();
      console.log('✅ Continuum OS started successfully');
      
      // Log system status
      const health = this.systemManager.getSystemHealth();
      console.log(`📊 System Status: ${health.overall}`);
      console.log(`🕐 System Uptime: ${health.systemUptime}s`);
      console.log(`🔧 Active Daemons: ${Object.keys(health.daemons).length}`);
      
      // Ensure browser interface is available (PRIMARY INTERFACE)
      console.log('🌐 Ensuring browser interface is available...');
      await this.ensureBrowserInterface();
      
    } catch (error) {
      console.error('❌ Failed to start Continuum OS:', error);
      throw error;
    }
  }

  /**
   * Stop the complete Continuum system
   */
  async stop(): Promise<void> {
    console.log('🛑 Stopping Continuum OS...');
    await this.systemManager.stopSystem();
    console.log('✅ Continuum OS stopped');
  }

  /**
   * Get system health status
   */
  getHealth() {
    return this.systemManager.getSystemHealth();
  }

  /**
   * Setup event handlers for system monitoring
   */
  private setupEventHandlers(): void {
    this.systemManager.on('system:starting', () => {
      console.log('🚀 System startup initiated');
    });

    this.systemManager.on('system:started', () => {
      console.log('✅ System startup completed');
      this.logSystemInfo();
    });

    this.systemManager.on('system:stopping', () => {
      console.log('🛑 System shutdown initiated');
    });

    this.systemManager.on('system:stopped', () => {
      console.log('✅ System shutdown completed');
    });

    this.systemManager.on('daemon:started', (daemonName) => {
      console.log(`✅ Daemon started: ${daemonName}`);
    });

    this.systemManager.on('daemon:stopped', (daemonName) => {
      console.log(`📴 Daemon stopped: ${daemonName}`);
    });

    this.systemManager.on('daemon:error', (daemonName, error) => {
      console.error(`❌ Daemon error ${daemonName}:`, error.message);
    });

    this.systemManager.on('daemon:restart_failed', (daemonName, error) => {
      console.error(`❌ Daemon restart failed ${daemonName}:`, error.message);
    });

    this.systemManager.on('system:error', (error) => {
      console.error('❌ System error:', error);
    });
  }

  /**
   * Ensure browser interface is available (PRIMARY INTERFACE)
   */
  private async ensureBrowserInterface(): Promise<void> {
    // Get WebSocket daemon to check for existing connections
    const wsStatus = this.systemManager.getSystemHealth();
    const wsHealthy = wsStatus.overall === 'healthy';
    
    if (!wsHealthy) {
      console.log('⚠️ WebSocket daemon not healthy, skipping browser launch');
      return;
    }

    // Use the browser manager from WebSocket daemon to check connections
    console.log('🔍 Checking for existing browser connections...');
    
    // Give the system a moment to settle
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    try {
      // Check if browser is already connected
      const { default: fetch } = await import('node-fetch');
      const response = await fetch('http://localhost:9000/api/browser/state');
      if (response.ok) {
        const browserState = await response.json();
        
        if (browserState.hasActiveConnections) {
          console.log('✅ Browser already connected');
          console.log(`👥 Active clients: ${browserState.connectedClients.length}`);
          return;
        }
      }
    } catch (error) {
      console.log('⚠️ Could not check browser state, proceeding with launch');
    }

    // No browser connected, launch one
    console.log('🚀 Launching browser interface...');
    await this.launchBrowser();
  }

  /**
   * Launch browser with the Continuum interface
   */
  private async launchBrowser(): Promise<void> {
    const url = `http://localhost:${this.config.websocket.port}`;
    
    try {
      const { spawn } = await import('child_process');
      
      // Check if DevTools mode requested
      const args = process.argv.slice(2);
      const devToolsMode = args.includes('--devtools');
      
      if (devToolsMode) {
        console.log('🛠️ Launching browser with DevTools debugging...');
        await this.launchBrowserWithDevTools(url);
      } else {
        console.log('🌐 Opening browser interface...');
        
        // Use system default browser
        const command = process.platform === 'darwin' ? 'open' : 
                       process.platform === 'win32' ? 'start' : 'xdg-open';
        
        spawn(command, [url], { 
          detached: true,
          stdio: 'ignore'
        }).unref();
      }
      
      console.log(`✅ Browser launched: ${url}`);
      console.log('🎯 Continuum interface should open automatically');
      
    } catch (error) {
      console.log(`⚠️ Failed to launch browser: ${error.message}`);
      console.log(`💡 Please manually open: http://localhost:${this.config.websocket.port}`);
    }
  }

  /**
   * Launch browser with DevTools debugging enabled
   */
  private async launchBrowserWithDevTools(url: string): Promise<void> {
    const { spawn } = await import('child_process');
    const { accessSync } = await import('fs');
    
    // Try different browsers in order of preference for DevTools
    const browsers = [
      {
        name: 'Opera GX',
        path: '/Applications/Opera GX.app/Contents/MacOS/Opera',
        args: ['--remote-debugging-port=9222', '--new-window', url]
      },
      {
        name: 'Google Chrome',
        path: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        args: ['--remote-debugging-port=9222', '--new-window', url]
      }
    ];

    for (const browser of browsers) {
      try {
        accessSync(browser.path);
        
        spawn(browser.path, browser.args, {
          detached: true,
          stdio: 'ignore'
        }).unref();
        
        console.log(`✅ Launched ${browser.name} with DevTools on port 9222`);
        console.log('🔌 DevTools API: http://localhost:9222/json');
        return;
        
      } catch (error) {
        console.log(`⚠️ ${browser.name} not found, trying next...`);
      }
    }

    // Fallback to system default
    const command = process.platform === 'darwin' ? 'open' : 
                   process.platform === 'win32' ? 'start' : 'xdg-open';
    
    spawn(command, [url], { 
      detached: true,
      stdio: 'ignore'
    }).unref();
    
    console.log('✅ Launched with system default browser');
  }

  /**
   * Check if Continuum service is already running and healthy
   */
  private async checkExistingService(): Promise<{exists: boolean, healthy: boolean, browserConnections: number}> {
    try {
      const { default: fetch } = await import('node-fetch');
      
      // Check if service responds on expected port
      const healthResponse = await fetch(`http://localhost:${this.config.websocket.port}/health`, {
        timeout: 2000
      });
      
      if (!healthResponse.ok) {
        return { exists: false, healthy: false, browserConnections: 0 };
      }
      
      // Check browser connection state
      const browserResponse = await fetch(`http://localhost:${this.config.websocket.port}/api/browser/state`, {
        timeout: 2000
      });
      
      if (browserResponse.ok) {
        const browserState = await browserResponse.json();
        return {
          exists: true,
          healthy: true,
          browserConnections: browserState.connectedClients?.length || 0
        };
      }
      
      return { exists: true, healthy: false, browserConnections: 0 };
      
    } catch (error) {
      // Check if port is occupied by non-Continuum process
      try {
        const net = await import('net');
        return new Promise((resolve) => {
          const server = net.createServer();
          server.listen(this.config.websocket.port, () => {
            server.close();
            resolve({ exists: false, healthy: false, browserConnections: 0 });
          });
          server.on('error', () => {
            resolve({ exists: true, healthy: false, browserConnections: 0 });
          });
        });
      } catch {
        return { exists: false, healthy: false, browserConnections: 0 };
      }
    }
  }

  /**
   * Clean up unhealthy existing service
   */
  private async cleanupExistingService(): Promise<void> {
    try {
      const { spawn } = await import('child_process');
      
      // Find and kill processes using our port
      const lsofProcess = spawn('lsof', ['-ti', `:${this.config.websocket.port}`], {
        stdio: ['ignore', 'pipe', 'ignore']
      });
      
      let pids = '';
      lsofProcess.stdout.on('data', (data) => {
        pids += data.toString();
      });
      
      lsofProcess.on('close', (code) => {
        if (code === 0 && pids.trim()) {
          const pidList = pids.trim().split('\n');
          console.log(`🧹 Cleaning up ${pidList.length} orphaned processes`);
          
          pidList.forEach(pid => {
            try {
              process.kill(parseInt(pid), 'SIGTERM');
              console.log(`🗑️ Terminated process ${pid}`);
            } catch (error) {
              console.log(`⚠️ Could not terminate process ${pid}`);
            }
          });
          
          // Give processes time to cleanup
          setTimeout(() => {
            console.log('✅ Cleanup complete');
          }, 1000);
        }
      });
      
    } catch (error) {
      console.log('⚠️ Cleanup failed, proceeding anyway:', error.message);
    }
  }

  /**
   * Log comprehensive system information
   */
  private logSystemInfo(): void {
    const health = this.systemManager.getSystemHealth();
    
    console.log('\n📊 CONTINUUM SYSTEM STATUS');
    console.log('════════════════════════════');
    console.log(`Overall Status: ${health.overall.toUpperCase()}`);
    console.log(`System Uptime: ${health.systemUptime}s`);
    console.log(`Started: ${health.startTime}`);
    console.log('\nDaemon Status:');
    
    for (const [name, daemon] of Object.entries(health.daemons)) {
      const status = daemon.status === 'running' ? '✅' : '❌';
      console.log(`  ${status} ${name}: ${daemon.status} (uptime: ${daemon.uptime}s, restarts: ${daemon.restartCount})`);
    }
    
    console.log('\n🔗 Access Points:');
    console.log(`  WebSocket Server: ws://${this.config.websocket.host}:${this.config.websocket.port}`);
    console.log(`  HTTP Interface: http://${this.config.websocket.host}:${this.config.websocket.port}`);
    console.log(`  Health Check: http://${this.config.websocket.host}:${this.config.websocket.port}/health`);
    console.log(`  System Status: http://${this.config.websocket.host}:${this.config.websocket.port}/status`);
    console.log('════════════════════════════\n');
  }
}

// CLI execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const config = {
    websocket: {
      port: process.env.CONTINUUM_PORT ? parseInt(process.env.CONTINUUM_PORT) : 9000,
      host: process.env.CONTINUUM_HOST || 'localhost'
    }
  };
  
  const system = new ContinuumSystem(config);
  
  system.start().catch(error => {
    console.error('❌ System startup failed:', error);
    process.exit(1);
  });
}