/**
 * Browser Adapter System
 * Modular, abstracted browser tab management across different browsers
 */

class BrowserAdapter {
  constructor() {
    this.adapters = new Map();
    this.initializeAdapters();
  }

  initializeAdapters() {
    // Register platform-specific adapters
    if (process.platform === 'darwin') {
      this.adapters.set('opera', new OperaAdapter());
      this.adapters.set('chrome', new ChromeAdapter());
      this.adapters.set('safari', new SafariAdapter());
      this.adapters.set('firefox', new FirefoxAdapter());
    } else if (process.platform === 'win32') {
      this.adapters.set('chrome', new WindowsChromeAdapter());
      this.adapters.set('firefox', new WindowsFirefoxAdapter());
    } else {
      this.adapters.set('chrome', new LinuxChromeAdapter());
      this.adapters.set('firefox', new LinuxFirefoxAdapter());
    }
  }

  async closeAllTabs(url) {
    console.log('🔍 Scanning browsers for existing tabs...');
    const results = [];
    
    // First check which browsers are actually running
    const runningBrowsers = await this.getRunningBrowsers();
    
    for (const [browserName, adapter] of this.adapters) {
      // Only try browsers that are running to avoid permission requests
      if (!runningBrowsers.includes(browserName)) {
        continue;
      }
      
      try {
        const closed = await adapter.closeTabs(url);
        if (closed > 0) {
          console.log(`🗑️ Closed ${closed} tabs in ${browserName}`);
          results.push({ browser: browserName, closed });
        }
      } catch (error) {
        // Browser not available or error, continue
      }
    }
    
    return results;
  }

  async focusExistingTab(url) {
    console.log('🎯 Attempting to focus existing tab...');
    
    // First check which browsers are actually running
    const runningBrowsers = await this.getRunningBrowsers();
    
    for (const [browserName, adapter] of this.adapters) {
      // Only try browsers that are running to avoid permission requests
      if (!runningBrowsers.includes(browserName)) {
        continue;
      }
      
      try {
        const focused = await adapter.focusTab(url);
        if (focused) {
          console.log(`✅ Focused existing tab in ${browserName}`);
          return true;
        }
      } catch (error) {
        // Browser not available or tab not found, try next
      }
    }
    
    return false;
  }

  async openNewTab(url) {
    console.log('📱 Opening new browser tab...');
    
    // First check which browsers are actually running
    const runningBrowsers = await this.getRunningBrowsers();
    
    // Try adapters in preference order, but only if they're running
    const preferredOrder = ['opera', 'chrome', 'safari', 'firefox'];
    
    // First try running browsers
    for (const browserName of preferredOrder) {
      if (runningBrowsers.includes(browserName)) {
        const adapter = this.adapters.get(browserName);
        if (adapter) {
          try {
            await adapter.openTab(url);
            console.log(`🌐 Opened new tab in ${browserName}`);
            return true;
          } catch (error) {
            // Try next browser
          }
        }
      }
    }
    
    // If no running browsers worked, try to launch a preferred browser
    for (const browserName of preferredOrder) {
      const adapter = this.adapters.get(browserName);
      if (adapter) {
        try {
          await adapter.openTab(url);
          console.log(`🌐 Opened new tab in ${browserName}`);
          return true;
        } catch (error) {
          // Try next browser
        }
      }
    }
    
    // Fallback to system default
    return this.openWithSystemDefault(url);
  }

  async getRunningBrowsers() {
    if (process.platform !== 'darwin') {
      return []; // Only implemented for macOS for now
    }
    
    const { spawn } = require('child_process');
    const runningBrowsers = [];
    
    try {
      const result = await new Promise((resolve, reject) => {
        const process = spawn('osascript', ['-e', 'tell application "System Events" to get name of every application process']);
        let output = '';
        
        process.stdout.on('data', (data) => output += data.toString());
        process.on('close', (code) => {
          if (code === 0) {
            resolve(output.trim());
          } else {
            reject(new Error('Failed to get running processes'));
          }
        });
        process.on('error', reject);
      });
      
      const processes = result.split(', ').map(name => name.trim());
      
      // Map process names to our browser identifiers
      const browserMap = {
        'Opera': 'opera',
        'Google Chrome': 'chrome', 
        'Safari': 'safari',
        'Firefox': 'firefox'
      };
      
      for (const [processName, browserId] of Object.entries(browserMap)) {
        if (processes.includes(processName)) {
          runningBrowsers.push(browserId);
        }
      }
      
      if (runningBrowsers.length > 0) {
        console.log(`🔍 Found running browsers: ${runningBrowsers.join(', ')}`);
      }
      
    } catch (error) {
      // If we can't detect, assume no browsers are running to avoid permission prompts
      console.log('⚠️ Could not detect running browsers, will use system default');
    }
    
    return runningBrowsers;
  }
  
  async openWithSystemDefault(url) {
    const { spawn } = require('child_process');
    
    try {
      let command, args;
      
      switch (process.platform) {
        case 'darwin':
          command = 'open';
          args = [url];
          break;
        case 'win32':
          command = 'start';
          args = ['', url];
          break;
        default:
          command = 'xdg-open';
          args = [url];
      }
      
      spawn(command, args, { detached: true, stdio: 'ignore' });
      console.log(`🌐 Opened with system default browser`);
      return true;
      
    } catch (error) {
      console.log(`⚠️ Could not open browser. Please visit: ${url}`);
      return false;
    }
  }
}

// Base Browser Adapter Class
class BaseBrowserAdapter {
  constructor(browserName, scriptEngine = 'applescript') {
    this.browserName = browserName;
    this.scriptEngine = scriptEngine;
  }

  async executeScript(script) {
    const { spawn } = require('child_process');
    
    return new Promise((resolve, reject) => {
      let command, args;
      
      switch (this.scriptEngine) {
        case 'applescript':
          command = 'osascript';
          args = ['-e', script];
          break;
        case 'powershell':
          command = 'powershell';
          args = ['-Command', script];
          break;
        default:
          reject(new Error(`Unsupported script engine: ${this.scriptEngine}`));
          return;
      }
      
      const process = spawn(command, args);
      let output = '';
      let error = '';
      
      process.stdout.on('data', (data) => output += data.toString());
      process.stderr.on('data', (data) => error += data.toString());
      
      process.on('close', (code) => {
        if (code === 0) {
          resolve(output.trim());
        } else {
          reject(new Error(`Script failed: ${error || 'Unknown error'}`));
        }
      });
      
      process.on('error', reject);
    });
  }
}

// macOS Browser Adapters
class OperaAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Opera', 'applescript');
  }

  async closeTabs(url) {
    const script = `
      tell application "Opera"
        set closedCount to 0
        repeat with w in windows
          set tabsToClose to {}
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set end of tabsToClose to t
            end if
          end repeat
          repeat with t in reverse of tabsToClose
            close t
            set closedCount to closedCount + 1
          end repeat
        end repeat
        return closedCount
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return parseInt(result) || 0;
    } catch (error) {
      return 0;
    }
  }

  async focusTab(url) {
    const script = `
      tell application "Opera"
        repeat with w in windows
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set active tab index of w to index of t
              set index of w to 1
              reload t
              activate
              return "found"
            end if
          end repeat
        end repeat
        return "not found"
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return result === 'found';
    } catch (error) {
      return false;
    }
  }

  async openTab(url) {
    const script = `
      tell application "Opera"
        activate
        tell front window to make new tab with properties {URL:"${url}"}
      end tell
    `;
    
    await this.executeScript(script);
  }

  extractDomain(url) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
}

class ChromeAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Google Chrome', 'applescript');
  }

  async closeTabs(url) {
    const script = `
      tell application "Google Chrome"
        set closedCount to 0
        repeat with w in windows
          set tabsToClose to {}
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set end of tabsToClose to t
            end if
          end repeat
          repeat with t in reverse of tabsToClose
            close t
            set closedCount to closedCount + 1
          end repeat
        end repeat
        return closedCount
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return parseInt(result) || 0;
    } catch (error) {
      return 0;
    }
  }

  async focusTab(url) {
    const script = `
      tell application "Google Chrome"
        repeat with w in windows
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set active tab index of w to index of t
              set index of w to 1
              reload t
              activate
              return "found"
            end if
          end repeat
        end repeat
        return "not found"
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return result === 'found';
    } catch (error) {
      return false;
    }
  }

  async openTab(url) {
    const script = `
      tell application "Google Chrome"
        activate
        tell front window to make new tab with properties {URL:"${url}"}
      end tell
    `;
    
    await this.executeScript(script);
  }

  extractDomain(url) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
}

class SafariAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Safari', 'applescript');
  }

  async closeTabs(url) {
    // Safari has different AppleScript syntax
    const script = `
      tell application "Safari"
        set closedCount to 0
        repeat with w in windows
          set tabsToClose to {}
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set end of tabsToClose to t
            end if
          end repeat
          repeat with t in reverse of tabsToClose
            close t
            set closedCount to closedCount + 1
          end repeat
        end repeat
        return closedCount
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return parseInt(result) || 0;
    } catch (error) {
      return 0;
    }
  }

  async focusTab(url) {
    const script = `
      tell application "Safari"
        repeat with w in windows
          repeat with t in tabs of w
            if URL of t contains "${this.extractDomain(url)}" then
              set current tab of w to t
              set index of w to 1
              tell t to do JavaScript "location.reload()"
              activate
              return "found"
            end if
          end repeat
        end repeat
        return "not found"
      end tell
    `;
    
    try {
      const result = await this.executeScript(script);
      return result === 'found';
    } catch (error) {
      return false;
    }
  }

  async openTab(url) {
    const script = `
      tell application "Safari"
        activate
        tell front window to make new tab with properties {URL:"${url}"}
      end tell
    `;
    
    await this.executeScript(script);
  }

  extractDomain(url) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
}

// Placeholder adapters for other browsers and platforms
class FirefoxAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Firefox', 'applescript');
  }
  
  async closeTabs(url) { return 0; }
  async focusTab(url) { return false; }
  async openTab(url) { throw new Error('Firefox adapter not implemented'); }
}

class WindowsChromeAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Chrome', 'powershell');
  }
  
  async closeTabs(url) { return 0; }
  async focusTab(url) { return false; }
  async openTab(url) { throw new Error('Windows Chrome adapter not implemented'); }
}

class WindowsFirefoxAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Firefox', 'powershell');
  }
  
  async closeTabs(url) { return 0; }
  async focusTab(url) { return false; }
  async openTab(url) { throw new Error('Windows Firefox adapter not implemented'); }
}

class LinuxChromeAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Chrome', 'shell');
  }
  
  async closeTabs(url) { return 0; }
  async focusTab(url) { return false; }
  async openTab(url) { throw new Error('Linux Chrome adapter not implemented'); }
}

class LinuxFirefoxAdapter extends BaseBrowserAdapter {
  constructor() {
    super('Firefox', 'shell');
  }
  
  async closeTabs(url) { return 0; }
  async focusTab(url) { return false; }
  async openTab(url) { throw new Error('Linux Firefox adapter not implemented'); }
}

module.exports = { BrowserAdapter };