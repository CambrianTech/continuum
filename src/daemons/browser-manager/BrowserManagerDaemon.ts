/**
 * Browser Manager Daemon - Clean orchestrator
 * 
 * MODULAR ARCHITECTURE:
 * - BrowserLauncher: Handles browser process launching
 * - BrowserSessionManager: Manages session lifecycle and placement
 * - Composed together for clean, testable functionality
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon.js';
import { DaemonResponse } from '../base/DaemonProtocol.js';
import { BrowserType, BrowserRequest, BrowserConfig, BrowserStatus, BrowserAction } from './types/index.js';
// BrowserFilters unused in this file
import { BrowserLauncher } from './modules/BrowserLauncher.js';
import { BrowserSessionManager } from './modules/BrowserSessionManager.js';
import { ChromeBrowserModule } from './modules/ChromeBrowserModule.js';
import { SessionConsoleLogger } from '../session-manager/modules/SessionConsoleLogger.js';
import { SimpleTabManager } from './modules/SimpleTabManager.js';
import { ZombieTabKiller } from './modules/ZombieTabKiller.js';
import { BrowserTabManager } from './modules/BrowserTabAdapter.js';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class BrowserManagerDaemon extends MessageRoutedDaemon {
  public readonly name = 'browser-manager';
  public readonly version = '2.0.0';

  private launcher = new BrowserLauncher();
  private sessionManager = new BrowserSessionManager();
  // private hasActiveBrowser = false; // TODO: Remove if not needed
  private tabManager = new SimpleTabManager();
  private zombieKiller = new ZombieTabKiller();
  private browserTabManager = new BrowserTabManager();
  
  // Track console loggers for each session
  private consoleLoggers = new Map<string, SessionConsoleLogger>();
  
  // Auto-cleanup interval
  private zombieCleanupInterval: NodeJS.Timeout | undefined = undefined;
  
  // MessageRoutedDaemon implementation
  protected readonly primaryMessageType = 'browser_request';
  
  protected getRouteMap(): MessageRouteMap {
    return {
      'create': this.createBrowser.bind(this),
      'destroy': this.destroyBrowser.bind(this),
      'list': this.listBrowsers.bind(this),
      'optimize': this.optimizeResources.bind(this)
    };
  }

  protected getAdditionalMessageHandlers(): { [messageType: string]: MessageRouteHandler } {
    return {
      'get_capabilities': this.getCapabilities.bind(this),
      'create_browser': this.handleSmartBrowserRequest.bind(this)
    };
  }

  protected async onStart(): Promise<void> {
    // Initialize browser modules
    await this.initializeBrowserModules();
    
    // Browser launching now event-driven via session creation (no more auto-spawning)
    
    this.log('Browser Manager Daemon v2.0 started - waiting for session events');
    
    // Listen for session creation events to launch browsers
    this.setupSessionEventListening();
    
    // Start continuous zombie monitoring and auto-cleanup
    this.startZombieMonitoring();
  }

  protected async onStop(): Promise<void> {
    // Stop zombie monitoring
    if (this.zombieCleanupInterval) {
      clearInterval(this.zombieCleanupInterval);
      this.zombieCleanupInterval = undefined;
      this.log('🛑 Stopped zombie monitoring');
    }
    
    // Stop all console loggers
    for (const [sessionId, logger] of this.consoleLoggers) {
      try {
        await logger.stopLogging();
        this.log(`🔌 Stopped console logging for session ${sessionId}`);
      } catch (error) {
        this.log(`⚠️ Error stopping console logger for ${sessionId}: ${error}`, 'warn');
      }
    }
    this.consoleLoggers.clear();
    
    // Cleanup managed browsers
    const browsers = this.sessionManager.getAllBrowsers();
    this.log(`Shutting down ${browsers.length} managed browsers`);
    
    // TODO: Implement graceful browser shutdown
  }
  
  /**
   * Start event-driven zombie monitoring (cleanup on session creation)
   */
  private startZombieMonitoring(): void {
    this.log('🚨 Event-driven zombie cleanup enabled (on session creation)');
  }
  
  /**
   * Detect and automatically clean up zombie browser connections
   */
  private async performZombieCleanup(): Promise<void> {
    try {
      const status = await this.tabManager.checkTabs();
      const zombieStatus = await this.zombieKiller.getTabStatus();
      
      this.log(`🔍 Browser check: ${status.count} tab(s) found - ${status.action}`);
      
      if (zombieStatus.zombies > 0) {
        this.log(`💀 Found ${zombieStatus.zombies} zombie tabs (${zombieStatus.total} total connections)`);
        this.log(`🔍 Tab details: ${JSON.stringify(zombieStatus.details)}`);
        
        // AUTO-CLEANUP: Kill zombie tabs
        const killed = await this.zombieKiller.killZombieTabs((msg) => this.log(msg));
        
        if (killed > 0) {
          this.log(`✅ Auto-cleanup complete: ${killed} zombie tabs killed`);
          
          // Give time for connections to clear
          await new Promise(resolve => setTimeout(resolve, 1000));
          
          // Verify cleanup worked
          const newStatus = await this.zombieKiller.getTabStatus();
          if (newStatus.zombies === 0) {
            this.log(`✨ Browser connections healthy - console capture should work`);
          } else {
            this.log(`⚠️ Still have ${newStatus.zombies} zombie tabs after cleanup`);
          }
        }
      } else {
        // Periodic health check passed
        this.log(`✅ Browser connections healthy (${zombieStatus.active} active, 0 zombies)`);
      }
      
    } catch (error) {
      this.log(`❌ Zombie cleanup error: ${error}`, 'error');
    }
  }
  
  /**
   * Setup session event listening to launch browsers only when sessions are created
   */
  private setupSessionEventListening(): void {
    // Listen for session_created events from session manager
    this.on('session_created', async (event: any) => {
      const { sessionId, sessionType, owner } = event;
      this.log(`📋 Session created: ${sessionId} (${sessionType}) for ${owner}`);
      
      // Only launch browser if this session needs one
      if (this.sessionNeedsBrowser(sessionType)) {
        await this.launchBrowserForSession(sessionId, sessionType, owner);
      }
    });
    
    this.log('👂 Listening for session_created events to launch browsers');
  }
  
  /**
   * Check if a session type needs a browser
   */
  private sessionNeedsBrowser(sessionType: string): boolean {
    // Portal and development sessions need browsers
    // Validation and persona sessions might not
    return ['portal', 'development', 'user'].includes(sessionType);
  }
  
  /**
   * Launch browser specifically for a session
   */
  private async launchBrowserForSession(sessionId: string, sessionType: string, _owner: string): Promise<void> {
    try {
      this.log(`🚀 Launching browser for session ${sessionId} (${sessionType})`);
      
      // Clean up any zombie tabs BEFORE launching new browser
      await this.performZombieCleanup();
      
      // TODO: Use actual browser launching logic
      // For now, just log that we would launch
      this.log(`🌐 Browser would be launched for session ${sessionId}`);
      
      // Console logging will be handled by session manager
      this.log(`📝 Console logging managed by session-manager for ${sessionId}`);
      
    } catch (error) {
      this.log(`❌ Failed to launch browser for session ${sessionId}: ${error}`, 'error');
    }
  }

  /**
   * Initialize available browser modules
   */
  private async initializeBrowserModules(): Promise<void> {
    // Register Chrome browser module
    const chromeModule = new ChromeBrowserModule();
    if (await chromeModule.isAvailable()) {
      this.launcher.registerModule(BrowserType.CHROME, chromeModule);
      this.log('✅ Chrome browser module registered');
    }
    
    // Register Opera GX as Chromium variant
    this.launcher.registerModule(BrowserType.CHROMIUM, chromeModule);
    this.log('✅ Chromium/Opera GX support registered');
    
    // TODO: Add other browser modules (Firefox, Safari, Edge)
  }

  /**
   * Get daemon capabilities
   */
  private async getCapabilities(): Promise<DaemonResponse> {
    return {
      success: true,
      data: {
        capabilities: [
          'browser-management',
          'session-optimization', 
          'modular-architecture'
        ],
        version: this.version,
        messageTypes: this.getSupportedMessageTypes(),
        stats: this.sessionManager.getStats()
      }
    };
  }

  /**
   * Create browser based on session requirements
   */
  private async createBrowser(request: BrowserRequest): Promise<DaemonResponse> {
    try {
      const sessionId = request.sessionId || `session-${Date.now()}`;
      
      // Calculate optimal placement strategy
      const strategy = this.sessionManager.calculatePlacement(sessionId, request.options);
      
      switch (strategy.type) {
        case 'reuse':
          return this.reuseBrowser(strategy.browserId!, sessionId);
          
        case 'add-tab':
          return this.addTabToBrowser(strategy.browserId!, sessionId);
          
        case 'create':
        default:
          return this.createNewBrowser(request, sessionId);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to create browser: ${errorMessage}`
      };
    }
  }

  /**
   * Create a new browser instance
   */
  private async createNewBrowser(request: BrowserRequest, sessionId: string): Promise<DaemonResponse> {
    const debugPort = await this.allocatePort();
    
    // Launch browser using the launcher module
    const defaultConfig: BrowserConfig = { type: BrowserType.DEFAULT };
    const launchResult = await this.launcher.launch(request.options || defaultConfig, debugPort);
    
    // Create managed browser record
    const browser = {
      id: `browser-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: request.options?.type || BrowserType.DEFAULT,
      pid: launchResult.pid,
      debugPort: launchResult.debugPort,
      status: BrowserStatus.READY,
      devToolsUrl: launchResult.devToolsUrl || `http://localhost:${debugPort}`,
      launchedAt: new Date(),
      lastActivity: new Date(),
      config: request.options || defaultConfig
    };
    
    // Register with session manager
    this.sessionManager.registerBrowser(browser);
    this.sessionManager.addSession(browser.id, sessionId);
    
    // Start console logging for this session if browser has debug port
    if (browser.debugPort && browser.devToolsUrl) {
      await this.startSessionConsoleLogging(sessionId, browser.devToolsUrl);
    }
    
    this.log(`✅ Created new browser ${browser.id} (${browser.type}) on port ${browser.debugPort}`);
    
    return {
      success: true,
      data: {
        browser: {
          id: browser.id,
          type: browser.type,
          port: browser.debugPort,
          status: browser.status,
          sessionId,
          devToolsUrl: browser.devToolsUrl
        }
      }
    };
  }

  /**
   * Reuse existing browser (refresh current tab)
   */
  private async reuseBrowser(browserId: string, sessionId: string): Promise<DaemonResponse> {
    const browser = this.sessionManager.getBrowser(browserId);
    if (!browser) {
      return { success: false, error: `Browser ${browserId} not found` };
    }

    // Add session to browser
    this.sessionManager.addSession(browserId, sessionId);
    
    // TODO: Implement tab refresh via DevTools API
    
    return {
      success: true,
      data: {
        browser: {
          id: browser.id,
          type: browser.type,
          port: browser.debugPort,
          status: browser.status,
          sessionId,
          reused: true
        }
      }
    };
  }

  /**
   * Add new tab to existing browser
   */
  private async addTabToBrowser(browserId: string, sessionId: string): Promise<DaemonResponse> {
    const browser = this.sessionManager.getBrowser(browserId);
    if (!browser) {
      return { success: false, error: `Browser ${browserId} not found` };
    }

    // Add session to browser
    this.sessionManager.addSession(browserId, sessionId);
    
    // TODO: Implement new tab creation via DevTools API
    
    return {
      success: true,
      data: {
        browser: {
          id: browser.id,
          type: browser.type,
          port: browser.debugPort,
          status: browser.status,
          sessionId,
          newTab: true
        }
      }
    };
  }

  /**
   * Destroy browser instance
   */
  private async destroyBrowser(_request: BrowserRequest): Promise<DaemonResponse> {
    // TODO: Implement browser destruction
    return { success: true, data: { message: 'Browser destroy not implemented' } };
  }

  /**
   * List managed browsers
   */
  private async listBrowsers(request: BrowserRequest): Promise<DaemonResponse> {
    const filters = request.filters || {};
    const browsers = this.sessionManager.filterBrowsers(filters);
    
    const browserList = browsers.map(browser => ({
      id: browser.id,
      type: browser.type,
      status: browser.status,
      created: browser.launchedAt,
      port: browser.debugPort
    }));
    
    return {
      success: true,
      data: {
        browsers: browserList,
        total: browserList.length,
        stats: this.sessionManager.getStats()
      }
    };
  }

  /**
   * Optimize browser resources
   */
  private async optimizeResources(_request: BrowserRequest): Promise<DaemonResponse> {
    const cleanedBrowsers = this.sessionManager.cleanupIdleBrowsers();
    
    return {
      success: true,
      data: {
        message: `Cleaned up ${cleanedBrowsers.length} idle browsers`,
        cleanedBrowsers,
        stats: this.sessionManager.getStats()
      }
    };
  }

  /**
   * Handle smart browser requests (legacy compatibility)
   */
  private async handleSmartBrowserRequest(requestData: any): Promise<DaemonResponse> {
    // Convert legacy request format to new BrowserRequest format
    const request: BrowserRequest = {
      action: BrowserAction.LAUNCH,
      sessionId: requestData.sessionId,
      options: {
        type: BrowserType.DEFAULT,
        devtools: requestData.devtools || false
      }
    };
    
    return this.createBrowser(request);
  }

  /**
   * Allocate an available port for browser debugging
   */
  private async allocatePort(): Promise<number> {
    // Simple port allocation - start from 9222 and increment
    const basePort = 9222;
    for (let i = 0; i < 100; i++) {
      const port = basePort + i;
      // TODO: Check if port is actually available
      return port;
    }
    throw new Error('No available ports for browser debugging');
  }

  /**
   * Start console logging for a session via the session manager
   */
  private async startSessionConsoleLogging(sessionId: string, devToolsUrl: string): Promise<void> {
    try {
      this.log(`🔌 Starting console logging for session ${sessionId}: ${devToolsUrl}`);
      
      // Get session info from session manager daemon via messaging
      const sessionInfoResponse = await this.sendMessage('session-manager', 'get_session_info', { sessionId });
      
      if (!sessionInfoResponse.success) {
        this.log(`⚠️ Could not get session info for ${sessionId}: ${sessionInfoResponse.error}`, 'warn');
        return;
      }
      
      const sessionData = sessionInfoResponse.data;
      const browserLogPath = sessionData.artifacts?.logs?.client?.[0];
      
      if (!browserLogPath) {
        this.log(`⚠️ No browser log path found for session ${sessionId}`, 'warn');
        return;
      }
      
      // Create and configure console logger
      const consoleLogger = new SessionConsoleLogger();
      consoleLogger.setSessionLogPath(browserLogPath);
      
      // Start capturing console logs
      await consoleLogger.startLogging(devToolsUrl);
      
      // Track the logger for cleanup
      this.consoleLoggers.set(sessionId, consoleLogger);
      
      this.log(`✅ Console logging active for session ${sessionId} → ${browserLogPath}`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to start console logging for session ${sessionId}: ${errorMessage}`, 'error');
    }
  }
}