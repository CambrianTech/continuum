/**
 * Browser Launcher Module - Focused browser process management
 * 
 * Responsibilities:
 * - Launch browser processes with proper configuration
 * - Monitor browser startup and readiness
 * - Handle browser process lifecycle
 * - Manage browser arguments and environment
 */

import { BrowserType, BrowserConfig, BrowserLaunchResult } from '../types/index.js';
import { IBrowserModule } from './IBrowserModule.js';

export class BrowserLauncher {
  private browserModules = new Map<BrowserType, IBrowserModule>();

  /**
   * Register a browser module for launching
   */
  registerModule(browserType: BrowserType, module: IBrowserModule): void {
    this.browserModules.set(browserType, module);
  }

  /**
   * Launch a browser with the given configuration
   */
  async launch(config: BrowserConfig, debugPort: number): Promise<BrowserLaunchResult> {
    const browserType = this.selectBrowserType(config);
    const module = this.getBrowserModule(browserType);
    
    if (!module) {
      throw new Error(`No browser module available for ${browserType}`);
    }

    // Launch using the appropriate module
    const result = await module.launch(config, debugPort);
    
    // Wait for browser to be ready
    await this.waitForBrowserReady(result, module);
    
    return result;
  }

  /**
   * Select appropriate browser type based on configuration
   */
  private selectBrowserType(config: BrowserConfig): BrowserType {
    // Use explicit browser choice if provided
    if (config.type) {
      return config.type;
    }

    // DevTools sessions require Chrome for best compatibility
    if (config.devtools) {
      return BrowserType.CHROME;
    }

    // Default to system default browser
    return BrowserType.DEFAULT;
  }

  /**
   * Get browser module for the specified type
   */
  private getBrowserModule(browserType: BrowserType): IBrowserModule | null {
    // Handle DEFAULT by using system's open() command
    if (browserType === BrowserType.DEFAULT) {
      return this.createSystemDefaultModule();
    }
    
    return this.browserModules.get(browserType) || null;
  }

  /**
   * Create system default module that uses open() command
   */
  private createSystemDefaultModule(): IBrowserModule {
    return {
      browserType: BrowserType.DEFAULT,
      capabilities: {
        supportsConsole: false,
        supportsNetwork: false,
        supportsPerformance: false,
        supportsScreenshot: false,
        supportsProfiling: false,
        supportsCodeCoverage: false,
        supportsSecurityAnalysis: false
      },
      async isAvailable(): Promise<boolean> { 
        return true; // System always has a default browser
      },
      async getBinaryPath(): Promise<string | null> { 
        return 'system'; 
      },
      buildLaunchArgs: () => [], // No args needed for open() command
      async launch() {
        const url = 'http://localhost:9000';
        
        let openCommand = 'open'; // macOS
        if (process.platform === 'linux') openCommand = 'xdg-open';
        if (process.platform === 'win32') openCommand = 'start';
        
        console.log(`🌐 Launching default browser with '${openCommand} ${url}'...`);
        
        const { spawn } = await import('child_process');
        const browserProcess = spawn(openCommand, [url], { 
          detached: true, 
          stdio: 'ignore'
        });
        
        browserProcess.unref();
        
        return {
          process: null, // System handles the browser process
          pid: browserProcess.pid || Math.random() * 10000,
          debugPort: 0, // No DevTools for system default
          devToolsUrl: undefined,
          capabilities: {
            supportsConsole: false,
            supportsNetwork: false,
            supportsPerformance: false,
            supportsScreenshot: false,
            supportsProfiling: false,
            supportsCodeCoverage: false,
            supportsSecurityAnalysis: false
          }
        };
      },
      async waitForReady(): Promise<void> {
        // For system default, just wait a moment for browser to start
        await new Promise(resolve => setTimeout(resolve, 1000));
      },
      async getTabAPI() {
        throw new Error('Tab management not supported for system default browser');
      },
      async terminate(): Promise<void> {
        // Cannot programmatically terminate system default browser
      },
      async isHealthy(): Promise<boolean> { 
        return true; 
      },
      async getMetrics() {
        return { tabs: 0, memory: 0, cpu: 0, devToolsConnections: 0 };
      }
    };
  }

  /**
   * Wait for browser to be ready for connections
   */
  private async waitForBrowserReady(
    result: BrowserLaunchResult, 
    module: IBrowserModule
  ): Promise<void> {
    // Use module-specific readiness check
    await module.waitForReady({
      id: 'temp-id',
      type: module.browserType,
      pid: result.pid,
      debugPort: result.debugPort,
      status: 'launching',
      devToolsUrl: result.devToolsUrl,
      launchedAt: new Date(),
      lastActivity: new Date(),
      config: {} as BrowserConfig
    });
  }
}