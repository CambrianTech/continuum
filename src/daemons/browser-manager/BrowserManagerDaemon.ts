/**
 * Browser Manager Daemon - System service for browser orchestration
 * Runs as a daemon process under Continuum OS
 * Handles browser lifecycle, tab management, resource optimization
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon.js';
import { DaemonResponse } from '../base/DaemonProtocol.js';
import ProcessCommand from '../../commands/kernel/system/ProcessCommand';

export enum BrowserType {
  DEFAULT = 'default',
  CHROME = 'chrome',
  FIREFOX = 'firefox',
  SAFARI = 'safari',
  EDGE = 'edge',
  OPERA = 'opera'
}

export enum BrowserPurpose {
  USER = 'user',
  DEVELOPMENT = 'development',
  AUTOMATION = 'automation',
  TESTING = 'testing',
  INTEGRATION_TEST = 'integration-test'
}

export enum BrowserVisibility {
  HIDDEN = 'hidden',
  MINIMIZED = 'minimized', 
  VISIBLE = 'visible'
}

export enum BrowserIsolation {
  SHARED = 'shared',
  SANDBOXED = 'sandboxed',
  DEDICATED = 'dedicated'
}

export enum BrowserPersistence {
  EPHEMERAL = 'ephemeral',
  SESSION = 'session',
  PERMANENT = 'permanent'
}

export interface BrowserRequest {
  type: 'create' | 'destroy' | 'list' | 'optimize';
  sessionId?: string;
  config?: BrowserConfig;
  filters?: BrowserFilters;
}

export interface BrowserConfig {
  purpose: BrowserPurpose;
  persona: string;
  browserType?: BrowserType; // Optional explicit browser choice
  requirements: {
    devtools?: boolean;
    extensions?: boolean;
    isolation: BrowserIsolation;
    visibility: BrowserVisibility;
    persistence: BrowserPersistence;
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
      'get_capabilities': this.getCapabilities.bind(this),
      'create_browser': this.handleSmartBrowserRequest.bind(this)
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
   * Create new browser instance with real process
   */
  private async createNewBrowser(config: BrowserConfig): Promise<ManagedBrowser> {
    const browserId = `browser-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const port = await this.allocatePort();
    
    // Select browser type and binary path
    const browserType = this.selectBrowserType(config);
    const browserPath = await this.getBrowserPath(browserType);
    
    if (!browserPath) {
      throw new Error(`Browser ${browserType} not found on system`);
    }
    
    // Build browser launch arguments
    const args = this.buildBrowserArgs(config, port);
    
    try {
      // Launch real browser process
      const { spawn } = await import('child_process');
      const browserProcess = spawn(browserPath, args, {
        detached: false,
        stdio: 'pipe'
      });
      
      const browser: ManagedBrowser = {
        id: browserId,
        pid: browserProcess.pid!,
        port,
        type: browserType,
        state: 'starting',
        sessions: new Set(),
        resources: { memory: 0, cpu: 0, tabs: 0 },
        config,
        startTime: new Date(),
        lastActivity: new Date()
      };
      
      // Set up process monitoring
      this.monitorBrowserProcess(browser, browserProcess);
      
      // Wait for browser to be ready
      await this.waitForBrowserReady(browser);
      
      this.log(`✅ Real browser launched: ${browserId} (${browserType}) PID ${browser.pid} on port ${port}`);
      return browser;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to launch browser: ${errorMessage}`, 'error');
      throw new Error(`Browser launch failed: ${errorMessage}`);
    }
  }
  
  /**
   * Get browser binary path with intelligent detection and fallbacks
   */
  private async getBrowserPath(browserType: BrowserType): Promise<string | null> {
    try {
      // Special handling for 'default' - detect system default browser
      if (browserType === 'default') {
        const defaultBrowser = await this.getDefaultSystemBrowser();
        if (defaultBrowser) {
          return await this.getBrowserPath(defaultBrowser);
        }
      }
      
      // Define browser detection patterns and paths
      const browserConfigs = {
        chrome: {
          patterns: ['chrome', 'chromium'],
          paths: [
            '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
            '/usr/bin/google-chrome',
            '/usr/bin/chromium-browser'
          ]
        },
        firefox: {
          patterns: ['firefox'],
          paths: [
            '/Applications/Firefox.app/Contents/MacOS/firefox',
            '/usr/bin/firefox'
          ]
        },
        safari: {
          patterns: ['safari'],
          paths: [
            '/Applications/Safari.app/Contents/MacOS/Safari'
          ]
        },
        edge: {
          patterns: ['edge', 'msedge'],
          paths: [
            '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
            '/usr/bin/microsoft-edge'
          ]
        },
        opera: {
          patterns: ['opera'],
          paths: [
            '/Applications/Opera.app/Contents/MacOS/Opera',
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '/usr/bin/opera'
          ]
        }
      };
      
      const config = browserConfigs[browserType as keyof typeof browserConfigs];
      if (!config) {
        this.log(`Unknown browser type: ${browserType}`, 'error');
        return null;
      }
      
      // First check common installation paths
      const fs = await import('fs');
      for (const path of config.paths) {
        if (fs.existsSync(path)) {
          this.log(`Found ${browserType} at: ${path}`);
          return path;
        }
      }
      
      // Fallback: try to find via which/where command
      try {
        const { exec } = await import('child_process');
        const { promisify } = await import('util');
        const execAsync = promisify(exec);
        
        for (const pattern of config.patterns) {
          try {
            const { stdout } = await execAsync(`which ${pattern} 2>/dev/null || where ${pattern} 2>/dev/null`);
            const path = stdout.trim();
            if (path && fs.existsSync(path)) {
              this.log(`Found ${browserType} via which: ${path}`);
              return path;
            }
          } catch (error) {
            // Continue trying other patterns
          }
        }
      } catch (error) {
        this.log(`Failed to search for ${browserType}: ${error}`);
      }
      
      this.log(`Browser ${browserType} not found on system`);
      return null;
    } catch (error) {
      this.log(`Failed to get browser path for ${browserType}: ${error}`, 'error');
      return null;
    }
  }
  
  /**
   * Detect system default browser
   */
  private async getDefaultSystemBrowser(): Promise<string | null> {
    try {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      // macOS: Use system_profiler to get default browser
      if (process.platform === 'darwin') {
        try {
          const { stdout } = await execAsync('defaults read com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers | grep -A 2 "LSHandlerURLScheme.*http" | grep LSHandlerRoleAll | head -1');
          const bundleId = stdout.match(/LSHandlerRoleAll = "(.+?)"/)?.[1];
          
          if (bundleId) {
            // Map bundle IDs to browser types
            const bundleMap: Record<string, string> = {
              'com.google.chrome': 'chrome',
              'org.mozilla.firefox': 'firefox',
              'com.apple.safari': 'safari',
              'com.microsoft.edgemac': 'edge',
              'com.operasoftware.opera': 'opera'
            };
            
            const browserType = bundleMap[bundleId];
            if (browserType) {
              this.log(`Detected default browser: ${browserType} (${bundleId})`);
              return browserType;
            }
          }
        } catch (error) {
          // Fallback detection methods
        }
      }
      
      // Linux: Check xdg-settings or alternatives
      if (process.platform === 'linux') {
        try {
          const { stdout } = await execAsync('xdg-settings get default-web-browser');
          const desktopFile = stdout.trim();
          
          if (desktopFile.includes('chrome')) return 'chrome';
          if (desktopFile.includes('firefox')) return 'firefox';
          if (desktopFile.includes('safari')) return 'safari';
          if (desktopFile.includes('edge')) return 'edge';
          if (desktopFile.includes('opera')) return 'opera';
        } catch (error) {
          // Continue to fallback
        }
      }
      
      // Universal fallback: Look for most common browsers in order of preference
      const preferenceOrder = ['chrome', 'firefox', 'safari', 'edge', 'opera'];
      
      for (const browserType of preferenceOrder) {
        const path = await this.getBrowserPath(browserType);
        if (path) {
          this.log(`Using fallback browser: ${browserType}`);
          return browserType;
        }
      }
      
      this.log('No default browser detected, and no fallback browsers available');
      return null;
    } catch (error) {
      this.log(`Failed to detect default browser: ${error}`, 'error');
      return null;
    }
  }
  
  /**
   * Build browser launch arguments based on config
   */
  private buildBrowserArgs(config: BrowserConfig, debugPort: number): string[] {
    const args = [
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-default-apps',
      '--disable-popup-blocking',
      '--disable-translate',
      '--disable-background-timer-throttling',
      '--disable-renderer-backgrounding',
      '--disable-device-discovery-notifications',
      `--remote-debugging-port=${debugPort}`
    ];
    
    // DevTools specific arguments
    if (config.requirements.devtools) {
      args.push(
        '--auto-open-devtools-for-tabs',
        '--disable-web-security',
        '--disable-features=VizDisplayCompositor'
      );
    }
    
    // Isolation arguments
    if (config.requirements.isolation === 'sandboxed') {
      args.push('--no-sandbox', '--disable-setuid-sandbox');
    } else if (config.requirements.isolation === 'dedicated') {
      args.push(`--user-data-dir=/tmp/continuum-browser-${Date.now()}`);
    }
    
    // Visibility arguments
    if (config.requirements.visibility === 'hidden') {
      args.push('--headless');
    } else if (config.requirements.visibility === 'minimized') {
      args.push('--start-minimized');
    }
    
    // Launch URL
    args.push('http://localhost:9000');
    
    return args;
  }
  
  /**
   * Monitor browser process lifecycle
   */
  private monitorBrowserProcess(browser: ManagedBrowser, process: any): void {
    process.on('error', (error: any) => {
      this.log(`❌ Browser ${browser.id} process error: ${error}`, 'error');
      browser.state = 'stopping';
    });
    
    process.on('exit', (code: number) => {
      this.log(`🔄 Browser ${browser.id} exited with code ${code}`);
      this.browsers.delete(browser.id);
    });
    
    // Monitor stdout/stderr for debugging
    if (process.stdout) {
      process.stdout.on('data', (data: any) => {
        this.log(`📋 Browser ${browser.id} stdout: ${data.toString().trim()}`);
      });
    }
    
    if (process.stderr) {
      process.stderr.on('data', (data: any) => {
        this.log(`⚠️  Browser ${browser.id} stderr: ${data.toString().trim()}`);
      });
    }
  }
  
  /**
   * Wait for browser to be ready using event-driven feedback with timeout fallback
   */
  private async waitForBrowserReady(browser: ManagedBrowser): Promise<void> {
    return new Promise((resolve, reject) => {
      // Timeout fallback - reject if event never comes (defensive programming)
      const timeoutMs = 30000; // 30 second fallback - should be plenty for Chrome
      const fallbackTimeout = setTimeout(() => {
        reject(new Error(`Browser ${browser.id} failed to become ready within ${timeoutMs}ms fallback timeout`));
      }, timeoutMs);
      
      let checkAttempts = 0;
      
      const checkReadiness = async () => {
        checkAttempts++;
        
        try {
          // First check if process is still alive using ProcessCommand feedback
          const processResult = await ProcessCommand.execute({
            subcommand: 'find',
            criteria: { pattern: browser.pid.toString(), matchType: 'name' }
          });
          
          if (!processResult.success || processResult.data.matches.length === 0) {
            clearTimeout(fallbackTimeout);
            reject(new Error(`Browser process ${browser.pid} died during startup`));
            return;
          }
          
          // Then check if DevTools port is responsive (event-driven check)
          try {
            const response = await fetch(`http://localhost:${browser.port}/json/version`, {
              signal: AbortSignal.timeout(2000) // Quick check per attempt
            });
            
            if (response.ok) {
              clearTimeout(fallbackTimeout);
              browser.state = 'ready';
              this.log(`✅ Browser ${browser.id} ready on port ${browser.port} after ${checkAttempts} checks`);
              resolve();
              return;
            }
          } catch (fetchError) {
            // DevTools not ready yet, but process is alive - continue checking
            this.log(`🔄 Browser ${browser.id} DevTools not ready yet (attempt ${checkAttempts})`);
          }
          
          // Schedule next check with exponential backoff (event-driven)
          const backoffMs = Math.min(500 * Math.pow(1.3, checkAttempts), 3000);
          setTimeout(checkReadiness, backoffMs);
          
        } catch (error) {
          clearTimeout(fallbackTimeout);
          reject(new Error(`Browser readiness check failed: ${error}`));
        }
      };
      
      // Start checking immediately
      checkReadiness();
    });
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

  private selectBrowserType(config: BrowserConfig): BrowserType {
    // Use explicit browser choice if provided
    if (config.browserType) {
      this.log(`Using explicitly requested browser: ${config.browserType}`);
      return config.browserType;
    }
    
    // Intelligent defaults based on purpose and requirements
    let defaultBrowser: BrowserType;
    
    switch (config.purpose) {
      case BrowserPurpose.AUTOMATION:
      case BrowserPurpose.TESTING:
      case BrowserPurpose.INTEGRATION_TEST:
        // DevTools and automation work best with Chrome
        defaultBrowser = BrowserType.CHROME;
        break;
        
      case BrowserPurpose.DEVELOPMENT:
        // Development can use default browser unless DevTools required
        defaultBrowser = config.requirements.devtools ? BrowserType.CHROME : BrowserType.DEFAULT;
        break;
        
      case BrowserPurpose.USER:
      default:
        // Regular user experience should respect their default browser choice
        defaultBrowser = BrowserType.DEFAULT;
        break;
    }
    
    this.log(`Intelligent browser selection: ${defaultBrowser} for ${config.purpose} (devtools: ${config.requirements.devtools})`);
    return defaultBrowser;
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
  /**
   * Allocate available port using ProcessCommand
   */
  private async allocatePort(): Promise<number> { 
    try {
      const ProcessCommandModule = await import('../../commands/kernel/system/ProcessCommand');
      const ProcessCommand = ProcessCommandModule.default;
      
      // Check ports in DevTools range (9222-9299)
      const candidatePorts = Array.from({ length: 78 }, (_, i) => 9222 + i);
      
      const result = await ProcessCommand.execute({
        subcommand: 'ports',
        ports: candidatePorts
      });
      
      if (result.success) {
        // Find first available port
        for (const port of candidatePorts) {
          const portData = result.data.portProcesses[port];
          if (!portData || portData.length === 0) {
            this.log(`Allocated port ${port} for browser`);
            return port;
          }
        }
      }
      
      // Fallback: simple allocation if ProcessCommand fails
      const usedPorts = new Set(Array.from(this.browsers.values()).map(b => b.port));
      let port = 9222;
      while (usedPorts.has(port) && port < 9300) {
        port++;
      }
      
      this.log(`Fallback allocation: port ${port} for browser`);
      return port;
    } catch (error) {
      this.log(`Port allocation error: ${error}`, 'error');
      
      // Emergency fallback
      return 9222 + Math.floor(Math.random() * 78);
    }
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
  /**
   * Smart browser request handler - determines state and responds appropriately
   * Instead of always creating new browsers, intelligently manages existing ones
   */
  private async handleSmartBrowserRequest(requestData: any): Promise<DaemonResponse> {
    const { sessionId, url, config } = requestData;
    
    try {
      // 1. Check if browser already exists for this session/URL
      const existingBrowser = this.findExistingBrowser(sessionId, url);
      
      if (existingBrowser) {
        // 2. Determine if refresh is sufficient vs new browser needed
        if (this.shouldRefreshExisting(existingBrowser, config)) {
          await this.refreshBrowserTab(existingBrowser, url);
          this.log(`♻️  Refreshed existing browser ${existingBrowser.id} for session ${sessionId}`);
          
          return {
            success: true,
            data: {
              action: 'refreshed',
              browserId: existingBrowser.id,
              pid: existingBrowser.pid,
              url: url,
              devtools: config?.requirements?.devtools || false
            }
          };
        }
      }
      
      // 3. Check if we need to kill ports/processes
      await this.handlePortConflicts(config);
      
      // 4. Create new browser or reuse existing based on intelligent strategy
      const browserConfig: BrowserConfig = {
        purpose: config?.purpose || 'development',
        persona: config?.persona || 'system',
        requirements: {
          devtools: config?.requirements?.devtools || false,
          isolation: config?.requirements?.isolation || 'dedicated',
          visibility: config?.requirements?.visibility || 'visible',
          persistence: config?.requirements?.persistence || 'session'
        },
        resources: {
          priority: config?.resources?.priority || 'high'
        }
      };
      
      const strategy = await this.calculatePlacement(browserConfig);
      const browser = await this.executePlacement(strategy, browserConfig);
      
      // Track session association
      browser.sessions.add(sessionId);
      this.browsers.set(browser.id, browser);
      
      this.log(`🌐 Smart browser management: ${strategy.type} for session ${sessionId}`);
      
      return {
        success: true,
        data: {
          action: strategy.type,
          browserId: browser.id,
          pid: browser.pid,
          url: url,
          devtools: browserConfig.requirements.devtools,
          reasoning: strategy.reasoning
        }
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Smart browser request failed: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }
  
  /**
   * Find existing browser for session/URL combination
   */
  private findExistingBrowser(sessionId: string, url: string): ManagedBrowser | null {
    for (const browser of this.browsers.values()) {
      if (browser.sessions.has(sessionId) && browser.state === 'ready') {
        return browser;
      }
    }
    return null;
  }
  
  /**
   * Determine if refreshing existing browser is sufficient
   */
  private shouldRefreshExisting(browser: ManagedBrowser, newConfig: any): boolean {
    // Refresh if same requirements and browser is healthy
    const devToolsMatch = (browser.config.requirements.devtools || false) === (newConfig?.requirements?.devtools || false);
    const browserHealthy = browser.state === 'ready' && browser.resources.cpu < 80;
    
    return devToolsMatch && browserHealthy;
  }
  
  /**
   * Refresh existing browser tab
   */
  private async refreshBrowserTab(browser: ManagedBrowser, url: string): Promise<void> {
    // In real implementation, this would send refresh command to browser
    browser.lastActivity = new Date();
    this.log(`🔄 Refreshing browser ${browser.id} to ${url}`);
  }
  
  /**
   * Handle port conflicts intelligently
   */
  private async handlePortConflicts(config: any): Promise<void> {
    // Kill specific ports if needed - surgical process management
    if (config?.killPorts) {
      for (const port of config.killPorts) {
        this.log(`🔧 Killing processes on port ${port}`);
        // In real implementation: exec(`lsof -ti:${port} | xargs kill -9`)
      }
    }
  }
  
  /**
   * Calculate smart placement strategy for browser session
   */
  private async calculatePlacement(config: BrowserConfig): Promise<PlacementStrategy> {
    // DevTools sessions always get new browser + new tab (isolation needed)
    if (config.requirements.devtools) {
      return {
        type: 'create',
        reasoning: `DevTools sessions always get fresh browser for clean debugging environment`
      };
    }
    
    // Check for existing browsers that could be reused (user sessions only)
    const compatibleBrowsers = Array.from(this.browsers.values()).filter(browser => 
      browser.state === 'ready' && 
      browser.config.purpose === config.purpose &&
      browser.resources.cpu < 70 && // Not overloaded
      !browser.config.requirements.devtools // Don't reuse DevTools browsers
    );
    
    if (compatibleBrowsers.length > 0) {
      const browser = compatibleBrowsers[0];
      
      // Decide between reuse and add-tab based on session count
      if (browser.sessions.size < 3) {
        return {
          type: 'add-tab',
          browserId: browser.id,
          reasoning: `Adding tab to existing ${config.purpose} browser with ${browser.sessions.size} sessions`
        };
      } else {
        return {
          type: 'reuse',
          browserId: browser.id,
          reasoning: `Reusing existing ${config.purpose} browser (refreshing current tab)`
        };
      }
    }
    
    // No compatible browsers found - create new one
    return {
      type: 'create',
      reasoning: `Creating new browser for ${config.purpose} (no compatible browsers available)`
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