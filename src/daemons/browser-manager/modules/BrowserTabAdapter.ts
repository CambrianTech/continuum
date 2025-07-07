/**
 * Browser Tab Adapter - Platform-specific tab management
 * Inspired by claude/testing-autonomy but adapted for our TypeScript daemon architecture
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { platform } from 'os';

const execAsync = promisify(exec);

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
}

/**
 * macOS Opera Adapter using AppleScript
 */
export class MacOperaAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Opera GX');
  }
  
  async closeTabs(_urlPattern: string): Promise<number> {
    const script = `
      tell application "Opera GX"
        set closedCount to 0
        set foundFirst to false
        repeat with w in (get windows)
          set tabList to tabs of w
          repeat with i from (count of tabList) to 1 by -1
            set t to item i of tabList
            set currentURL to URL of t
            -- Match ONLY the exact URL (no paths, only query params/fragments allowed)
            if (currentURL is equal to "${_urlPattern}") or ¬
               (currentURL is equal to "${_urlPattern}/") or ¬
               (currentURL starts with "${_urlPattern}?") or ¬
               (currentURL starts with "${_urlPattern}#") then
              if foundFirst then
                close t
                set closedCount to closedCount + 1
              else
                set foundFirst to true
              end if
            end if
          end repeat
        end repeat
        return closedCount
      end tell
    `;
    
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    const script = `
      tell application "Opera GX"
        set tabCount to 0
        repeat with w in windows
          repeat with t in tabs of w
            set currentURL to URL of t
            -- Match ONLY the exact URL (no paths, only query params/fragments allowed)
            if (currentURL is equal to "${_urlPattern}") or ¬
               (currentURL is equal to "${_urlPattern}/") or ¬
               (currentURL starts with "${_urlPattern}?") or ¬
               (currentURL starts with "${_urlPattern}#") then
              set tabCount to tabCount + 1
            end if
          end repeat
        end repeat
        return tabCount
      end tell
    `;
    
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    const script = `
      tell application "Opera GX"
        -- First pass: Look for exact match only (highest priority)
        repeat with w in windows
          repeat with t in tabs of w
            set currentURL to URL of t
            if (currentURL is equal to "${_urlPattern}") then
              set active tab index of w to index of t
              set index of w to 1
              activate
              return "found-exact"
            end if
          end repeat
        end repeat
        
        -- Second pass: Look for exact match with trailing slash
        repeat with w in windows
          repeat with t in tabs of w
            set currentURL to URL of t
            if (currentURL is equal to "${_urlPattern}/") then
              set active tab index of w to index of t
              set index of w to 1
              activate
              return "found-slash"
            end if
          end repeat
        end repeat
        
        -- Third pass: Look for query params or fragments
        repeat with w in windows
          repeat with t in tabs of w
            set currentURL to URL of t
            if (currentURL starts with "${_urlPattern}?") or ¬
               (currentURL starts with "${_urlPattern}#") then
              set active tab index of w to index of t
              set index of w to 1
              activate
              return "found-params"
            end if
          end repeat
        end repeat
        
        return "not found"
      end tell
    `;
    
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
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
    const script = `
      tell application "Google Chrome"
        set closedCount to 0
        set foundFirst to false
        repeat with w in (get windows)
          set tabList to tabs of w
          repeat with i from (count of tabList) to 1 by -1
            set t to item i of tabList
            if (URL of t contains "${_urlPattern}") then
              if foundFirst then
                close t
                set closedCount to closedCount + 1
              else
                set foundFirst to true
              end if
            end if
          end repeat
        end repeat
        return closedCount
      end tell
    `;
    
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    const script = `
      tell application "Google Chrome"
        set tabCount to 0
        repeat with w in windows
          repeat with t in tabs of w
            if (URL of t contains "${_urlPattern}") then
              set tabCount to tabCount + 1
            end if
          end repeat
        end repeat
        return tabCount
      end tell
    `;
    
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return parseInt(stdout.trim()) || 0;
    } catch {
      return 0;
    }
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    // Similar to Opera implementation
    return false;
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