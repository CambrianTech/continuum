/**
 * Browser Manager Daemon - Clean orchestrator
 * 
 * MODULAR ARCHITECTURE:
 * - BrowserLauncher: Handles browser process launching
 * - BrowserSessionManager: Manages session lifecycle and placement
 * - Composed together for clean, testable functionality
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon';
import { DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { SystemEventType, SessionCreatedPayload, SessionJoinedPayload } from '../base/EventTypes';
import { BrowserType, BrowserRequest, BrowserConfig, BrowserStatus, BrowserAction } from './types/index.js';
// BrowserFilters unused in this file
import { BrowserLauncher } from './modules/BrowserLauncher';
import { BrowserSessionManager } from './modules/BrowserSessionManager';
import { ChromeBrowserModule } from './modules/ChromeBrowserModule';
import { SessionConsoleLogger } from '../session-manager/modules/SessionConsoleLogger';
import { MacOperaAdapter, MacChromeAdapter, BaseBrowserAdapter } from './modules/BrowserTabAdapter';
import { DAEMON_EVENT_BUS } from '../base/DaemonEventBus';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);
// import { BrowserTabManager } from './modules/BrowserTabAdapter.js'; // TODO: Remove if not used

export class BrowserManagerDaemon extends MessageRoutedDaemon {
  public readonly name = 'browser-manager';
  public readonly version = '2.0.0';
  public readonly daemonType = DaemonType.BROWSER_MANAGER;

  private launcher = new BrowserLauncher();
  private sessionManager = new BrowserSessionManager();
  // private hasActiveBrowser = false; // TODO: Remove if not needed
  private tabAdapter: BaseBrowserAdapter;
  // private _browserTabManager = new BrowserTabManager(); // TODO: Remove if not used
  
  // Track console loggers for each session
  private consoleLoggers = new Map<string, SessionConsoleLogger>();
  
  // Auto-cleanup interval
  private zombieCleanupInterval: NodeJS.Timeout | undefined = undefined;
  
  
  // SAFETY: Prevent multiple simultaneous browser launches globally
  private isLaunchingBrowser = false;
  private readonly BROWSER_LAUNCH_TIMEOUT_MS = 10000; // 10 seconds timeout
  
  // MessageRoutedDaemon implementation
  protected readonly primaryMessageType = 'browser_request';
  
  protected getRouteMap(): MessageRouteMap {
    return {
      'create': (data: unknown) => this.createBrowser(data as BrowserRequest),
      'destroy': (data: unknown) => this.destroyBrowser(data as BrowserRequest),
      'list': (data: unknown) => this.listBrowsers(data as BrowserRequest),
      'optimize': (data: unknown) => this.optimizeResources(data as BrowserRequest)
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
    
    // Initialize platform-specific tab adapter
    await this.initializeTabAdapter();
    
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
    Array.from(this.consoleLoggers.keys()).forEach(async (sessionId) => {
      const logger = this.consoleLoggers.get(sessionId);
      if (logger) {
        try {
          await logger.stopLogging();
          this.log(`🔌 Stopped console logging for session ${sessionId}`);
        } catch (error) {
          this.log(`⚠️ Error stopping console logger for ${sessionId}: ${error}`, 'warn');
        }
      }
    });
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
   * Setup session event listening to launch browsers when sessions are created OR joined
   */
  private setupSessionEventListening(): void {
    // Listen for session_created events - check if session needs browser
    DAEMON_EVENT_BUS.onEvent(SystemEventType.SESSION_CREATED, async (event: SessionCreatedPayload) => {
      const { sessionId, sessionType, owner } = event;
      this.log(`📋 Session created: ${sessionId} (${sessionType}) for ${owner}`);
      
      if (this.sessionNeedsBrowser(sessionType)) {
        await this.ensureSessionHasBrowser(sessionId, sessionType, owner);
      }
    });
    
    // Listen for session_joined events - check if session needs browser  
    DAEMON_EVENT_BUS.onEvent(SystemEventType.SESSION_JOINED, async (event: SessionJoinedPayload) => {
      const { sessionId, sessionType, owner } = event;
      this.log(`📋 Session joined: ${sessionId} (${sessionType}) for ${owner}`);
      
      if (this.sessionNeedsBrowser(sessionType)) {
        await this.ensureSessionHasBrowser(sessionId, sessionType, owner);
      }
    });
    
    this.log('👂 Listening for session events with smart single-tab logic');
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
   * SMART: Ensure session has browser - check if exists first, only launch if missing
   * Uses semaphore protection to prevent race conditions and multiple launches
   * @param killZombies - Whether to close zombie tabs that aren't connected to active sessions
   */
  private async ensureSessionHasBrowser(sessionId: string, _sessionType: string, _owner: string, killZombies: boolean = false): Promise<void> {
    try {
      // SEMAPHORE: Prevent multiple simultaneous browser launches globally
      if (this.isLaunchingBrowser) {
        this.log(`⏸️ Already launching browser - skipping duplicate request for session ${sessionId}`);
        return;
      }
      
      // Acquire semaphore FIRST to block all other requests
      this.isLaunchingBrowser = true;
      
      this.log(`🔍 [SEMAPHORE] Acquired browser launch lock for session ${sessionId}`);
      this.log(`🔍 Checking if any browser tab exists for localhost:9000...`);
      
      // Check if browser tab exists AFTER acquiring semaphore using proper adapter
      const tabCount = await this.tabAdapter.countTabs('localhost:9000');
      this.log(`🔍 Tab check result: ${tabCount} tab(s) found (via ${this.tabAdapter.constructor.name})`);
      
      if (tabCount > 0) {
        this.log(`✅ Found ${tabCount} browser tab(s) already open - ONE TAB POLICY satisfied`);
        
        // SMART ZOMBIE KILLER: Close zombie tabs if requested
        if (killZombies && tabCount > 1) {
          this.log(`🧟 Zombie killer enabled - will close ${tabCount - 1} zombie tab(s)`);
          await this.killZombieTabs(sessionId);
        }
        
        this.log(`🔍 [SEMAPHORE] Releasing lock - no launch needed`);
        this.isLaunchingBrowser = false; // Release semaphore
        return; // Tab exists, do nothing
      }
      
      this.log(`🚀 No browser tabs found - launching exactly ONE tab for session ${sessionId}`);
      
      // Set timeout to automatically release semaphore if launch hangs
      const timeoutId = setTimeout(() => {
        if (this.isLaunchingBrowser) {
          this.log(`⏰ Browser launch timeout - releasing semaphore`, 'warn');
          this.isLaunchingBrowser = false;
        }
      }, this.BROWSER_LAUNCH_TIMEOUT_MS);
      
      try {
        // Launch exactly one browser tab
        const browserConfig: BrowserConfig = {
          type: BrowserType.DEFAULT,
          headless: false,
          url: 'http://localhost:9000',
        };
        
        const launchResult = await this.launcher.launch(browserConfig, 0);
        this.log(`✅ Browser launched for session ${sessionId} (PID: ${launchResult.pid})`);
        
        // FOCUS BROWSER: Bring to front if requested
        if (killZombies) { // Use killZombies as a proxy for focus for now
          await this.focusBrowser();
        }
        this.log(`🔍 [SEMAPHORE] Browser launch complete - releasing lock`);
        
      } finally {
        // Clear timeout and release semaphore
        clearTimeout(timeoutId);
        this.isLaunchingBrowser = false;
        this.log(`🔍 [SEMAPHORE] Lock released after launch attempt`);
      }
      
    } catch (error) {
      this.log(`❌ Failed to ensure browser for session ${sessionId}: ${error}`, 'error');
      // Ensure we clean up the semaphore on error
      this.isLaunchingBrowser = false;
      this.log(`🔍 [SEMAPHORE] Lock released due to error`);
    }
  }
  

  /**
   * Initialize platform-specific tab adapter
   */
  private async initializeTabAdapter(): Promise<void> {
    // Detect platform and available browsers
    const platform = process.platform;
    
    if (platform === 'darwin') {
      // macOS - try Opera first, then Chrome
      try {
        const operaAdapter = new MacOperaAdapter();
        // Test if Opera GX is available
        const testCount = await operaAdapter.countTabs('localhost:9000');
        this.tabAdapter = operaAdapter;
        this.log('✅ Using Opera GX adapter for tab management');
        return;
      } catch (error) {
        this.log('⚠️ Opera GX not available, trying Chrome adapter');
      }
      
      try {
        this.tabAdapter = new MacChromeAdapter();
        this.log('✅ Using Chrome adapter for tab management');
        return;
      } catch (error) {
        this.log('⚠️ Chrome adapter failed, falling back to default');
      }
    }
    
    // Fallback: Use Opera adapter as default (most compatible)
    this.tabAdapter = new MacOperaAdapter();
    this.log('⚠️ Using Opera adapter as fallback for tab management');
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
    const defaultConfig: BrowserConfig = { 
      type: BrowserType.DEFAULT,
      url: 'http://localhost:9000'
    };
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
        url: 'http://localhost:9000',
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
   * SMART ZOMBIE KILLER: Close browser tabs that aren't connected to active sessions
   * Uses DevTools Protocol for reliable tab management
   */
  private async killZombieTabs(currentSessionId: string): Promise<void> {
    try {
      this.log(`🧟 Starting zombie tab cleanup for session ${currentSessionId}`);
      
      // For now, use simple AppleScript approach for macOS Opera GX
      // TODO: Implement DevTools Protocol approach for reliable tab identification
      if (process.platform === 'darwin') {
        const script = `
          tell application "Opera GX"
            set tabsToClose to {}
            repeat with w in (get windows)
              repeat with t in tabs of w
                if (URL of t contains "localhost:9000") then
                  -- TODO: Add WebSocket connection checking via DevTools
                  -- For now, keep only the first tab found
                  if (count of tabsToClose) > 0 then
                    set end of tabsToClose to t
                  end if
                end if
              end repeat
            end repeat
            
            repeat with t in tabsToClose
              close t
            end repeat
            
            return (count of tabsToClose)
          end tell
        `;
        
        const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
        const closedCount = parseInt(stdout.trim()) || 0;
        this.log(`🧟 Closed ${closedCount} zombie tab(s)`);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️ Zombie cleanup failed: ${errorMessage}`, 'warn');
    }
  }

  /**
   * FOCUS BROWSER: Bring browser window to front using platform-specific methods
   */
  private async focusBrowser(): Promise<void> {
    try {
      if (process.platform === 'darwin') {
        // macOS: Use AppleScript to bring Opera GX to front
        const script = `
          tell application "Opera GX"
            activate
            repeat with w in (get windows)
              repeat with t in tabs of w
                if (URL of t contains "localhost:9000") then
                  set index of w to 1
                  set active tab index of w to (index of t)
                  return
                end if
              end repeat
            end repeat
          end tell
        `;
        
        await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
        this.log(`🎯 Focused browser window`);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️ Browser focus failed: ${errorMessage}`, 'warn');
    }
  }

  /**
   * Start console logging for a session via the session manager
   */
  private async startSessionConsoleLogging(sessionId: string, devToolsUrl: string): Promise<void> {
    try {
      this.log(`🔌 Starting console logging for session ${sessionId}: ${devToolsUrl}`);
      
      // Get session info from session manager daemon via messaging
      const sessionInfoResponse = await this.sendMessage(DaemonType.SESSION_MANAGER, 'get_session_info', { sessionId });
      
      if (!sessionInfoResponse.success) {
        this.log(`⚠️ Could not get session info for ${sessionId}: ${sessionInfoResponse.error}`, 'warn');
        return;
      }
      
      const sessionData = sessionInfoResponse.data as any; // TODO: Add proper session type
      const browserLogPath = sessionData?.artifacts?.logs?.client?.[0];
      
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