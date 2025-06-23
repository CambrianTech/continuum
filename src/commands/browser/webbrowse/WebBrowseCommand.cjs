/**
 * WebBrowse Command - DevTools-powered web browsing and automation
 * Provides comprehensive website browsing, screenshots, and content interaction
 */

const BaseCommand = require('../../core/BaseCommand.cjs');
const { spawn } = require('child_process');
const fetch = require('node-fetch');
const WebSocket = require('ws');
const path = require('path');
const fs = require('fs').promises;

class WebBrowseCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'webbrowse',
      description: 'Browse websites, take screenshots, and interact with web content using DevTools Protocol',
      icon: '🌐',
      category: 'browser',
      parameters: {
        action: {
          type: 'string',
          required: true,
          description: 'Action: navigate, screenshot, click, extract, evaluate, wait, status'
        },
        url: {
          type: 'string',
          required: false,
          description: 'Target URL (required for navigate action)'
        },
        options: {
          type: 'object',
          required: false,
          description: 'Action-specific options (selector, filename, script, etc.)'
        }
      },
      examples: [
        {
          description: 'Navigate to website and take screenshot',
          usage: 'webbrowse navigate --url "https://example.com" --options {"screenshot": true}'
        },
        {
          description: 'Take screenshot of current page',
          usage: 'webbrowse screenshot --options {"filename": "page.png"}'
        },
        {
          description: 'Extract page title',
          usage: 'webbrowse extract --options {"selector": "title", "property": "textContent"}'
        }
      ]
    };
  }

  constructor() {
    super();
    this.browserProcess = null;
    this.devtoolsPort = 9224; // Use dedicated port for web browsing
    this.wsConnection = null;
    this.currentTabId = null;
  }

  async execute(params, context) {
    try {
      const { action, url, options = {} } = params;
      
      console.log(`🌐 WebBrowse: Starting ${action} action`);
      
      // Ensure browser is running for all actions
      await this.ensureBrowserRunning();
      
      switch (action) {
        case 'navigate':
          if (!url) {
            return this.createErrorResult('URL is required for navigate action');
          }
          return await this.navigate(url, options);
          
        case 'screenshot':
          return await this.takeScreenshot(options);
          
        case 'extract':
          return await this.extractContent(options);
          
        case 'click':
          return await this.clickElement(options);
          
        case 'type':
          return await this.typeText(options);
          
        case 'evaluate':
          return await this.evaluateScript(options);
          
        case 'wait':
          return await this.waitForElement(options);
          
        case 'status':
          return await this.getBrowserStatus();
          
        default:
          return this.createErrorResult(`Unknown action: ${action}`);
      }
    } catch (error) {
      console.error(`❌ WebBrowse command failed: ${error.message}`);
      return this.createErrorResult(`WebBrowse operation failed: ${error.message}`);
    }
  }

  async ensureBrowserRunning() {
    try {
      // Check if DevTools port is responding
      const response = await fetch(`http://localhost:${this.devtoolsPort}/json/version`, {
        timeout: 3000
      });
      
      if (response.ok) {
        console.log('🔌 WebBrowse: Browser already running');
        return;
      }
    } catch (error) {
      // Browser not running, need to start it
    }

    console.log('🚀 WebBrowse: Launching isolated browser for web browsing...');
    await this.launchBrowser();
    
    // Wait for browser to be ready
    await this.waitForBrowserReady();
  }

  async launchBrowser() {
    // Kill any existing browsers on our port
    try {
      await this.killExistingBrowsers();
    } catch (error) {
      console.log('ℹ️ No existing browsers to clean up');
    }

    const browserCmd = [
      '/Applications/Opera GX.app/Contents/MacOS/Opera',
      `--remote-debugging-port=${this.devtoolsPort}`,
      '--headless', // Use headless for web browsing
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
      `--user-data-dir=/tmp/opera-webbrowse-${this.devtoolsPort}`,
      'about:blank'
    ];

    console.log(`🚀 WebBrowse: Starting browser on port ${this.devtoolsPort}`);
    this.browserProcess = spawn(browserCmd[0], browserCmd.slice(1), {
      stdio: 'pipe',
      detached: false
    });

    this.browserProcess.on('error', (error) => {
      console.error('❌ WebBrowse: Browser process error:', error);
    });

    this.browserProcess.on('exit', (code) => {
      console.log(`🛑 WebBrowse: Browser process exited with code ${code}`);
      this.browserProcess = null;
    });
  }

  async killExistingBrowsers() {
    const { spawn } = require('child_process');
    return new Promise((resolve) => {
      const killProcess = spawn('pkill', ['-f', `remote-debugging-port=${this.devtoolsPort}`]);
      killProcess.on('close', () => {
        setTimeout(resolve, 1000); // Wait a second for cleanup
      });
    });
  }

  async waitForBrowserReady() {
    const maxAttempts = 20;
    const delayMs = 500;
    
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        const response = await fetch(`http://localhost:${this.devtoolsPort}/json/version`, {
          timeout: 3000
        });
        
        if (response.ok) {
          console.log(`✅ WebBrowse: Browser ready on port ${this.devtoolsPort} (attempt ${attempt})`);
          await this.connectToFirstTab();
          return;
        }
      } catch (error) {
        // Continue trying
      }
      
      if (attempt < maxAttempts) {
        await new Promise(resolve => setTimeout(resolve, delayMs));
      }
    }
    
    throw new Error(`Browser failed to start after ${maxAttempts} attempts`);
  }

  async connectToFirstTab() {
    try {
      const tabsResponse = await fetch(`http://localhost:${this.devtoolsPort}/json`);
      const tabs = await tabsResponse.json();
      
      if (tabs.length === 0) {
        throw new Error('No browser tabs available');
      }
      
      const tab = tabs[0];
      this.currentTabId = tab.id;
      
      console.log(`🔌 WebBrowse: Connected to tab: ${tab.title || 'New Tab'}`);
      
      // Connect WebSocket to tab
      if (tab.webSocketDebuggerUrl) {
        await this.connectWebSocket(tab.webSocketDebuggerUrl);
      }
    } catch (error) {
      console.error('❌ WebBrowse: Failed to connect to browser tab:', error);
      throw error;
    }
  }

  async connectWebSocket(wsUrl) {
    return new Promise((resolve, reject) => {
      this.wsConnection = new WebSocket(wsUrl);
      
      this.wsConnection.on('open', () => {
        console.log('🔌 WebBrowse: WebSocket connected');
        
        // Enable necessary domains
        this.enableDevToolsDomains().then(resolve).catch(reject);
      });
      
      this.wsConnection.on('error', (error) => {
        console.error('❌ WebBrowse: WebSocket error:', error);
        reject(error);
      });
      
      this.wsConnection.on('close', () => {
        console.log('🔌 WebBrowse: WebSocket disconnected');
        this.wsConnection = null;
      });
    });
  }

  async enableDevToolsDomains() {
    const domains = ['Page', 'Runtime', 'DOM'];
    
    for (const domain of domains) {
      await this.sendDevToolsCommand(`${domain}.enable`);
    }
    
    console.log('✅ WebBrowse: DevTools domains enabled');
  }

  async sendDevToolsCommand(method, params = {}) {
    return new Promise((resolve, reject) => {
      if (!this.wsConnection) {
        reject(new Error('WebSocket not connected'));
        return;
      }
      
      const id = Math.floor(Math.random() * 1000000);
      const message = {
        id,
        method,
        params
      };
      
      const timeout = setTimeout(() => {
        reject(new Error(`DevTools command ${method} timed out`));
      }, 10000);
      
      const messageHandler = (data) => {
        try {
          const response = JSON.parse(data);
          if (response.id === id) {
            clearTimeout(timeout);
            this.wsConnection.off('message', messageHandler);
            
            if (response.error) {
              reject(new Error(`DevTools error: ${response.error.message}`));
            } else {
              resolve(response.result);
            }
          }
        } catch (error) {
          // Ignore parsing errors for other messages
        }
      };
      
      this.wsConnection.on('message', messageHandler);
      this.wsConnection.send(JSON.stringify(message));
    });
  }

  async navigate(url, options = {}) {
    try {
      console.log(`🌐 WebBrowse: Navigating to ${url}`);
      
      await this.sendDevToolsCommand('Page.navigate', { url });
      
      // Wait for navigation to complete
      await this.waitForNavigation();
      
      console.log(`✅ WebBrowse: Navigation to ${url} completed`);
      
      // Take screenshot if requested
      if (options.screenshot) {
        const screenshotResult = await this.takeScreenshot({
          filename: options.filename || `webbrowse-${Date.now()}.png`
        });
        
        return this.createSuccessResult({
          url,
          screenshot: screenshotResult.data
        }, `Navigated to ${url} and captured screenshot`);
      }
      
      return this.createSuccessResult({ url }, `Successfully navigated to ${url}`);
    } catch (error) {
      return this.createErrorResult(`Navigation failed: ${error.message}`);
    }
  }

  async waitForNavigation(timeout = 10000) {
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        reject(new Error('Navigation timeout'));
      }, timeout);
      
      const messageHandler = (data) => {
        try {
          const message = JSON.parse(data);
          if (message.method === 'Page.loadEventFired') {
            clearTimeout(timeoutId);
            this.wsConnection.off('message', messageHandler);
            resolve();
          }
        } catch (error) {
          // Ignore parsing errors
        }
      };
      
      this.wsConnection.on('message', messageHandler);
    });
  }

  async takeScreenshot(options = {}) {
    try {
      console.log('📸 WebBrowse: Taking screenshot...');
      
      const screenshotParams = {
        format: 'png',
        quality: 100
      };
      
      if (options.fullPage !== false) {
        // Get page dimensions for full page screenshot
        const metrics = await this.sendDevToolsCommand('Page.getLayoutMetrics');
        screenshotParams.clip = {
          x: 0,
          y: 0,
          width: metrics.contentSize.width,
          height: metrics.contentSize.height,
          scale: 1
        };
      }
      
      const result = await this.sendDevToolsCommand('Page.captureScreenshot', screenshotParams);
      
      // Save screenshot to file
      const filename = options.filename || `webbrowse-${Date.now()}.png`;
      const screenshotDir = path.join(process.cwd(), '.continuum', 'screenshots');
      const filepath = path.join(screenshotDir, filename);
      
      // Ensure screenshots directory exists
      await fs.mkdir(screenshotDir, { recursive: true });
      
      // Decode base64 and save
      const buffer = Buffer.from(result.data, 'base64');
      await fs.writeFile(filepath, buffer);
      
      console.log(`✅ WebBrowse: Screenshot saved to ${filepath}`);
      
      return this.createSuccessResult({
        filename,
        filepath,
        size: buffer.length
      }, `Screenshot captured: ${filename}`);
    } catch (error) {
      return this.createErrorResult(`Screenshot failed: ${error.message}`);
    }
  }

  async extractContent(options = {}) {
    try {
      const { selector, property = 'textContent', all = false } = options;
      
      if (!selector) {
        return this.createErrorResult('Selector is required for extract action');
      }
      
      console.log(`📝 WebBrowse: Extracting ${property} from ${selector}`);
      
      const script = all 
        ? `Array.from(document.querySelectorAll('${selector}')).map(el => el.${property})`
        : `document.querySelector('${selector}')?.${property}`;
      
      const result = await this.sendDevToolsCommand('Runtime.evaluate', {
        expression: script,
        returnByValue: true
      });
      
      if (result.exceptionDetails) {
        return this.createErrorResult(`Script error: ${result.exceptionDetails.text}`);
      }
      
      return this.createSuccessResult({
        selector,
        property,
        value: result.result.value
      }, `Extracted ${property} from ${selector}`);
    } catch (error) {
      return this.createErrorResult(`Content extraction failed: ${error.message}`);
    }
  }

  async clickElement(options = {}) {
    try {
      const { selector } = options;
      
      if (!selector) {
        return this.createErrorResult('Selector is required for click action');
      }
      
      console.log(`👆 WebBrowse: Clicking element ${selector}`);
      
      const script = `
        const element = document.querySelector('${selector}');
        if (element) {
          element.click();
          true;
        } else {
          false;
        }
      `;
      
      const result = await this.sendDevToolsCommand('Runtime.evaluate', {
        expression: script,
        returnByValue: true
      });
      
      if (result.result.value) {
        return this.createSuccessResult({ selector }, `Clicked element: ${selector}`);
      } else {
        return this.createErrorResult(`Element not found: ${selector}`);
      }
    } catch (error) {
      return this.createErrorResult(`Click failed: ${error.message}`);
    }
  }

  async typeText(options = {}) {
    try {
      const { selector, text } = options;
      
      if (!selector || !text) {
        return this.createErrorResult('Selector and text are required for type action');
      }
      
      console.log(`⌨️ WebBrowse: Typing into ${selector}`);
      
      const script = `
        const element = document.querySelector('${selector}');
        if (element) {
          element.focus();
          element.value = '${text.replace(/'/g, "\\'")}';
          element.dispatchEvent(new Event('input', { bubbles: true }));
          true;
        } else {
          false;
        }
      `;
      
      const result = await this.sendDevToolsCommand('Runtime.evaluate', {
        expression: script,
        returnByValue: true
      });
      
      if (result.result.value) {
        return this.createSuccessResult({ selector, text }, `Typed text into: ${selector}`);
      } else {
        return this.createErrorResult(`Element not found: ${selector}`);
      }
    } catch (error) {
      return this.createErrorResult(`Type failed: ${error.message}`);
    }
  }

  async evaluateScript(options = {}) {
    try {
      const { script } = options;
      
      if (!script) {
        return this.createErrorResult('Script is required for evaluate action');
      }
      
      console.log(`⚡ WebBrowse: Evaluating JavaScript`);
      
      const result = await this.sendDevToolsCommand('Runtime.evaluate', {
        expression: script,
        returnByValue: true
      });
      
      if (result.exceptionDetails) {
        return this.createErrorResult(`Script error: ${result.exceptionDetails.text}`);
      }
      
      return this.createSuccessResult({
        script,
        result: result.result.value
      }, `Script evaluated successfully`);
    } catch (error) {
      return this.createErrorResult(`Script evaluation failed: ${error.message}`);
    }
  }

  async waitForElement(options = {}) {
    try {
      const { selector, timeout = 5000 } = options;
      
      if (!selector) {
        return this.createErrorResult('Selector is required for wait action');
      }
      
      console.log(`⏳ WebBrowse: Waiting for element ${selector}`);
      
      const script = `
        new Promise((resolve) => {
          const checkElement = () => {
            const element = document.querySelector('${selector}');
            if (element) {
              resolve(true);
            } else {
              setTimeout(checkElement, 100);
            }
          };
          checkElement();
          setTimeout(() => resolve(false), ${timeout});
        });
      `;
      
      const result = await this.sendDevToolsCommand('Runtime.evaluate', {
        expression: script,
        awaitPromise: true,
        returnByValue: true
      });
      
      if (result.result.value) {
        return this.createSuccessResult({ selector }, `Element appeared: ${selector}`);
      } else {
        return this.createErrorResult(`Element not found within ${timeout}ms: ${selector}`);
      }
    } catch (error) {
      return this.createErrorResult(`Wait failed: ${error.message}`);
    }
  }

  async getBrowserStatus() {
    try {
      let status = {
        browserRunning: false,
        wsConnected: false,
        currentTab: null,
        port: this.devtoolsPort
      };
      
      // Check if browser is running
      try {
        const response = await fetch(`http://localhost:${this.devtoolsPort}/json/version`);
        if (response.ok) {
          status.browserRunning = true;
          const version = await response.json();
          status.browserVersion = version;
        }
      } catch (error) {
        // Browser not running
      }
      
      // Check WebSocket connection
      if (this.wsConnection && this.wsConnection.readyState === WebSocket.OPEN) {
        status.wsConnected = true;
      }
      
      // Get current tab info
      if (status.browserRunning) {
        try {
          const tabsResponse = await fetch(`http://localhost:${this.devtoolsPort}/json`);
          const tabs = await tabsResponse.json();
          if (tabs.length > 0) {
            status.currentTab = {
              id: tabs[0].id,
              title: tabs[0].title,
              url: tabs[0].url
            };
          }
        } catch (error) {
          // Could not get tab info
        }
      }
      
      return this.createSuccessResult(status, 'Browser status retrieved');
    } catch (error) {
      return this.createErrorResult(`Status check failed: ${error.message}`);
    }
  }

  async cleanup() {
    try {
      if (this.wsConnection) {
        this.wsConnection.close();
        this.wsConnection = null;
      }
      
      if (this.browserProcess) {
        this.browserProcess.kill();
        this.browserProcess = null;
      }
      
      // Kill browser by port
      await this.killExistingBrowsers();
      
      console.log('🧹 WebBrowse: Cleanup completed');
    } catch (error) {
      console.error('❌ WebBrowse: Cleanup error:', error);
    }
  }
}

module.exports = WebBrowseCommand;