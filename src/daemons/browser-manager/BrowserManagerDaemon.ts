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
import { BrowserType, BrowserRequest } from './types/index.js';
// BrowserFilters unused in this file
import { BrowserLauncher } from './modules/BrowserLauncher.js';
import { BrowserSessionManager } from './modules/BrowserSessionManager.js';
import { ChromeBrowserModule } from './modules/ChromeBrowserModule.js';

export class BrowserManagerDaemon extends MessageRoutedDaemon {
  public readonly name = 'browser-manager';
  public readonly version = '2.0.0';

  private launcher = new BrowserLauncher();
  private sessionManager = new BrowserSessionManager();
  
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
    
    this.log('Browser Manager Daemon v2.0 started - modular architecture');
  }

  protected async onStop(): Promise<void> {
    // Cleanup managed browsers
    const browsers = this.sessionManager.getAllBrowsers();
    this.log(`Shutting down ${browsers.length} managed browsers`);
    
    // TODO: Implement graceful browser shutdown
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
    const launchResult = await this.launcher.launch(request.options || {}, debugPort);
    
    // Create managed browser record
    const browser = {
      id: `browser-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: request.options?.type || BrowserType.DEFAULT,
      pid: launchResult.pid,
      debugPort: launchResult.debugPort,
      status: 'ready' as const,
      devToolsUrl: launchResult.devToolsUrl,
      launchedAt: new Date(),
      lastActivity: new Date(),
      config: request.options || {}
    };
    
    // Register with session manager
    this.sessionManager.registerBrowser(browser);
    this.sessionManager.addSession(browser.id, sessionId);
    
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
      action: 'launch',
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
}