/**
 * Chrome Browser Module - Chrome/Chromium integration with DevTools Protocol
 * 
 * Handles:
 * - Chrome DevTools Protocol (CDP) integration
 * - Headless and headed mode support
 * - Remote debugging configuration
 * - Tab management via CDP
 * - Process monitoring and health checks
 */

import { IBrowserModule, DevToolsCapabilities, BrowserLaunchResult, TabManagementAPI } from './IBrowserModule';
import { BrowserConfig, ManagedBrowser, BrowserType, BrowserVisibility, BrowserIsolation } from '../BrowserManagerDaemon';
import { ChromiumDevToolsAdapter } from '../adapters/ChromiumDevToolsAdapter';
import { spawn, ChildProcess } from 'child_process';

export class ChromeBrowserModule implements IBrowserModule {
  readonly browserType = BrowserType.CHROME;
  readonly capabilities: DevToolsCapabilities = {
    protocol: 'chrome-devtools',
    supportsHeadless: true,
    supportsRemoteDebugging: true,
    supportsAutomation: true,
    supportsExtensions: true,
    defaultPort: 9222,
    portRange: [9222, 9299]
  };

  async isAvailable(): Promise<boolean> {
    const binaryPath = await this.getBinaryPath();
    return binaryPath !== null;
  }

  async getBinaryPath(): Promise<string | null> {
    try {
      // Try common Chrome installation paths
      const chromePaths = [
        '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', // macOS
        '/usr/bin/google-chrome',                                        // Linux
        '/usr/bin/chromium-browser',                                     // Linux Chromium
        'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'     // Windows
      ];
      
      const fs = await import('fs');
      for (const path of chromePaths) {
        if (fs.existsSync(path)) {
          return path;
        }
      }
      
      // Try which/where command
      try {
        const { exec } = await import('child_process');
        const { promisify } = await import('util');
        const execAsync = promisify(exec);
        
        const { stdout } = await execAsync('which google-chrome || which chromium-browser || where chrome.exe');
        const path = stdout.trim();
        if (path && fs.existsSync(path)) {
          return path;
        }
      } catch (error) {
        // Continue to return null
      }
      
      return null;
    } catch (error) {
      console.error('Chrome binary detection failed:', error);
      return null;
    }
  }

  buildLaunchArgs(config: BrowserConfig, debugPort: number): string[] {
    const args = [
      // Core Chrome arguments
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-default-apps',
      '--disable-popup-blocking',
      '--disable-translate',
      '--disable-background-timer-throttling',
      '--disable-renderer-backgrounding',
      '--disable-device-discovery-notifications',
      '--disable-backgrounding-occluded-windows',
      
      // DevTools configuration
      `--remote-debugging-port=${debugPort}`,
      '--enable-devtools-experiments',
      '--disable-web-security', // For local development
      
      // Performance optimizations
      '--disable-background-timer-throttling',
      '--disable-features=TranslateUI',
      '--disable-ipc-flooding-protection'
    ];

    // DevTools specific arguments
    if (config.requirements.devtools) {
      args.push(
        '--auto-open-devtools-for-tabs',
        '--disable-features=VizDisplayCompositor'
      );
    }

    // Isolation configuration
    switch (config.requirements.isolation) {
      case BrowserIsolation.SANDBOXED:
        args.push('--no-sandbox', '--disable-setuid-sandbox');
        break;
      case BrowserIsolation.DEDICATED:
        args.push(`--user-data-dir=/tmp/continuum-chrome-${Date.now()}`);
        break;
      case BrowserIsolation.SHARED:
        // Use default user data directory
        break;
    }

    // Visibility configuration
    switch (config.requirements.visibility) {
      case BrowserVisibility.HIDDEN:
        args.push('--headless=new');
        break;
      case BrowserVisibility.MINIMIZED:
        args.push('--start-minimized');
        break;
      case BrowserVisibility.VISIBLE:
        // Default visible mode
        break;
    }

    // Resource limits
    if (config.resources.memoryLimit) {
      args.push(`--max_old_space_size=${config.resources.memoryLimit}`);
    }

    // Extensions support
    if (!config.requirements.extensions) {
      args.push('--disable-extensions');
    }

    // Add target URL
    args.push('http://localhost:9000');

    return args;
  }

  async launch(config: BrowserConfig, debugPort: number): Promise<BrowserLaunchResult> {
    const binaryPath = await this.getBinaryPath();
    if (!binaryPath) {
      throw new Error('Chrome binary not found on system');
    }

    const args = this.buildLaunchArgs(config, debugPort);
    
    try {
      const process = spawn(binaryPath, args, {
        detached: false,
        stdio: 'pipe'
      });

      if (!process.pid) {
        throw new Error('Failed to spawn Chrome process');
      }

      return {
        process,
        pid: process.pid,
        debugPort,
        devToolsUrl: `http://localhost:${debugPort}`,
        capabilities: this.capabilities
      };
    } catch (error) {
      throw new Error(`Chrome launch failed: ${error}`);
    }
  }

  async waitForReady(browser: ManagedBrowser): Promise<void> {
    return new Promise((resolve, reject) => {
      const maxAttempts = 20;
      let attempts = 0;
      
      // Timeout fallback
      const timeoutMs = 30000;
      const fallbackTimeout = setTimeout(() => {
        reject(new Error(`Chrome browser ${browser.id} failed to become ready within ${timeoutMs}ms`));
      }, timeoutMs);

      const checkReady = async () => {
        attempts++;
        
        try {
          // Check if DevTools port is responsive
          const response = await fetch(`http://localhost:${browser.port}/json/version`, {
            signal: AbortSignal.timeout(2000)
          });
          
          if (response.ok) {
            const version = await response.json();
            if (version.Browser && version.Browser.includes('Chrome')) {
              clearTimeout(fallbackTimeout);
              browser.state = 'ready';
              console.log(`✅ Chrome browser ${browser.id} ready on port ${browser.port}`);
              resolve();
              return;
            }
          }
        } catch (error) {
          // DevTools not ready yet
        }
        
        if (attempts >= maxAttempts) {
          clearTimeout(fallbackTimeout);
          reject(new Error(`Chrome browser ${browser.id} DevTools not responsive after ${maxAttempts} attempts`));
          return;
        }
        
        // Exponential backoff
        const backoffMs = Math.min(500 * Math.pow(1.3, attempts), 3000);
        setTimeout(checkReady, backoffMs);
      };
      
      checkReady();
    });
  }

  async getTabAPI(browser: ManagedBrowser): Promise<TabManagementAPI> {
    const baseUrl = `http://localhost:${browser.port}`;
    
    return {
      async createTab(url: string): Promise<string> {
        const response = await fetch(`${baseUrl}/json/new?${encodeURIComponent(url)}`);
        const tab = await response.json();
        return tab.id;
      },
      
      async closeTab(tabId: string): Promise<void> {
        await fetch(`${baseUrl}/json/close/${tabId}`);
      },
      
      async refreshTab(tabId: string): Promise<void> {
        // Send reload command via DevTools Protocol
        const response = await fetch(`${baseUrl}/json/runtime/evaluate`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            expression: 'location.reload()',
            contextId: tabId
          })
        });
      },
      
      async navigateTab(tabId: string, url: string): Promise<void> {
        const response = await fetch(`${baseUrl}/json/runtime/evaluate`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            expression: `location.href = '${url}'`,
            contextId: tabId
          })
        });
      },
      
      async listTabs(): Promise<Array<{ id: string; url: string; title: string }>> {
        const response = await fetch(`${baseUrl}/json`);
        const tabs = await response.json();
        return tabs.map((tab: any) => ({
          id: tab.id,
          url: tab.url,
          title: tab.title
        }));
      }
    };
  }

  async terminate(browser: ManagedBrowser): Promise<void> {
    try {
      // Gracefully close via DevTools first
      await fetch(`http://localhost:${browser.port}/json/close`, { method: 'POST' });
      
      // Wait a moment, then force kill if needed
      setTimeout(() => {
        try {
          process.kill(browser.pid, 'SIGTERM');
        } catch (error) {
          // Process might already be dead
        }
      }, 2000);
      
    } catch (error) {
      // Force kill immediately
      try {
        process.kill(browser.pid, 'SIGKILL');
      } catch (killError) {
        // Process might already be dead
      }
    }
  }

  async isHealthy(browser: ManagedBrowser): Promise<boolean> {
    try {
      const response = await fetch(`http://localhost:${browser.port}/json/version`, {
        signal: AbortSignal.timeout(5000)
      });
      return response.ok;
    } catch (error) {
      return false;
    }
  }

  async getMetrics(browser: ManagedBrowser): Promise<{
    tabs: number;
    memory: number;
    cpu: number;
    devToolsConnections: number;
  }> {
    try {
      // Get tab count
      const tabsResponse = await fetch(`http://localhost:${browser.port}/json`);
      const tabs = await tabsResponse.json();
      
      // Get memory/CPU from system (would use ProcessCommand in real implementation)
      return {
        tabs: tabs.length,
        memory: 0, // TODO: Get from ProcessCommand
        cpu: 0,    // TODO: Get from ProcessCommand  
        devToolsConnections: 1 // TODO: Count active DevTools connections
      };
    } catch (error) {
      return {
        tabs: 0,
        memory: 0,
        cpu: 0,
        devToolsConnections: 0
      };
    }
  }
}