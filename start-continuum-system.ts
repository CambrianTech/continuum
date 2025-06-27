#!/usr/bin/env tsx
/**
 * Continuum System Launcher - Like Docker Desktop Service
 * Starts TypeScript daemon system with connection-triggered behavior
 * Pure TypeScript - no Python dependencies
 */

import { WebSocketDaemon } from './src/integrations/websocket/WebSocketDaemon.js';
import { RendererDaemon } from './src/daemons/renderer/RendererDaemon.js';
import { BrowserManagerDaemon } from './src/daemons/browser-manager/BrowserManagerDaemon.js';
import { AcademyDaemon } from './src/daemons/academy/AcademyDaemon.js';
import { CommandProcessorDaemon } from './src/daemons/command-processor/CommandProcessorDaemon.js';
import { spawn } from 'child_process';
import { writeFileSync, readFileSync, unlinkSync, accessSync, mkdirSync } from 'fs';
import { existsSync } from 'fs';
import { join } from 'path';

export class ContinuumSystem {
  private static readonly LOCK_FILE = '.continuum/system.lock';
  private static readonly PID_FILE = '.continuum/system.pid';
  private webSocketDaemon: WebSocketDaemon | null = null;
  private rendererDaemon: RendererDaemon | null = null;
  private browserManagerDaemon: BrowserManagerDaemon | null = null;
  private academyDaemon: AcademyDaemon | null = null;
  private commandProcessorDaemon: CommandProcessorDaemon | null = null;
  private browserProcess: any = null;
  private isRunning = false;

  /**
   * Auto-increment version on system start
   */
  private async incrementVersion(): Promise<string> {
    try {
      const { readFileSync, writeFileSync } = await import('fs');
      const packagePath = './package.json';
      const packageData = JSON.parse(readFileSync(packagePath, 'utf8'));
      
      // Parse current version (0.2.XXXX)
      const versionParts = packageData.version.split('.');
      const buildNumber = parseInt(versionParts[2]) + 1;
      const newVersion = `${versionParts[0]}.${versionParts[1]}.${buildNumber}`;
      
      // Update package.json
      packageData.version = newVersion;
      writeFileSync(packagePath, JSON.stringify(packageData, null, 2) + '\n');
      
      console.log(`🔢 Version auto-incremented: ${versionParts.join('.')} → ${newVersion}`);
      return newVersion;
    } catch (error) {
      console.warn(`⚠️ Failed to increment version: ${error.message}`);
      return '0.2.UNKNOWN';
    }
  }

  async start(): Promise<void> {
    // Auto-increment version FIRST
    const newVersion = await this.incrementVersion();
    
    // Check for existing system first
    await this.checkAndCleanupExisting();

    if (this.isRunning) {
      console.log('🔄 Continuum system already running');
      return;
    }

    console.log(`🚀 STARTING CONTINUUM OS v${newVersion} - Full TypeScript Daemon System`);
    console.log('='.repeat(60));

    // Create system lock
    this.createSystemLock();

    try {
      // Step 1: Start WebSocket daemon (core communication)
      console.log('📡 Starting WebSocket daemon...');
      this.webSocketDaemon = new WebSocketDaemon({ port: 9000 });
      await this.webSocketDaemon.start();
      console.log('✅ WebSocket daemon ready on ws://localhost:9000');

      // Step 2: Start Renderer daemon (UI management)
      console.log('🎨 Starting Renderer daemon...');
      this.rendererDaemon = new RendererDaemon();
      await this.rendererDaemon.start();
      await this.webSocketDaemon.registerExternalDaemon('renderer', this.rendererDaemon);
      console.log('✅ Renderer daemon registered');

      // Step 3: Start Browser Manager daemon (browser orchestration)
      console.log('🌐 Starting Browser Manager daemon...');
      this.browserManagerDaemon = new BrowserManagerDaemon();
      await this.browserManagerDaemon.start();
      await this.webSocketDaemon.registerExternalDaemon('browser-manager', this.browserManagerDaemon);
      console.log('✅ Browser Manager daemon registered');

      // Step 4: Start Academy daemon (Academy functionality)
      console.log('🎓 Starting Academy daemon...');
      this.academyDaemon = new AcademyDaemon();
      await this.academyDaemon.start();
      await this.webSocketDaemon.registerExternalDaemon('academy', this.academyDaemon);
      console.log('✅ Academy daemon registered');

      // Step 5: Start Command Processor daemon (Chat/Command handling)
      console.log('💬 Starting Command Processor daemon...');
      this.commandProcessorDaemon = new CommandProcessorDaemon();
      await this.commandProcessorDaemon.start();
      
      // Inject WebSocket daemon reference for room access
      (this.commandProcessorDaemon as any).webSocketDaemon = this.webSocketDaemon;
      (global as any).continuumWebSocketDaemon = this.webSocketDaemon;
      
      await this.webSocketDaemon.registerExternalDaemon('command-processor', this.commandProcessorDaemon);
      console.log('✅ Command Processor daemon registered');

      // Step 6: Pre-create basic chat rooms
      console.log('🏠 Creating basic chat rooms...');
      await this.createBasicRooms();

      // Step 7: Ensure browser tab is always open (PRIMARY INTERFACE)
      console.log('🚀 Ensuring browser interface is available...');
      await this.ensureBrowserInterface();

      // Step 8: Set up self-healing
      this.setupSelfHealing();

      this.isRunning = true;
      console.log('✅ CONTINUUM OS READY');
      console.log('   🌐 Browser: http://localhost:9000');
      console.log('   📡 WebSocket: ws://localhost:9000');
      console.log('   🎨 UI Renderer: Active');
      console.log('   🌐 Browser Manager: Active');
      console.log('   🎓 Academy: Active');
      console.log('   💬 Command Processor: Active');
      console.log('   🔧 Self-healing: Active');

    } catch (error) {
      console.error('❌ Failed to start Continuum system:', error);
      await this.stop();
      throw error;
    }
  }

  async stop(): Promise<void> {
    console.log('🛑 Stopping Continuum system...');

    try {
      if (this.browserProcess) {
        this.browserProcess.kill();
        console.log('✅ Browser process stopped');
      }

      if (this.academyDaemon) {
        await this.academyDaemon.stop();
        console.log('✅ Academy daemon stopped');
      }

      if (this.browserManagerDaemon) {
        await this.browserManagerDaemon.stop();
        console.log('✅ Browser Manager daemon stopped');
      }

      if (this.rendererDaemon) {
        await this.rendererDaemon.stop();
        console.log('✅ Renderer daemon stopped');
      }

      if (this.webSocketDaemon) {
        await this.webSocketDaemon.stop();
        console.log('✅ WebSocket daemon stopped');
      }

      // Remove system lock
      this.removeSystemLock();

      this.isRunning = false;
      console.log('✅ Continuum system stopped');

    } catch (error) {
      console.error('⚠️ Error during shutdown:', error.message);
    }
  }

  private async ensureBrowserInterface(): Promise<void> {
    // Browser is THE primary interface - always ensure it's open
    console.log('🌐 Browser is the primary Continuum interface');
    
    if (await this.isTabAlreadyOpen()) {
      console.log('✅ Browser interface already active');
      return;
    }

    console.log('🚀 Launching primary browser interface...');
    await this.launchBrowserTab();
  }

  private async launchBrowserTab(): Promise<void> {
    try {
      // First check if localhost:9000 is already open in a tab
      const existingTab = await this.findExistingContinuumTab();
      
      if (existingTab) {
        console.log('✅ Found existing Continuum tab, bringing to front...');
        await this.activateExistingTab(existingTab);
        console.log('🌐 Continuum tab is now active');
        console.log(`🔌 DevTools available on port ${existingTab.devToolsPort}`);
        return;
      }

      // No existing tab, check if we need DevTools enabled
      const needsDevTools = this.shouldEnableDevTools();
      
      if (needsDevTools) {
        console.log('🔧 AI/Portal mode: Launching browser with DevTools...');
        await this.launchBrowserWithDevTools();
      } else {
        console.log('🚀 Opening new tab: http://localhost:9000');
        this.browserProcess = spawn('open', ['http://localhost:9000'], {
          detached: true,
          stdio: 'ignore'
        });
      }

      // Small delay to let the tab open
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      console.log('✅ Continuum tab ready');
      console.log('🌐 Interface: http://localhost:9000');
      if (needsDevTools) {
        console.log('🔌 DevTools: http://localhost:9222');
      }
      
    } catch (error) {
      console.log(`⚠️ Failed to launch browser: ${error.message}`);
      console.log('💡 Please manually navigate to: http://localhost:9000');
    }
  }

  private async isTabAlreadyOpen(): Promise<boolean> {
    // PRIMARY: Check WebSocket connections first (most reliable)
    if (this.webSocketDaemon) {
      const status = this.webSocketDaemon.getSystemStatus();
      if (status.connections && status.connections.totalClients > 0) {
        console.log(`🔍 Browser already connected via WebSocket (${status.connections.totalClients} clients)`);
        return true;
      }
    }

    // FALLBACK: Try DevTools API if WebSocket shows no connections
    try {
      const { default: fetch } = await import('node-fetch');
      const response = await fetch('http://localhost:9222/json', { timeout: 2000 });
      
      if (response.ok) {
        const tabs = await response.json() as any[];
        const continuumTab = tabs.find(tab => 
          tab.url && tab.url.includes('localhost:9000')
        );
        
        if (continuumTab) {
          console.log('🔍 Found existing Continuum tab via DevTools');
          return true;
        }
      }
    } catch {
      // DevTools not available - this is normal
    }

    return false;
  }

  private async waitForBrowserConnection(): Promise<void> {
    console.log('⏳ Waiting for browser connection...');
    
    for (let attempt = 0; attempt < 15; attempt++) {
      try {
        // Check if DevTools port is responding
        const { default: fetch } = await import('node-fetch');
        const response = await fetch('http://localhost:9222/json', { timeout: 2000 });
        
        if (response.ok) {
          const tabs = await response.json() as any[];
          const continuumTab = tabs.find(tab => 
            tab.url && tab.url.includes('localhost:9000')
          );
          
          if (continuumTab) {
            console.log('✅ Browser connected with Continuum tab');
            return;
          }
        }
      } catch {
        // Keep trying
      }

      // Also check WebSocket connections
      if (this.webSocketDaemon) {
        const status = this.webSocketDaemon.getSystemStatus();
        if (status.connections && status.connections.totalClients > 0) {
          console.log('✅ Browser connected via WebSocket');
          return;
        }
      }

      await new Promise(resolve => setTimeout(resolve, 1000));
    }

    console.log('⚠️ Browser connection timeout - tab may still be loading');
  }

  private detectBrowser(): { name: string; path: string; args: string[] } | null {
    const browsers = [
      {
        name: 'Opera GX',
        path: '/Applications/Opera GX.app/Contents/MacOS/Opera',
        args: ['--remote-debugging-port=9222'] // Use existing window, enable DevTools
      },
      {
        name: 'Chrome',
        path: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        args: ['--remote-debugging-port=9222'] // Use existing window, enable DevTools
      },
      {
        name: 'Firefox',
        path: '/Applications/Firefox.app/Contents/MacOS/firefox',
        args: []
      },
      {
        name: 'Safari',
        path: '/Applications/Safari.app/Contents/MacOS/Safari',
        args: []
      }
    ];

    for (const browser of browsers) {
      try {
        accessSync(browser.path);
        console.log(`✅ Browser detected: ${browser.name}`);
        return browser;
      } catch (error) {
        console.log(`❌ Browser not found: ${browser.name} (${error.code})`);
        continue;
      }
    }

    console.log('❌ No browsers found in standard macOS locations');
    return null;
  }

  private async findExistingContinuumTab(): Promise<any> {
    try {
      // Try to find existing tab via DevTools Protocol
      const { default: fetch } = await import('node-fetch');
      
      // Check common DevTools ports
      for (const port of [9222, 9223]) {
        try {
          const response = await fetch(`http://localhost:${port}/json`, { timeout: 2000 });
          if (response.ok) {
            const tabs = await response.json() as any[];
            const continuumTab = tabs.find(tab => 
              tab.url && (
                tab.url.includes('localhost:9000') || 
                tab.url.includes('127.0.0.1:9000')
              )
            );
            
            if (continuumTab) {
              console.log(`🔍 Found existing tab: ${continuumTab.title || 'Continuum'}`);
              return { ...continuumTab, devToolsPort: port };
            }
          }
        } catch {
          // This port doesn't have DevTools or browser not running
        }
      }
    } catch {
      // DevTools Protocol not available
    }
    
    return null;
  }

  private async activateExistingTab(tab: any): Promise<void> {
    try {
      // Activate the tab using DevTools Protocol
      const { default: fetch } = await import('node-fetch');
      const activateUrl = `http://localhost:${tab.devToolsPort}/json/activate/${tab.id}`;
      
      await fetch(activateUrl, { method: 'POST', timeout: 2000 });
      console.log('✅ Tab activated successfully');
      
    } catch (error) {
      console.log('⚠️ Could not activate tab via DevTools, using fallback...');
      
      // Fallback: just open the URL (will create new tab or focus existing)
      spawn('open', ['http://localhost:9000'], {
        detached: true,
        stdio: 'ignore'
      });
    }
  }

  private shouldEnableDevTools(): boolean {
    // Check if this was started with --devtools flag
    const args = process.argv.slice(2);
    return args.includes('--devtools');
  }

  private async launchBrowserWithDevTools(): Promise<void> {
    const browserCommand = this.detectBrowser();
    
    if (!browserCommand) {
      // Fallback to open command
      console.log('🚀 Opening browser with DevTools fallback...');
      this.browserProcess = spawn('open', ['http://localhost:9000'], {
        detached: true,
        stdio: 'ignore'
      });
      return;
    }

    console.log(`🚀 Launching ${browserCommand.name} with DevTools...`);
    this.browserProcess = spawn(browserCommand.path, [
      ...browserCommand.args,
      'http://localhost:9000'
    ], {
      detached: true,
      stdio: 'ignore'
    });
  }

  private setupSelfHealing(): void {
    // Monitor WebSocket health and browser connection
    setInterval(async () => {
      try {
        // Check WebSocket daemon health
        const wsStatus = this.webSocketDaemon?.getSystemStatus();
        if (!this.webSocketDaemon || !wsStatus || wsStatus.status === 'error') {
          console.log('🔧 Self-healing: WebSocket daemon unhealthy, restarting...');
          await this.restartWebSocket();
        }

        // DISABLED: Browser tab auto-launch to prevent tab proliferation
        // The tab detection logic needs fixing before re-enabling
        const hasActiveClients = wsStatus?.connections?.totalClients > 0;
        
        if (hasActiveClients) {
          // Browser is connected, no action needed
        } else {
          // TODO: Fix tab detection logic before re-enabling auto-launch
          console.log('🔧 Self-healing: Browser not connected (auto-launch disabled)');
        }

      } catch (error) {
        console.error('❌ Self-healing error:', error.message);
      }
    }, 30000); // Check every 30 seconds

    console.log('🔧 Self-healing monitoring active (WebSocket + Browser)');
  }

  private async restartWebSocket(): Promise<void> {
    if (this.webSocketDaemon) {
      await this.webSocketDaemon.stop();
    }
    
    this.webSocketDaemon = new WebSocketDaemon({ port: 9000 });
    await this.webSocketDaemon.start();
    
    // Re-register daemons
    if (this.rendererDaemon) {
      await this.webSocketDaemon.registerExternalDaemon('renderer', this.rendererDaemon);
    }
    if (this.browserManagerDaemon) {
      await this.webSocketDaemon.registerExternalDaemon('browser-manager', this.browserManagerDaemon);
    }
    
    console.log('✅ Self-healing: WebSocket daemon restarted');
  }

  getSystemStatus(): object {
    return {
      running: this.isRunning,
      webSocket: this.webSocketDaemon?.getSystemStatus() || null,
      renderer: this.rendererDaemon?.getSimpleStatus() || null,
      browserManager: this.browserManagerDaemon?.getSimpleStatus() || null,
      browser: this.browserProcess ? 'running' : 'stopped'
    };
  }

  /**
   * Create basic chat rooms from configuration
   */
  private async createBasicRooms(): Promise<void> {
    try {
      const { readFileSync } = await import('fs');
      const { join } = await import('path');
      
      const roomsConfigPath = join(process.cwd(), 'config/rooms.json');
      const roomsConfig = JSON.parse(readFileSync(roomsConfigPath, 'utf8'));
      
      console.log(`🏠 Loading ${roomsConfig.preloadedRooms.length} preloaded rooms from config`);
      
      // Initialize room storage in WebSocket daemon
      if (this.webSocketDaemon) {
        const roomStorage = new Map();
        
        for (const roomConfig of roomsConfig.preloadedRooms) {
          const room = {
            ...roomConfig,
            participants: [...roomConfig.defaultParticipants],
            agents: [...roomConfig.defaultAgents],
            messages: [],
            createdAt: new Date().toISOString(),
            lastActivity: new Date().toISOString(),
            activeUsers: 0
          };
          
          roomStorage.set(roomConfig.id, room);
          console.log(`📄 Created room: ${roomConfig.id} (${roomConfig.name})`);
        }
        
        // Store room data in WebSocket daemon for global access
        (this.webSocketDaemon as any).chatRooms = roomStorage;
        (this.webSocketDaemon as any).roomsConfig = roomsConfig;
        
        console.log(`✅ Initialized ${roomStorage.size} chat rooms`);
      }
      
    } catch (error) {
      console.warn(`⚠️ Failed to load rooms config: ${error.message}`);
      console.log('🏠 Creating default rooms manually...');
      
      // Fallback: create basic rooms manually
      if (this.webSocketDaemon) {
        const roomStorage = new Map();
        
        roomStorage.set('general', {
          id: 'general',
          name: 'General Chat',
          type: 'public',
          persistent: true,
          participants: ['system'],
          agents: [],
          messages: [],
          createdAt: new Date().toISOString(),
          lastActivity: new Date().toISOString()
        });
        
        roomStorage.set('academy', {
          id: 'academy', 
          name: 'Academy Training',
          type: 'public',
          persistent: true,
          participants: ['system'],
          agents: ['AcademyAI'],
          messages: [],
          createdAt: new Date().toISOString(),
          lastActivity: new Date().toISOString()
        });
        
        (this.webSocketDaemon as any).chatRooms = roomStorage;
        console.log('✅ Created fallback rooms: general, academy');
      }
    }
  }

  /**
   * SINGLETON PROTECTION - Only one Continuum system per machine
   */
  private async checkAndCleanupExisting(): Promise<void> {
    console.log('🔍 Checking for existing Continuum system...');

    // Check for lock file
    if (existsSync(ContinuumSystem.LOCK_FILE)) {
      try {
        const lockData = JSON.parse(readFileSync(ContinuumSystem.LOCK_FILE, 'utf8'));
        const pid = lockData.pid;

        // Check if process is still running
        try {
          process.kill(pid, 0); // Signal 0 checks if process exists
          console.log(`🚨 Existing Continuum system found (PID: ${pid})`);
          console.log('🔄 Please stop the existing system first or use:');
          console.log(`   kill ${pid}`);
          process.exit(1);
        } catch {
          // Process doesn't exist, clean up stale lock
          console.log('🧹 Cleaning up stale lock file...');
          this.removeSystemLock();
        }
      } catch {
        // Invalid lock file, remove it
        console.log('🧹 Removing invalid lock file...');
        this.removeSystemLock();
      }
    }

    // Kill any orphaned processes
    await this.killOrphanedProcesses();
  }

  private createSystemLock(): void {
    try {
      // Ensure .continuum directory exists
      const continuumDir = '.continuum';
      if (!existsSync(continuumDir)) {
        mkdirSync(continuumDir, { recursive: true });
      }

      const lockData = {
        pid: process.pid,
        startTime: new Date().toISOString(),
        version: '1.0.0'
      };

      writeFileSync(ContinuumSystem.LOCK_FILE, JSON.stringify(lockData, null, 2));
      writeFileSync(ContinuumSystem.PID_FILE, process.pid.toString());
      
      console.log(`🔒 System lock created (PID: ${process.pid})`);
    } catch (error) {
      console.error('⚠️ Failed to create system lock:', error.message);
    }
  }

  private removeSystemLock(): void {
    try {
      if (existsSync(ContinuumSystem.LOCK_FILE)) {
        unlinkSync(ContinuumSystem.LOCK_FILE);
      }
      if (existsSync(ContinuumSystem.PID_FILE)) {
        unlinkSync(ContinuumSystem.PID_FILE);
      }
      console.log('🔓 System lock removed');
    } catch (error) {
      console.error('⚠️ Failed to remove system lock:', error.message);
    }
  }

  private async killOrphanedProcesses(): Promise<void> {
    console.log('🧹 Cleaning up orphaned processes...');
    
    const processesToKill = [
      'WebSocketDaemon.ts',
      'RendererDaemon.ts', 
      'BrowserManagerDaemon.ts',
      'CommandProcessorDaemon.ts'
    ];

    for (const processName of processesToKill) {
      try {
        const killProcess = spawn('pkill', ['-f', processName]);
        await new Promise(resolve => {
          killProcess.on('close', resolve);
          setTimeout(resolve, 2000); // Timeout after 2 seconds
        });
      } catch {
        // Ignore errors - process might not exist
      }
    }

    // Small delay to let processes die
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log('✅ Orphaned process cleanup completed');
  }
}

// CLI entry point with connection-triggered behavior
if (import.meta.url === `file://${process.argv[1]}`) {
  // Parse command line arguments
  const args = process.argv.slice(2);
  const devToolsMode = args.includes('--devtools');
  
  console.log('🌐 Continuum Service - Like Docker Desktop for AI collaboration');
  if (devToolsMode) {
    console.log('🔧 Enhanced mode: Full DevTools + AI monitoring capabilities');
  }
  console.log('');
  
  const system = new ContinuumSystem();

  // Graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await system.stop();
    process.exit(0);
  });

  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await system.stop();
    process.exit(0);
  });

  // Start system
  system.start()
    .then(() => {
      console.log('\n⏰ System running... (Press Ctrl+C to stop)');
      // Keep process alive
      setInterval(() => {
        const status = system.getSystemStatus();
        if (!status.running) {
          console.log('❌ System unhealthy, exiting...');
          process.exit(1);
        }
      }, 60000); // Health check every minute
    })
    .catch((error) => {
      console.error('💥 System startup failed:', error);
      process.exit(1);
    });
}

export default ContinuumSystem;