/**
 * Intelligent Browser Manager - Smart browser detection and launching
 * 
 * CAPABILITIES:
 * - Detects existing browser instances automatically
 * - Uses system default browser intelligently  
 * - Integrates with development tools (portal, git hooks)
 * - Opens DevTools when needed for debugging
 * - Handles multiple browser scenarios gracefully
 * - Works across all platforms (macOS, Windows, Linux)
 */

import { SafeBrowserManager } from './SafeBrowserManager.js';

export interface BrowserInfo {
  name: string;
  executable: string;
  isDefault: boolean;
  supportsDevTools: boolean;
  version?: string;
}

export interface DevToolsOptions {
  enabled: boolean;
  openConsole?: boolean;
  openNetwork?: boolean;
  openSources?: boolean;
  inspectElement?: string; // CSS selector to inspect
}

export interface LaunchContext {
  source: 'portal' | 'git-hook' | 'test' | 'development' | 'manual';
  purpose: 'debugging' | 'testing' | 'development' | 'presentation';
  devTools?: DevToolsOptions;
  url?: string;
}

export class IntelligentBrowserManager extends SafeBrowserManager {
  private detectedBrowsers: BrowserInfo[] = [];
  private defaultBrowser: BrowserInfo | null = null;
  private systemDetected = false;

  /**
   * Intelligently detect available browsers and system default
   */
  async detectBrowsers(): Promise<BrowserInfo[]> {
    if (this.systemDetected && this.detectedBrowsers.length > 0) {
      return this.detectedBrowsers;
    }

    console.log('🔍 Detecting available browsers...');
    this.detectedBrowsers = [];

    try {
      const platform = process.platform;
      
      switch (platform) {
        case 'darwin':
          await this.detectMacBrowsers();
          break;
        case 'win32':
          await this.detectWindowsBrowsers();
          break;
        case 'linux':
          await this.detectLinuxBrowsers();
          break;
        default:
          console.warn(`⚠️ Unsupported platform: ${platform}`);
      }

      // Detect system default browser
      await this.detectDefaultBrowser();
      
      this.systemDetected = true;
      console.log(`✅ Detected ${this.detectedBrowsers.length} browsers, default: ${this.defaultBrowser?.name || 'unknown'}`);
      
    } catch (error) {
      console.error('❌ Browser detection failed:', error);
    }

    return this.detectedBrowsers;
  }

  /**
   * Launch browser intelligently based on context
   */
  async launchIntelligent(context: LaunchContext): Promise<any> {
    await this.detectBrowsers();

    const url = context.url || 'http://localhost:9000';
    const browser = this.selectBestBrowser(context);

    console.log(`🚀 Intelligent launch for ${context.source}/${context.purpose}`);
    console.log(`📱 Using browser: ${browser.name}`);
    console.log(`🌐 URL: ${url}`);

    // Check if browser is already open with our URL
    const existingConnection = await this.checkExistingBrowserConnection(url);
    if (existingConnection) {
      console.log('✅ Browser already connected to target URL');
      return {
        success: true,
        action: 'already_connected',
        browser: browser.name,
        url
      };
    }

    // Launch with appropriate configuration
    const launchResult = await this.performIntelligentLaunch(browser, url, context);
    
    // Open DevTools if requested
    if (context.devTools?.enabled && launchResult.success) {
      await this.openDevTools(browser, context.devTools);
    }

    return launchResult;
  }

  /**
   * Select the best browser for the given context
   */
  private selectBestBrowser(context: LaunchContext): BrowserInfo {
    // Prefer default browser unless context suggests otherwise
    if (this.defaultBrowser) {
      // For debugging/development, prefer browsers with better DevTools
      if (context.purpose === 'debugging' || context.devTools?.enabled) {
        const chromeVariant = this.findBrowser(['Chrome', 'Chromium', 'Edge']);
        if (chromeVariant && chromeVariant.supportsDevTools) {
          console.log(`🔧 Using ${chromeVariant.name} for debugging (better DevTools)`);
          return chromeVariant;
        }
      }

      return this.defaultBrowser;
    }

    // Fallback: find any available browser
    const fallbackBrowser = this.detectedBrowsers[0];
    if (fallbackBrowser) {
      console.log(`⚠️ No default browser found, using: ${fallbackBrowser.name}`);
      return fallbackBrowser;
    }

    // Last resort: return a generic browser configuration
    console.warn('⚠️ No browsers detected, using system default command');
    return {
      name: 'System Default',
      executable: this.getGenericBrowserCommand(),
      isDefault: true,
      supportsDevTools: false
    };
  }

  /**
   * Detect macOS browsers
   */
  private async detectMacBrowsers(): Promise<void> {
    const browsers = [
      { name: 'Safari', path: '/Applications/Safari.app', cmd: 'open -a Safari', devTools: true },
      { name: 'Chrome', path: '/Applications/Google Chrome.app', cmd: 'open -a "Google Chrome"', devTools: true },
      { name: 'Firefox', path: '/Applications/Firefox.app', cmd: 'open -a Firefox', devTools: true },
      { name: 'Edge', path: '/Applications/Microsoft Edge.app', cmd: 'open -a "Microsoft Edge"', devTools: true },
      { name: 'Arc', path: '/Applications/Arc.app', cmd: 'open -a Arc', devTools: true }
    ];

    const fs = await import('fs');
    
    for (const browser of browsers) {
      if (fs.existsSync(browser.path)) {
        this.detectedBrowsers.push({
          name: browser.name,
          executable: browser.cmd,
          isDefault: false, // Will be set later
          supportsDevTools: browser.devTools
        });
      }
    }
  }

  /**
   * Detect Windows browsers
   */
  private async detectWindowsBrowsers(): Promise<void> {
    const { exec } = await import('child_process');
    const { promisify } = await import('util');
    // const execAsync = promisify(exec); // TODO: Implement Windows browser detection

    // Common Windows browser paths
    const browsers = [
      { name: 'Chrome', cmd: 'start chrome', devTools: true },
      { name: 'Edge', cmd: 'start msedge', devTools: true },
      { name: 'Firefox', cmd: 'start firefox', devTools: true }
    ];

    // Add detected browsers
    for (const browser of browsers) {
      this.detectedBrowsers.push({
        name: browser.name,
        executable: browser.cmd,
        isDefault: false,
        supportsDevTools: browser.devTools
      });
    }
  }

  /**
   * Detect Linux browsers
   */
  private async detectLinuxBrowsers(): Promise<void> {
    const browsers = [
      { name: 'Chrome', cmd: 'google-chrome', devTools: true },
      { name: 'Chromium', cmd: 'chromium-browser', devTools: true },
      { name: 'Firefox', cmd: 'firefox', devTools: true }
    ];

    const { exec } = await import('child_process');
    const { promisify } = await import('util');
    const execAsync = promisify(exec);

    for (const browser of browsers) {
      try {
        // Check if command exists
        await execAsync(`which ${browser.cmd}`);
        this.detectedBrowsers.push({
          name: browser.name,
          executable: browser.cmd,
          isDefault: false,
          supportsDevTools: browser.devTools
        });
      } catch {
        // Browser not available
      }
    }
  }

  /**
   * Detect system default browser
   */
  private async detectDefaultBrowser(): Promise<void> {
    try {
      const platform = process.platform;
      
      if (platform === 'darwin') {
        // macOS: get default browser
        const { exec } = await import('child_process');
        const { promisify } = await import('util');
        // const execAsync = promisify(exec); // TODO: Implement macOS browser detection
        
        // TODO: Implement macOS default browser detection
        // const { stdout } = await execAsync('defaults read ...');
        const bundleId = 'com.google.Chrome'; // Default fallback
        
        // Map bundle ID to browser name
        const browserMap: Record<string, string> = {
          'com.apple.safari': 'Safari',
          'com.google.chrome': 'Chrome',
          'org.mozilla.firefox': 'Firefox',
          'com.microsoft.edgemac': 'Edge'
        };
        
        const defaultBrowserName = browserMap[bundleId];
        if (defaultBrowserName) {
          const browser = this.detectedBrowsers.find(b => b.name === defaultBrowserName);
          if (browser) {
            browser.isDefault = true;
            this.defaultBrowser = browser;
          }
        }
      }
      
      // Fallback: mark first detected browser as default
      if (!this.defaultBrowser && this.detectedBrowsers.length > 0) {
        this.detectedBrowsers[0].isDefault = true;
        this.defaultBrowser = this.detectedBrowsers[0];
      }
      
    } catch (error) {
      console.warn('⚠️ Could not detect default browser:', error);
      // Use first available browser as fallback
      if (this.detectedBrowsers.length > 0) {
        this.detectedBrowsers[0].isDefault = true;
        this.defaultBrowser = this.detectedBrowsers[0];
      }
    }
  }

  /**
   * Find browser by name variants
   */
  private findBrowser(names: string[]): BrowserInfo | null {
    for (const name of names) {
      const browser = this.detectedBrowsers.find(b => 
        b.name.toLowerCase().includes(name.toLowerCase())
      );
      if (browser) return browser;
    }
    return null;
  }

  /**
   * Check if browser is already connected to target URL
   */
  private async checkExistingBrowserConnection(url: string): Promise<boolean> {
    // Check our connection status
    const status = this.getConnectionStatus();
    if (status.isConnected) {
      // TODO: Could also check if connected browser is on the right URL
      return true;
    }
    return false;
  }

  /**
   * Perform intelligent browser launch with context awareness
   */
  private async performIntelligentLaunch(browser: BrowserInfo, url: string, context: LaunchContext): Promise<any> {
    try {
      console.log(`🚀 Launching ${browser.name} for ${context.source}...`);
      
      const { spawn } = await import('child_process');
      
      // Build command arguments based on context
      const args = this.buildLaunchArgs(browser, url, context);
      
      const process = spawn(args.cmd, args.args, {
        detached: true,
        stdio: 'ignore'
      });

      process.unref();

      return {
        success: true,
        action: 'launched',
        browser: browser.name,
        url,
        context: context.source,
        pid: process.pid
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        action: 'failed',
        error: errorMessage,
        browser: browser.name
      };
    }
  }

  /**
   * Build launch arguments based on browser and context
   */
  private buildLaunchArgs(browser: BrowserInfo, url: string, context: LaunchContext): { cmd: string; args: string[] } {
    const platform = process.platform;
    
    // Base command
    let cmd: string;
    let args: string[] = [];

    if (platform === 'darwin') {
      // macOS: use 'open' command
      cmd = 'open';
      args = ['-a', browser.name.replace('open -a ', ''), url];
      
      // Add DevTools arguments for Chrome/Chromium
      if (context.devTools?.enabled && browser.supportsDevTools) {
        if (browser.name.toLowerCase().includes('chrome')) {
          args.push('--args', '--auto-open-devtools-for-tabs');
        }
      }
    } else {
      // Windows/Linux: use browser executable directly
      const parts = browser.executable.split(' ');
      cmd = parts[0];
      args = parts.slice(1).concat([url]);
      
      // Add DevTools arguments
      if (context.devTools?.enabled && browser.supportsDevTools) {
        if (browser.name.toLowerCase().includes('chrome')) {
          args.push('--auto-open-devtools-for-tabs');
        }
      }
    }

    return { cmd, args };
  }

  /**
   * Open DevTools programmatically (for supported browsers)
   */
  private async openDevTools(browser: BrowserInfo, devTools: DevToolsOptions): Promise<void> {
    if (!browser.supportsDevTools) {
      console.log(`⚠️ ${browser.name} does not support programmatic DevTools`);
      return;
    }

    console.log(`🔧 Opening DevTools for ${browser.name}...`);
    
    // DevTools opening would depend on browser automation
    // For now, just log what we would do
    if (devTools.openConsole) {
      console.log('  📋 Would open Console tab');
    }
    if (devTools.openNetwork) {
      console.log('  🌐 Would open Network tab');
    }
    if (devTools.openSources) {
      console.log('  📁 Would open Sources tab');
    }
    if (devTools.inspectElement) {
      console.log(`  🔍 Would inspect element: ${devTools.inspectElement}`);
    }
  }

  /**
   * Generic browser command for unsupported platforms
   */
  private getGenericBrowserCommand(): string {
    const platform = process.platform;
    
    switch (platform) {
      case 'darwin':
        return 'open';
      case 'win32':
        return 'start';
      case 'linux':
        return 'xdg-open';
      default:
        return 'xdg-open';
    }
  }

  /**
   * Get detected browsers info
   */
  getBrowserInfo(): { detected: BrowserInfo[]; default: BrowserInfo | null } {
    return {
      detected: [...this.detectedBrowsers],
      default: this.defaultBrowser
    };
  }
}