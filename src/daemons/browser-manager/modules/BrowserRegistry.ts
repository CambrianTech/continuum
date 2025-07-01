/**
 * Browser Registry - Focused module for browser type detection and module management
 * Handles browser discovery, module initialization, and intelligent browser selection
 */

import { BrowserType, BrowserConfig, BrowserPurpose } from '../types/index.js';
import { IBrowserModule } from './IBrowserModule';
import { ChromeBrowserModule } from './ChromeBrowserModule';

export class BrowserRegistry {
  private browserModules = new Map<BrowserType, IBrowserModule>();
  private initialized = false;

  /**
   * Initialize all available browser modules
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    // Register Chrome browser module
    const chromeModule = new ChromeBrowserModule();
    if (await chromeModule.isAvailable()) {
      this.browserModules.set(BrowserType.CHROME, chromeModule);
      console.log('✅ Chrome browser module available');
    } else {
      console.log('⚠️ Chrome browser module not available');
    }
    
    // Check for Opera GX specifically
    const fs = await import('fs');
    if (fs.existsSync('/Applications/Opera GX.app/Contents/MacOS/Opera')) {
      // For now, use Chrome module for Opera GX (same Chromium base, same DevTools Protocol)
      this.browserModules.set(BrowserType.CHROMIUM, chromeModule);
      console.log('✅ Opera GX browser module available (using Chrome DevTools Protocol)');
    }
    
    // TODO: Add other browser modules (Firefox, Safari, Edge)
    // this.browserModules.set(BrowserType.FIREFOX, new FirefoxBrowserModule());
    // this.browserModules.set(BrowserType.SAFARI, new SafariBrowserModule());
    // this.browserModules.set(BrowserType.EDGE, new EdgeBrowserModule());

    this.initialized = true;
  }

  /**
   * Get browser module for browser type
   */
  getBrowserModule(browserType: BrowserType): IBrowserModule | null {
    // Handle DEFAULT by returning null (BrowserLauncher handles system default)
    if (browserType === BrowserType.DEFAULT) {
      return null;
    }
    
    // Return the registered browser module
    return this.browserModules.get(browserType) || null;
  }

  /**
   * Get all available browser types
   */
  getAvailableBrowsers(): BrowserType[] {
    return Array.from(this.browserModules.keys());
  }

  /**
   * Check if browser type is available
   */
  isAvailable(browserType: BrowserType): boolean {
    if (browserType === BrowserType.DEFAULT) {
      return true; // System always has a default browser
    }
    return this.browserModules.has(browserType);
  }

  /**
   * Intelligent browser type selection based on config
   */
  selectBrowserType(config: BrowserConfig): BrowserType {
    // Use explicit browser choice if provided
    if (config.browserType) {
      console.log(`Using explicitly requested browser: ${config.browserType}`);
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
        // Development uses default browser (user's choice) unless DevTools specifically required
        // This respects user's browser preference (Opera GX in this case)
        defaultBrowser = config.requirements?.devtools ? BrowserType.CHROME : this.getSystemDefaultBrowser();
        break;
        
      case BrowserPurpose.USER:
      default:
        // Always respect user's default browser choice for user sessions
        defaultBrowser = this.getSystemDefaultBrowser();
        break;
    }
    
    console.log(`Intelligent browser selection: ${defaultBrowser} for ${config.purpose} (devtools: ${config.requirements?.devtools})`);
    return defaultBrowser;
  }

  /**
   * Detect system default browser
   */
  async detectSystemDefaultBrowser(): Promise<BrowserType | null> {
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
            const bundleMap: Record<string, BrowserType> = {
              'com.google.chrome': BrowserType.CHROME,
              'org.mozilla.firefox': BrowserType.FIREFOX,
              'com.apple.safari': BrowserType.SAFARI,
              'com.microsoft.edgemac': BrowserType.EDGE,
              'com.operasoftware.opera': BrowserType.OPERA
            };
            
            const browserType = bundleMap[bundleId];
            if (browserType) {
              console.log(`Detected default browser: ${browserType} (${bundleId})`);
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
          
          if (desktopFile.includes('chrome')) return BrowserType.CHROME;
          if (desktopFile.includes('firefox')) return BrowserType.FIREFOX;
          if (desktopFile.includes('safari')) return BrowserType.SAFARI;
          if (desktopFile.includes('edge')) return BrowserType.EDGE;
          if (desktopFile.includes('opera')) return BrowserType.OPERA;
        } catch (error) {
          // Continue to fallback
        }
      }
      
      // Universal fallback: Look for most common browsers in order of preference
      const preferenceOrder = [BrowserType.CHROME, BrowserType.FIREFOX, BrowserType.SAFARI, BrowserType.EDGE, BrowserType.OPERA];
      
      for (const browserType of preferenceOrder) {
        if (this.isAvailable(browserType)) {
          console.log(`Using fallback browser: ${browserType}`);
          return browserType;
        }
      }
      
      console.log('No default browser detected, and no fallback browsers available');
      return null;
    } catch (error) {
      console.log(`Failed to detect default browser: ${error}`);
      return null;
    }
  }

  /**
   * Get system default browser (always returns DEFAULT for system handling)
   */
  private getSystemDefaultBrowser(): BrowserType {
    // Always return DEFAULT - let the system handle which browser to open
    return BrowserType.DEFAULT;
  }

  /**
   * Get browser capabilities
   */
  getBrowserCapabilities(browserType: BrowserType): any {
    const module = this.getBrowserModule(browserType);
    return module?.capabilities || {
      protocol: 'none',
      supportsHeadless: false,
      supportsRemoteDebugging: false,
      supportsAutomation: false,
      supportsExtensions: false,
      defaultPort: 0,
      portRange: [0, 0]
    };
  }
}