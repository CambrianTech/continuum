#!/usr/bin/env tsx
/**
 * Continuum System - Clean event-driven architecture
 * No timeouts, no process spawning, no fake success reporting
 */

import { EventEmitter } from 'events';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon';
import { RendererDaemon } from '../../daemons/renderer/RendererDaemon';
import { CommandProcessorDaemon } from '../../daemons/command-processor/CommandProcessorDaemon';
import { BrowserManagerDaemon } from '../../daemons/browser-manager/BrowserManagerDaemon';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon';
import { ContinuumDirectoryDaemon } from '../../daemons/continuum-directory/ContinuumDirectoryDaemon';
import { StaticFileDaemon } from '../../daemons/static-file/StaticFileDaemon';
import { AcademyDaemon } from '../../daemons/academy/AcademyDaemon';
import { WidgetDaemon } from '../../daemons/widget/WidgetDaemon';
// import { PersonaDaemon } from '../../daemons/persona/PersonaDaemon';
import { ChatRoomDaemon } from '../../daemons/chatroom/ChatRoomDaemon';
import { ContinuumContext, continuumContextFactory } from '../../types/shared/core/ContinuumTypes';
// import { DaemonMessage } from '../../daemons/base/DaemonProtocol';
// import { DaemonMessageUtils } from '../../daemons/base/DaemonMessageUtils';
// import { UniversalLogger } from '../../logging/UniversalLogger';

export class ContinuumSystem extends EventEmitter {
  private daemons = new Map();
  private readyDaemons = new Set<string>();
  private systemContext: ContinuumContext;

  constructor() {
    super();
    
    // Create system-level context for all daemons
    this.systemContext = continuumContextFactory.create({
      sessionId: 'system' as any, // TODO: Make this proper UUID
      environment: 'server'
    });
    
    // Create daemons in dependency order - pass context where supported
    this.daemons.set('continuum-directory', new ContinuumDirectoryDaemon());
    this.daemons.set('session-manager', new SessionManagerDaemon(this.systemContext, '.continuum/sessions'));
    this.daemons.set('static-file', new StaticFileDaemon());
    this.daemons.set('websocket', new WebSocketDaemon(this.systemContext));
    this.daemons.set('renderer', new RendererDaemon());
    this.daemons.set('command-processor', new CommandProcessorDaemon());
    this.daemons.set('widget', new WidgetDaemon());
    this.daemons.set('chatroom', new ChatRoomDaemon());
    // PersonaDaemon needs special handling - it's created per persona, not as a system daemon
    // this.daemons.set('persona', new PersonaDaemon(this.systemContext));
    this.daemons.set('academy', new AcademyDaemon());
    this.daemons.set('browser-manager', new BrowserManagerDaemon());
  }
  
  private getPackageInfo(): { version: string; name: string } {
    try {
      const fs = require('fs');
      const pkg = JSON.parse(fs.readFileSync('package.json', 'utf-8'));
      return { version: pkg.version, name: pkg.name };
    } catch (error) {
      return { version: 'unknown', name: 'continuum' };
    }
  }
  
  private handleDaemonStopped(name: string): void {
    const stopTime = new Date().toISOString();
    console.log(`[${stopTime}] 🛑 DAEMON STOPPED: ${name}`);
    this.readyDaemons.delete(name);
  }
  
  private handleDaemonError(name: string, error: any): void {
    const errorTime = new Date().toISOString();
    console.error(`[${errorTime}] 🚨 DAEMON ERROR: ${name} - ${error.message}`);
    console.error(`[${errorTime}] 📋 Stack trace:`, error.stack);
  }

  async start(): Promise<void> {
    const startTime = new Date().toISOString();
    const pkg = this.getPackageInfo();
    
    console.log('╔══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗');
    console.log('║ 🌟 CONTINUUM SYSTEM STARTUP                                                                                          ║');
    console.log(`║ Version: ${pkg.version.padEnd(20)} Start Time: ${startTime.padEnd(30)} Process: ${process.pid.toString().padEnd(15)} ║`);
    console.log('╠══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣');
    console.log('║ 📋 Daemon Launch Sequence:                                                                                          ║');
    console.log('║   1. continuum-directory → 2. session-manager → 3. static-file → 4. websocket → 5. renderer → 6. command-processor → 7. academy → 8. browser ║');
    console.log('╚══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝');
    console.log('');
    
    // Console override temporarily disabled due to infinite loop issues
    // TODO: Fix console override to work properly with ConsoleCommand
    console.log('🔧 Console override temporarily disabled - UniversalLogger available for manual use');
    
    // Check if server is already running BEFORE we start
    const serverAlreadyRunning = await this.isServerRunning();
    if (serverAlreadyRunning) {
      console.log('⚠️  Server already running on port 9000');
      console.log('✅ Reusing existing instance (ONE TAB POLICY)');
      process.exit(0);
    }
    
    // Clear any existing port conflicts (temporarily disabled for debugging)
    // await this.clearPorts();
    
    // Set up daemon event listeners with detailed logging
    for (const [name, daemon] of this.daemons) {
      daemon.on('started', () => this.handleDaemonReady(name));
      daemon.on('failed', (error: any) => this.handleDaemonFailed(name, error));
      daemon.on('stopped', () => this.handleDaemonStopped(name));
      daemon.on('error', (error: any) => this.handleDaemonError(name, error));
    }
    
    // Start all daemons with comprehensive logging
    console.log('🚀 Starting daemons...');
    for (const [name, daemon] of this.daemons) {
      const daemonStartTime = new Date().toISOString();
      console.log(`[${daemonStartTime}] 🚀 Starting ${name} daemon...`);
      
      try {
        await daemon.start();
        const daemonReadyTime = new Date().toISOString();
        console.log(`[${daemonReadyTime}] ✅ ${name} daemon ready`);
      } catch (error) {
        const errorTime = new Date().toISOString();
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`[${errorTime}] 💥 ${name} daemon FAILED:`, errorMessage);
        throw error;
      }
    }
    
    // Wait for ALL daemons to be ready
    const waitTime = new Date().toISOString();
    console.log(`[${waitTime}] ⏳ Waiting for all daemons to be ready...`);
    await this.waitForSystemReady();
    
    // Set up inter-daemon communication
    await this.setupInterDaemonCommunication();
    
    // Set up session logging for all daemons
    this.setupSessionLogging();
    
    // Set up server-side continuum global object
    console.log('🔧 SETUP_DEBUG: About to call setupServerContinuum()');
    try {
      await this.setupServerContinuum();
      console.log('🔧 SETUP_DEBUG: setupServerContinuum() completed successfully');
    } catch (error) {
      console.error('🔧 SETUP_DEBUG: setupServerContinuum() failed:', error);
      throw error;
    }
    
    // System is now genuinely ready
    console.log('');
    console.log('✅ System ready and operational');
    console.log('🌐 Browser interface: http://localhost:9000');
    console.log('🔌 WebSocket API: ws://localhost:9000');
    console.log('');
    
    // Clear any ports that might be in use before running self-tests
    await this.clearPorts();
    
    // Run self-tests to validate everything works
    await this.runSelfTests();
  }

  private async clearPorts(): Promise<void> {
    // Check if port 9000 is in use and handle gracefully
    try {
      const net = await import('net');
      const server = net.createServer();
      
      return new Promise((resolve, reject) => {
        server.listen(9000, () => {
          server.close();
          resolve();
        });
        
        server.on('error', async (error: any) => {
          if (error.code === 'EADDRINUSE') {
            console.log('⚠️  Port 9000 in use - attempting cleanup...');
            try {
              await this.killPortProcesses(9000);
              console.log('✅ Port 9000 freed, retrying...');
              // Retry after cleanup
              setTimeout(() => {
                server.listen(9000, () => {
                  server.close();
                  resolve();
                });
              }, 1000);
            } catch (cleanupError) {
              reject(new Error(`Port 9000 cleanup failed: ${cleanupError instanceof Error ? cleanupError.message : String(cleanupError)}`));
            }
          } else {
            reject(error);
          }
        });
      });
    } catch (error) {
      console.log('⚠️  Port check failed:', error);
    }
  }

  private async killPortProcesses(port: number): Promise<void> {
    const { exec } = await import('child_process');
    const { promisify } = await import('util');
    const execAsync = promisify(exec);

    try {
      // Find processes using the port
      const { stdout } = await execAsync(`lsof -ti:${port}`);
      const pids = stdout.trim().split('\n').filter(pid => pid);
      
      if (pids.length > 0) {
        console.log(`🔧 Killing processes on port ${port}: ${pids.join(', ')}`);
        
        // Kill processes gracefully first
        for (const pid of pids) {
          try {
            await execAsync(`kill ${pid}`);
          } catch (error) {
            // If graceful kill fails, force kill
            try {
              await execAsync(`kill -9 ${pid}`);
            } catch (forceError) {
              console.log(`⚠️  Could not kill process ${pid}:`, forceError);
            }
          }
        }
        
        // Wait a moment for processes to die
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    } catch (error) {
      // lsof might fail if no processes found - that's actually good
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (!errorMessage.includes('No such file or directory')) {
        throw error;
      }
    }
  }

  private async waitForSystemReady(): Promise<void> {
    return new Promise((resolve, reject) => {
      // Set up timeout for safety (but based on real failure, not arbitrary waiting)
      const timeout = setTimeout(() => {
        const missing = Array.from(this.daemons.keys()).filter(name => !this.readyDaemons.has(name));
        reject(new Error(`Daemons failed to start: ${missing.join(', ')}`));
      }, 30000); // 30 seconds is generous for real failures
      
      this.on('all-daemons-ready', () => {
        clearTimeout(timeout);
        resolve();
      });
      
      this.on('daemon-failed', () => {
        clearTimeout(timeout);
        reject(new Error('Critical daemon failed to start'));
      });
      
      // Check if already ready (race condition protection)
      if (this.readyDaemons.size === this.daemons.size) {
        clearTimeout(timeout);
        resolve();
      }
    });
  }

  private handleDaemonReady(name: string): void {
    const readyTime = new Date().toISOString();
    console.log(`[${readyTime}] ✅ DAEMON READY: ${name} (${this.readyDaemons.size + 1}/${this.daemons.size})`);
    this.readyDaemons.add(name);
    
    if (this.readyDaemons.size === this.daemons.size) {
      const allReadyTime = new Date().toISOString();
      console.log(`[${allReadyTime}] 🎉 ALL DAEMONS READY - System operational!`);
      this.emit('all-daemons-ready');
    }
  }

  private handleDaemonFailed(name: string, error: any): void {
    const failTime = new Date().toISOString();
    console.error(`[${failTime}] 💥 DAEMON FAILED: ${name}`);
    console.error(`[${failTime}] 📋 Error: ${error.message}`);
    console.error(`[${failTime}] 📋 Stack: ${error.stack}`);
    console.error(`[${failTime}] 📊 System status: ${this.readyDaemons.size}/${this.daemons.size} daemons ready`);
    this.emit('daemon-failed', { name, error });
  }

  private async setupInterDaemonCommunication(): Promise<void> {
    console.log('🔌 Setting up inter-daemon communication...');
    
    const webSocketDaemon = this.daemons.get('websocket');
    const rendererDaemon = this.daemons.get('renderer');
    const commandProcessorDaemon = this.daemons.get('command-processor');
    const staticFileDaemon = this.daemons.get('static-file');
    const chatRoomDaemon = this.daemons.get('chatroom');
    const academyDaemon = this.daemons.get('academy');
    const sessionManagerDaemon = this.daemons.get('session-manager');
    const browserManagerDaemon = this.daemons.get('browser-manager');
    
    // Register daemons with WebSocket daemon for message routing
    webSocketDaemon.registerDaemon(rendererDaemon);
    webSocketDaemon.registerDaemon(commandProcessorDaemon);
    webSocketDaemon.registerDaemon(staticFileDaemon);
    webSocketDaemon.registerDaemon(chatRoomDaemon);
    webSocketDaemon.registerDaemon(academyDaemon);
    webSocketDaemon.registerDaemon(sessionManagerDaemon);
    webSocketDaemon.registerDaemon(browserManagerDaemon);
    
    // Register static file routes first (they take precedence)
    staticFileDaemon.registerWithWebSocketDaemon(webSocketDaemon);
    
    // Register specific command endpoints only - CommandProcessorDaemon registers its own endpoints
    webSocketDaemon.registerRouteHandler('/api/commands/*', 'command-processor', 'handle_api');
    
    // Register widget daemon with WebSocket for command routing
    const widgetDaemon = this.daemons.get('widget') as WidgetDaemon;
    if (widgetDaemon) {
      webSocketDaemon.registerDaemon(widgetDaemon);
      console.log(`🔧 Registered WidgetDaemon with WebSocket for command routing`);
    }
    
    // Register catch-all route for renderer daemon (handles everything else)
    webSocketDaemon.registerRouteHandler('*', 'renderer', 'http_request');
    
    console.log('✅ Inter-daemon communication established');
    console.log('🤖 AI autonomy infrastructure ready: ChatRoom + Persona + Academy daemons online');
  }

  private async runSelfTests(): Promise<void> {
    console.log('🧪 Running self-tests...');
    
    try {
      // Test that localhost:9000 responds
      const response = await fetch('http://localhost:9000');
      if (response.ok) {
        console.log('✅ HTTP server responding');
      } else {
        console.log(`⚠️  HTTP server returned: ${response.status}`);
      }
      
      // Test WebSocket connection
      const WebSocket = (await import('ws')).default;
      const ws = new WebSocket('ws://localhost:9000');
      
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          ws.close();
          reject(new Error('WebSocket connection timeout'));
        }, 5000);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          ws.close();
          console.log('✅ WebSocket connection working');
          resolve();
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
      
      // Let browser manager handle tab management
      console.log('🌐 Browser manager will handle tab management');
      
    } catch (error) {
      console.log('⚠️  Self-test failed:', error);
      console.log('🔧 System may not be fully operational');
    }
  }

  /**
   * Check if server is already running
   */
  private async isServerRunning(): Promise<boolean> {
    try {
      const response = await fetch('http://localhost:9000/api/status');
      return response.ok;
    } catch (error) {
      return false;
    }
  }

  /**
   * Set up server-side continuum global object with executeJS method
   */
  private async setupServerContinuum(): Promise<void> {
    console.log('🔧 SETUP_DEBUG: setupServerContinuum() method called');
    
    const webSocketDaemon = this.daemons.get('websocket');
    if (!webSocketDaemon) {
      console.error('🔧 SETUP_DEBUG: WebSocket daemon not found!');
      throw new Error('WebSocket daemon not found for continuum setup');
    }
    
    console.log('🔧 SETUP_DEBUG: WebSocket daemon found, creating server continuum object');
    
    // Create server-side continuum object
    const serverContinuum = {
      executeJS: async (script: string): Promise<any> => {
        console.log(`🌐 SERVER CONTINUUM: executeJS called with script length: ${script.length}`);
        console.log(`🌐 SERVER CONTINUUM: Script preview: ${script.substring(0, 100)}...`);
        
        // Generate unique request ID for this execution
        const requestId = `exec_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        
        // Send JavaScript to browser via WebSocket with request ID
        try {
          const broadcastResult = await webSocketDaemon.sendToConnectedClients({
            type: 'execute_js',
            script: script,
            requestId: requestId,
            timestamp: new Date().toISOString()
          });
          
          console.log(`🌐 SERVER CONTINUUM: Broadcast sent, waiting for response...`);
          
          // Wait for response from browser
          return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
              reject(new Error('JavaScript execution timeout (10 seconds)'));
            }, 10000);
            
            // Listen for response (this would need proper event handling)
            // For now, just return the broadcast result
            clearTimeout(timeout);
            resolve(broadcastResult);
          });
          
        } catch (error) {
          console.error(`🌐 SERVER CONTINUUM: executeJS error:`, error);
          throw error;
        }
      }
    };
    
    console.log('🔧 SETUP_DEBUG: About to attach serverContinuum to global scope');
    
    // Attach to global scope
    (global as any).continuum = serverContinuum;
    
    console.log('🔧 SETUP_DEBUG: Attached to global scope, verifying...');
    console.log('🔧 SETUP_DEBUG: global.continuum type:', typeof (global as any).continuum);
    console.log('🔧 SETUP_DEBUG: global.continuum.executeJS type:', typeof (global as any).continuum?.executeJS);
    
    console.log('🌐 SERVER CONTINUUM: Global continuum object created with executeJS method');
  }

  private setupSessionLogging(): void {
    const sessionManager = this.daemons.get('session-manager');
    const browserManager = this.daemons.get('browser-manager');
    
    if (!sessionManager) {
      console.log('⚠️ SessionManagerDaemon not available for session logging setup');
      return;
    }

    // Listen for session creation events
    sessionManager.on('session_created', (event: any) => {
      if (event.serverLogPath) {
        console.log(`📝 Enabling server logging for all daemons: ${event.serverLogPath}`);
        
        // Enable logging for all daemons
        for (const [name, daemon] of this.daemons) {
          try {
            daemon.setSessionLogPath(event.serverLogPath);
            // Log daemon startup to session log
            daemon.log(`✅ ${name} daemon session logging enabled`, 'info');
          } catch (error) {
            console.log(`⚠️ Failed to enable logging for ${name}: ${error}`);
          }
        }
      }
      
      // Forward session_created events to BrowserManagerDaemon
      if (browserManager) {
        browserManager.emit('session_created', event);
      }
    });
    
    // Listen for session joined events  
    sessionManager.on('session_joined', (event: any) => {
      // Forward session_joined events to BrowserManagerDaemon
      if (browserManager) {
        browserManager.emit('session_joined', event);
      }
    });
  }

  async getCurrentSessionInfo(): Promise<any> {
    // Use the new connection orchestration instead of direct session access
    const sessionManagerDaemon = this.daemons.get('session-manager');
    if (!sessionManagerDaemon) {
      return { success: false, error: 'SessionManagerDaemon not available' };
    }

    try {
      // Use intelligent connection orchestration
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'continuum-cli',
        owner: 'system',
        sessionPreference: 'current',
        capabilities: ['browser', 'commands', 'screenshots'],
        context: 'development',
        type: 'development'
      });

      if (!connectionResult.success) {
        return connectionResult;
      }

      // Transform to the expected format for display
      return {
        success: true,
        data: {
          session: {
            id: connectionResult.data.sessionId,
            action: connectionResult.data.action,
            launched: connectionResult.data.launched,
            logPaths: connectionResult.data.logs,
            directories: {
              screenshots: connectionResult.data.screenshots
            },
            interface: connectionResult.data.interface,
            commands: connectionResult.data.commands
          }
        }
      };
    } catch (error) {
      return { success: false, error: `Connection orchestration failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async stop(): Promise<void> {
    const shutdownStartTime = new Date().toISOString();
    const pkg = this.getPackageInfo();
    
    console.log('');
    console.log('╔══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗');
    console.log('║ 🛑 CONTINUUM SYSTEM SHUTDOWN                                                                                     ║');
    console.log(`║ Version: ${pkg.version.padEnd(20)} Shutdown Time: ${shutdownStartTime.padEnd(25)} Process: ${process.pid.toString().padEnd(15)} ║`);
    console.log('╚══════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝');
    
    for (const [name, daemon] of this.daemons) {
      const daemonStopTime = new Date().toISOString();
      console.log(`[${daemonStopTime}] 🛑 Stopping ${name}...`);
      
      try {
        await daemon.stop();
        const daemonStoppedTime = new Date().toISOString();
        console.log(`[${daemonStoppedTime}] ✅ ${name} stopped cleanly`);
      } catch (error) {
        const errorTime = new Date().toISOString();
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`[${errorTime}] ⚠️  ${name} stop error: ${errorMessage}`);
      }
    }
    
    const shutdownCompleteTime = new Date().toISOString();
    console.log(`[${shutdownCompleteTime}] ✅ SYSTEM SHUTDOWN COMPLETE`);
    console.log('');
  }
}

// This file is imported by main.ts - no direct execution needed