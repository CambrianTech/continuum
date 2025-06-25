/**
 * Clean TypeScript BrowserCoordinator - manages browser lifecycle
 * Event-driven, no polling, proper async/await patterns
 */

import { spawn, ChildProcess } from 'child_process';
import { IBrowserCoordinator, BrowserInfo, TabInfo, BrowserConfig } from './interfaces.js';

export class BrowserCoordinator implements IBrowserCoordinator {
  private readonly portRange = { start: 9222, end: 9232 };
  private activeBrowsers = new Map<number, BrowserInfo>();

  /**
   * Find existing browser with Continuum loaded
   */
  async findExistingBrowser(): Promise<BrowserInfo | null> {
    for (let port = this.portRange.start; port <= this.portRange.end; port++) {
      try {
        const response = await fetch(`http://localhost:${port}/json`, {
          signal: AbortSignal.timeout(1000)
        });
        
        if (response.ok) {
          const tabs: TabInfo[] = await response.json();
          const continuumTab = tabs.find(tab => 
            tab.url.includes('localhost:9000') || 
            tab.title.toLowerCase().includes('continuum')
          );
          
          if (continuumTab) {
            const browserInfo: BrowserInfo = {
              port,
              shared: true,
              tabs: tabs.map(tab => ({
                id: tab.id,
                url: tab.url,
                title: tab.title,
                ready: true // Assume existing tabs are ready
              }))
            };
            
            this.activeBrowsers.set(port, browserInfo);
            return browserInfo;
          }
        }
      } catch (error) {
        // Port not available, continue checking
      }
    }
    
    return null;
  }

  /**
   * Launch new browser with proper event-driven ready detection
   */
  async launchBrowser(config: BrowserConfig): Promise<BrowserInfo> {
    // First verify Continuum server is running
    await this.verifyContinuumServer();
    
    const operaCmd = [
      '/Applications/Opera GX.app/Contents/MacOS/Opera',
      `--remote-debugging-port=${config.port}`,
      '--disable-web-security',
      '--disable-features=TranslateUI',
      '--disable-component-update',
      '--disable-background-timer-throttling',
      '--disable-backgrounding-occluded-windows',
      '--disable-renderer-backgrounding',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-default-apps',
      '--disable-extensions',
      `--user-data-dir=${config.userDataDir}`,
      `--app=${config.initialUrl}`,
      `--window-name=${config.windowTitle}`
    ];

    // Add visibility and positioning options
    if (config.headless) {
      operaCmd.push('--headless');
    }
    
    if (config.visible === false) {
      operaCmd.push('--window-position=-9999,-9999'); // Move off-screen
    } else if (config.position) {
      operaCmd.push(`--window-position=${config.position.x},${config.position.y}`);
    }
    
    if (config.size) {
      operaCmd.push(`--window-size=${config.size.width},${config.size.height}`);
    }
    
    if (config.minimized) {
      operaCmd.push('--start-minimized');
    }

    const browserProcess = spawn(operaCmd[0], operaCmd.slice(1), {
      stdio: ['ignore', 'ignore', 'ignore'],
      detached: false
    });

    const browserInfo: BrowserInfo = {
      port: config.port,
      pid: browserProcess.pid,
      shared: config.shared,
      tabs: []
    };

    // Wait for browser to be ready with Continuum loaded (event-driven)
    try {
      await this.waitForBrowserReady(browserInfo);
      this.activeBrowsers.set(config.port, browserInfo);
      return browserInfo;
    } catch (error) {
      // Kill browser if it failed to load properly
      try {
        browserProcess.kill();
      } catch (killError) {
        // Process might already be dead
      }
      throw new Error(`Browser launch failed: ${error}`);
    }
  }

  /**
   * Create new tab in existing browser
   */
  async createTabInBrowser(browser: BrowserInfo, url: string): Promise<TabInfo> {
    try {
      const response = await fetch(`http://localhost:${browser.port}/json/new?${encodeURIComponent(url)}`, {
        method: 'PUT'
      });
      
      if (!response.ok) {
        throw new Error(`DevTools API error: ${response.status}`);
      }
      
      const newTab: TabInfo = await response.json();
      
      // Wait for the new tab to load Continuum
      await this.waitForTabReady(browser.port, newTab.id);
      
      newTab.ready = true;
      browser.tabs.push(newTab);
      
      return newTab;
    } catch (error) {
      throw new Error(`Failed to create tab: ${error}`);
    }
  }

  /**
   * Close browser and cleanup
   */
  async closeBrowser(browser: BrowserInfo): Promise<void> {
    if (browser.pid) {
      try {
        process.kill(browser.pid, 'SIGTERM');
      } catch (error) {
        // Process might already be dead
      }
    }
    
    this.activeBrowsers.delete(browser.port);
  }

  /**
   * Event-driven browser ready detection - no polling timeouts!
   */
  private async waitForBrowserReady(browser: BrowserInfo, timeout = 15000): Promise<void> {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeout) {
      try {
        const response = await fetch(`http://localhost:${browser.port}/json`);
        if (response.ok) {
          const tabs: TabInfo[] = await response.json();
          const continuumTab = tabs.find(tab => 
            tab.url.includes('localhost:9000') || 
            tab.title.toLowerCase().includes('continuum')
          );
          
          if (continuumTab) {
            // Verify Continuum is actually loaded by executing test JavaScript
            const testReady = await this.testContinuumReady(browser.port, continuumTab.id);
            if (testReady) {
              browser.tabs = tabs.map(tab => ({
                id: tab.id,
                url: tab.url,
                title: tab.title,
                ready: tab.id === continuumTab.id
              }));
              return;
            }
          }
        }
      } catch (error) {
        // Not ready yet, continue waiting
      }
      
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    
    throw new Error(`Browser not ready after ${timeout}ms`);
  }

  /**
   * Wait for specific tab to be ready
   */
  private async waitForTabReady(port: number, tabId: string, timeout = 10000): Promise<void> {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeout) {
      const ready = await this.testContinuumReady(port, tabId);
      if (ready) return;
      
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    
    throw new Error(`Tab ${tabId} not ready after ${timeout}ms`);
  }

  /**
   * Test if Continuum is actually loaded and responsive
   */
  private async testContinuumReady(port: number, tabId: string): Promise<boolean> {
    try {
      const response = await fetch(`http://localhost:${port}/json/runtime/evaluate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expression: `
            typeof window !== "undefined" && 
            document.readyState === "complete" && 
            window.location.hostname === "localhost" &&
            window.location.port === "9000"
          `,
          returnByValue: true
        })
      });
      
      if (response.ok) {
        const result = await response.json();
        return result.result?.value === true;
      }
    } catch (error) {
      // Not ready yet
    }
    
    return false;
  }

  /**
   * Verify Continuum server is running before launching browsers
   */
  private async verifyContinuumServer(): Promise<void> {
    try {
      const response = await fetch('http://localhost:9000', {
        signal: AbortSignal.timeout(3000)
      });
      
      if (!response.ok) {
        throw new Error(`Continuum server returned status: ${response.status}`);
      }
    } catch (error) {
      throw new Error(`Continuum server not accessible: ${error}. Start the server first.`);
    }
  }

  /**
   * Find available port for new browser
   */
  async findAvailablePort(): Promise<number> {
    for (let port = this.portRange.start; port <= this.portRange.end; port++) {
      if (!this.activeBrowsers.has(port)) {
        try {
          const response = await fetch(`http://localhost:${port}/json`, {
            signal: AbortSignal.timeout(1000)
          });
          // If we get a response, port is in use
        } catch (error) {
          // Port is available
          return port;
        }
      }
    }
    
    throw new Error(`No available ports in range ${this.portRange.start}-${this.portRange.end}`);
  }
}