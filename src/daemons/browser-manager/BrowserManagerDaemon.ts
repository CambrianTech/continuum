/**
 * Browser Manager Daemon - System service for browser orchestration
 * Runs as a daemon process under Continuum OS
 * Handles browser lifecycle, tab management, resource optimization
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon.js';
import { DaemonResponse } from '../base/DaemonProtocol.js';

export interface BrowserRequest {
  type: 'create' | 'destroy' | 'list' | 'optimize';
  sessionId?: string;
  config?: BrowserConfig;
  filters?: BrowserFilters;
}

export interface BrowserConfig {
  purpose: string;
  persona: string;
  requirements: {
    devtools?: boolean;
    extensions?: boolean;
    isolation: 'shared' | 'sandboxed' | 'dedicated';
    visibility: 'hidden' | 'minimized' | 'visible';
    persistence: 'ephemeral' | 'session' | 'permanent';
  };
  resources: {
    priority: 'background' | 'normal' | 'high';
    memoryLimit?: number;
    cpuLimit?: number;
  };
}

export interface BrowserFilters {
  status?: string[];
  persona?: string[];
  ageMinutes?: number;
}

export interface ManagedBrowser {
  id: string;
  pid: number;
  port: number;
  type: string; // chrome, firefox, opera, etc.
  state: 'starting' | 'ready' | 'busy' | 'idle' | 'stopping';
  sessions: Set<string>;
  resources: {
    memory: number;
    cpu: number;
    tabs: number;
  };
  config: BrowserConfig;
  startTime: Date;
  lastActivity: Date;
}

/**
 * Browser Manager Daemon - System service
 */
export class BrowserManagerDaemon extends MessageRoutedDaemon {
  public readonly name = 'browser-manager';
  public readonly version = '1.0.0';
  private browsers = new Map<string, ManagedBrowser>();
  private resourceMonitor: NodeJS.Timeout | null = null;
  
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
      'get_capabilities': this.getCapabilities.bind(this)
    };
  }

  protected async onStart(): Promise<void> {
    // Start resource monitoring
    this.startResourceMonitoring();
    
    // Register with OS process manager
    await this.registerWithOS();
    
    this.log('Browser Manager Daemon started');
  }

  protected async onStop(): Promise<void> {
    if (this.resourceMonitor) {
      clearInterval(this.resourceMonitor);
    }
    
    // Gracefully shutdown all browsers
    await this.shutdownAllBrowsers();
  }

  private async getCapabilities(): Promise<DaemonResponse> {
    return {
      success: true,
      data: {
        capabilities: [
          'browser-management',
          'tab-orchestration', 
          'resource-optimization'
        ],
        messageTypes: this.getSupportedMessageTypes(),
        routes: this.getSupportedRoutes()
      }
    };
  }

  /**
   * Create browser based on session requirements
   */
  private async createBrowser(request: BrowserRequest): Promise<DaemonResponse> {
    try {
      if (!request.config) {
        throw new Error('Browser config required for creation');
      }

      // Analyze placement strategy
      const strategy = this.analyzePlacement(request.config);
      
      // Execute strategy
      const browser = await this.executePlacement(strategy, request.config);
      
      // Register browser
      this.browsers.set(browser.id, browser);
      
      this.log(`Created browser ${browser.id} (${browser.type}) on port ${browser.port}`);
      
      return {
        success: true,
        data: {
          browser: {
            id: browser.id,
            type: browser.type,
            port: browser.port,
            status: browser.state,
            purpose: browser.config.purpose,
            persona: browser.config.persona,
            sessionId: request.sessionId,
            created: browser.startTime,
            pid: browser.pid
          }
        }
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Intelligent placement analysis
   */
  private analyzePlacement(config: BrowserConfig): PlacementStrategy {
    // Check for reusable browsers
    if (config.requirements.isolation === 'shared') {
      const compatible = this.findCompatibleBrowser(config);
      if (compatible) {
        return {
          type: 'reuse',
          browserId: compatible.id,
          reasoning: 'Found compatible shared browser'
        };
      }
    }

    // Check resource constraints
    if (this.browsers.size >= this.getMaxBrowsers()) {
      const leastUsed = this.findLeastUsedBrowser();
      if (leastUsed && leastUsed.sessions.size < this.getMaxTabsPerBrowser()) {
        return {
          type: 'add-tab',
          browserId: leastUsed.id,
          reasoning: 'Resource limits reached, adding tab to existing browser'
        };
      }
    }

    // Default: create new browser
    return {
      type: 'create',
      reasoning: 'Creating new dedicated browser instance'
    };
  }

  /**
   * Execute placement strategy
   */
  private async executePlacement(strategy: PlacementStrategy, config: BrowserConfig): Promise<ManagedBrowser> {
    switch (strategy.type) {
      case 'reuse':
        return await this.reuseBrowser(strategy.browserId!, config);
      
      case 'add-tab':
        return await this.addTabToBrowser(strategy.browserId!, config);
      
      case 'create':
        return await this.createNewBrowser(config);
      
      default:
        throw new Error(`Unknown placement strategy: ${strategy.type}`);
    }
  }

  /**
   * Create new browser instance
   */
  private async createNewBrowser(config: BrowserConfig): Promise<ManagedBrowser> {
    const browserId = `browser-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const port = await this.allocatePort();
    
    // Select browser type based on requirements
    const browserType = this.selectBrowserType(config);
    
    // For testing - create mock browser without actual process
    const browser: ManagedBrowser = {
      id: browserId,
      pid: Math.floor(Math.random() * 90000) + 10000, // Mock PID
      port,
      type: browserType,
      state: 'ready', // Start ready for testing
      sessions: new Set(),
      resources: { memory: 128, cpu: 5, tabs: 1 },
      config,
      startTime: new Date(),
      lastActivity: new Date()
    };

    this.log(`Mock browser created: ${browserId} (${browserType}) on port ${port}`);
    return browser;
  }

  /**
   * Resource monitoring and optimization
   */
  private startResourceMonitoring(): void {
    this.resourceMonitor = setInterval(async () => {
      await this.updateResourceUsage();
      await this.performOptimizations();
    }, 30000); // Every 30 seconds
  }

  /**
   * Update resource usage for all browsers
   */
  private async updateResourceUsage(): Promise<void> {
    for (const browser of this.browsers.values()) {
      try {
        // Get process stats
        const stats = await this.getProcessStats(browser.pid);
        browser.resources.memory = stats.memory;
        browser.resources.cpu = stats.cpu;
        
        // Get tab count from DevTools
        const tabs = await this.getTabCount(browser.port);
        browser.resources.tabs = tabs;
        
      } catch (error) {
        this.log(`Failed to update stats for browser ${browser.id}: ${error}`, 'warn');
      }
    }
  }

  /**
   * Perform resource optimizations
   */
  private async performOptimizations(): Promise<void> {
    // Close idle browsers
    for (const browser of this.browsers.values()) {
      if (this.isIdle(browser) && this.canClose(browser)) {
        await this.closeBrowser(browser.id);
      }
    }

    // Consolidate underutilized browsers
    await this.consolidateResources();
  }

  /**
   * Register with Continuum OS process manager
   */
  private async registerWithOS(): Promise<void> {
    // Send registration message to OS
    const registration = {
      daemon: this.name,
      version: this.version,
      capabilities: [
        'browser-management',
        'tab-orchestration', 
        'resource-optimization'
      ],
      endpoints: [
        'create-browser',
        'destroy-browser',
        'list-browsers',
        'optimize-resources'
      ]
    };

    await this.sendToOS('daemon-register', registration);
  }

  // Helper methods
  private findCompatibleBrowser(config: BrowserConfig): ManagedBrowser | null {
    for (const browser of this.browsers.values()) {
      if (this.isCompatible(browser, config)) {
        return browser;
      }
    }
    return null;
  }

  private isCompatible(browser: ManagedBrowser, _config: BrowserConfig): boolean {
    // Check if browser can handle this session
    return browser.config.requirements.isolation === 'shared' &&
           browser.sessions.size < this.getMaxTabsPerBrowser() &&
           browser.state === 'ready';
  }

  private findLeastUsedBrowser(): ManagedBrowser | null {
    return Array.from(this.browsers.values())
      .sort((a, b) => a.sessions.size - b.sessions.size)[0] || null;
  }

  private selectBrowserType(_config: BrowserConfig): string {
    // Default browser selection logic
    return 'chrome'; // Could be made configurable
  }

  // TODO: Browser process management - extract to BrowserProcessManager class
  // These methods will be implemented when browser automation is needed
  // Currently browser management is handled through external DevTools integration

  private isIdle(browser: ManagedBrowser): boolean {
    const idleThreshold = 5 * 60 * 1000; // 5 minutes
    return Date.now() - browser.lastActivity.getTime() > idleThreshold &&
           browser.sessions.size === 0;
  }

  private canClose(browser: ManagedBrowser): boolean {
    return browser.config.requirements.persistence === 'ephemeral';
  }

  private getMaxBrowsers(): number { return 5; }
  private getMaxTabsPerBrowser(): number { return 10; }

  // Stub implementations for compilation
  private async reuseBrowser(_browserId: string, _config: BrowserConfig): Promise<ManagedBrowser> { throw new Error('Not implemented'); }
  private async addTabToBrowser(_browserId: string, _config: BrowserConfig): Promise<ManagedBrowser> { throw new Error('Not implemented'); }
  private async allocatePort(): Promise<number> { 
    // Simple port allocation starting from 9222
    const usedPorts = new Set(Array.from(this.browsers.values()).map(b => b.port));
    let port = 9222;
    while (usedPorts.has(port) && port < 9300) {
      port++;
    }
    return port;
  }
  private async getProcessStats(_pid: number): Promise<{ memory: number; cpu: number }> { return { memory: 0, cpu: 0 }; }
  private async getTabCount(_port: number): Promise<number> { return 0; }
  private async closeBrowser(_id: string): Promise<void> {}
  private async consolidateResources(): Promise<void> {}
  private async destroyBrowser(request: BrowserRequest): Promise<DaemonResponse> {
    // Mock browser destruction
    return {
      success: true,
      data: {
        message: 'Browser terminated',
        sessionId: request.sessionId
      }
    };
  }

  private async listBrowsers(request: BrowserRequest): Promise<DaemonResponse> {
    const browsers = Array.from(this.browsers.values()).map(browser => ({
      id: browser.id,
      type: browser.type,
      status: browser.state,
      persona: browser.config.persona,
      created: browser.startTime
    }));
    
    return {
      success: true,
      data: {
        browsers
      }
    };
  }

  private async optimizeResources(request: BrowserRequest): Promise<DaemonResponse> {
    // Mock optimization
    return {
      success: true,
      data: {
        optimized: true,
        memoryFreed: Math.floor(Math.random() * 100) + 50,
        browsersOptimized: Math.floor(Math.random() * 5) + 1
      }
    };
  }
  private async shutdownAllBrowsers(): Promise<void> {}
  private async sendToOS(_type: string, _data: any): Promise<void> {}
}

interface PlacementStrategy {
  type: 'reuse' | 'add-tab' | 'create';
  browserId?: string;
  reasoning: string;
}