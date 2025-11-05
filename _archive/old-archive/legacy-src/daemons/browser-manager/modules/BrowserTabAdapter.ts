/**
 * Browser Tab Adapter - Platform-specific tab management
 * Inspired by claude/testing-autonomy but adapted for our TypeScript daemon architecture
 */

import { platform } from 'os';
import { appleScriptExecutor } from './AppleScriptExecutor';

export interface TabCloseResult {
  browser: string;
  closed: number;
  error?: string;
}

export abstract class BaseBrowserAdapter {
  constructor(protected browserName: string) {}
  
  abstract closeTabs(_urlPattern: string): Promise<number>;
  abstract countTabs(_urlPattern: string): Promise<number>;
  abstract focusTab(_urlPattern: string): Promise<boolean>;
  
  protected extractDomain(url: string): string {
    try {
      return new URL(url).hostname;
    } catch {
      return url;
    }
  }

  /**
   * Execute AppleScript - can be mocked in tests
   */
  protected async execAsync(scriptFile: string, functionName: string, args: string[]): Promise<{ stdout: string }> {
    const result = await appleScriptExecutor.executeScript(scriptFile, functionName, args);
    return { stdout: result.stdout };
  }
}

/**
 * macOS Opera Adapter using AppleScript
 */
export class MacOperaAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Opera GX');
  }
  
  async closeTabs(_urlPattern: string): Promise<number> {
    try {
      const { stdout } = await this.execAsync('OperaTabManager.applescript', 'closeTabs', [_urlPattern]);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    try {
      const { stdout } = await this.execAsync('OperaTabManager.applescript', 'countTabs', [_urlPattern]);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    try {
      const { stdout } = await this.execAsync('OperaTabManager.applescript', 'focusTab', [_urlPattern]);
      return stdout.trim().startsWith('found-');
    } catch {
      return false;
    }
  }
}

/**
 * macOS Chrome Adapter
 */
export class MacChromeAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Google Chrome');
  }
  
  async closeTabs(_urlPattern: string): Promise<number> {
    try {
      const { stdout } = await this.execAsync('ChromeTabManager.applescript', 'closeTabs', [_urlPattern]);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    try {
      const { stdout } = await this.execAsync('ChromeTabManager.applescript', 'countTabs', [_urlPattern]);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    try {
      const { stdout } = await this.execAsync('ChromeTabManager.applescript', 'focusTab', [_urlPattern]);
      return stdout.trim().startsWith('found');
    } catch {
      return false;
    }
  }
}

/**
 * Linux/Windows stub adapters - to be implemented
 */
export class LinuxBrowserAdapter extends BaseBrowserAdapter {
  async closeTabs(_urlPattern: string): Promise<number> {
    // Use xdotool or wmctrl for Linux
    console.log(`Linux adapter not implemented yet for ${this.browserName}`);
    return 0;
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    // Use lsof and window manager tools
    return 0;
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    return false;
  }
}

export class WindowsBrowserAdapter extends BaseBrowserAdapter {
  async closeTabs(_urlPattern: string): Promise<number> {
    // Use PowerShell or Windows API
    console.log(`Windows adapter not implemented yet for ${this.browserName}`);
    return 0;
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    return 0;
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    return false;
  }
}

/**
 * Smart Browser Manager that uses appropriate adapters
 */
export class BrowserTabManager {
  private adapters: Map<string, BaseBrowserAdapter> = new Map();
  
  constructor() {
    this.initializeAdapters();
  }
  
  private initializeAdapters() {
    const os = platform();
    
    if (os === 'darwin') {
      // macOS adapters
      this.adapters.set('opera', new MacOperaAdapter());
      this.adapters.set('chrome', new MacChromeAdapter());
    } else if (os === 'linux') {
      // Linux adapters
      this.adapters.set('chrome', new LinuxBrowserAdapter('Chrome'));
      this.adapters.set('firefox', new LinuxBrowserAdapter('Firefox'));
    } else if (os === 'win32') {
      // Windows adapters
      this.adapters.set('chrome', new WindowsBrowserAdapter('Chrome'));
      this.adapters.set('edge', new WindowsBrowserAdapter('Edge'));
    }
  }
  
  /**
   * Close all tabs except one for localhost:9000
   */
  async enforceOneTabPolicy(_urlPattern: string = 'localhost:9000'): Promise<TabCloseResult[]> {
    const results: TabCloseResult[] = [];
    
    for (const [browserName, adapter] of this.adapters) {
      try {
        const closed = await adapter.closeTabs(_urlPattern);
        if (closed > 0) {
          results.push({ browser: browserName, closed });
        }
      } catch (error) {
        results.push({ 
          browser: browserName, 
          closed: 0, 
          error: error instanceof Error ? error.message : String(error) 
        });
      }
    }
    
    return results;
  }
  
  /**
   * Count total tabs across all browsers
   */
  async countAllTabs(_urlPattern: string = 'localhost:9000'): Promise<number> {
    let total = 0;
    
    for (const [, adapter] of this.adapters.entries()) {
      try {
        total += await adapter.countTabs(_urlPattern);
      } catch {
        // Continue on error
      }
    }
    
    return total;
  }
  
  /**
   * Focus an existing tab if available
   */
  async focusExistingTab(_urlPattern: string = 'localhost:9000'): Promise<boolean> {
    for (const [, adapter] of this.adapters.entries()) {
      try {
        if (await adapter.focusTab(_urlPattern)) {
          return true;
        }
      } catch {
        // Continue trying other browsers
      }
    }
    
    return false;
  }
}