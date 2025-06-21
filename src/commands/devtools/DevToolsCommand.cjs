/**
 * DevTools Command
 * Expose browser DevTools capabilities through Continuum command system
 */

const BaseCommand = require('../core/BaseCommand.cjs');

class DevToolsCommand extends BaseCommand {
  constructor() {
    super();
    this.name = 'DEVTOOLS';
    this.description = 'Access browser development tools capabilities';
    this.devtools = null; // Will be injected by continuum
    
    this.subcommands = {
      'connect': this.connect.bind(this),
      'disconnect': this.disconnect.bind(this),
      'status': this.getStatus.bind(this),
      'screenshot': this.takeScreenshot.bind(this),
      'console': this.getConsoleHistory.bind(this),
      'websocket': this.getWebSocketFrames.bind(this),
      'execute': this.executeScript.bind(this),
      'inspect': this.inspectElement.bind(this),
      'performance': this.getPerformanceMetrics.bind(this),
      'network': this.getNetworkActivity.bind(this)
    };
  }

  async execute(params) {
    const { action = 'status', ...actionParams } = this.parseParams(params);
    
    if (!this.devtools) {
      return {
        success: false,
        error: 'DevTools integration not available',
        available_actions: Object.keys(this.subcommands)
      };
    }

    const handler = this.subcommands[action];
    if (!handler) {
      return {
        success: false,
        error: `Unknown action: ${action}`,
        available_actions: Object.keys(this.subcommands)
      };
    }

    try {
      return await handler(actionParams);
    } catch (error) {
      return {
        success: false,
        error: error.message,
        action: action
      };
    }
  }

  async connect(params) {
    const { adapter = 'auto', ...options } = params;
    
    const success = await this.devtools.connect(adapter);
    
    return {
      success,
      message: success ? `Connected via ${adapter}` : 'Connection failed',
      status: this.devtools.getStatus()
    };
  }

  async disconnect() {
    await this.devtools.disconnect();
    
    return {
      success: true,
      message: 'Disconnected from DevTools',
      status: this.devtools.getStatus()
    };
  }

  async getStatus() {
    return {
      success: true,
      status: this.devtools.getStatus(),
      capabilities: this.getCapabilities()
    };
  }

  async takeScreenshot(params = {}) {
    const { 
      format = 'png', 
      quality = 90, 
      fullPage = false,
      element = null,
      filename = null 
    } = params;

    if (!this.devtools.activeAdapter) {
      return {
        success: false,
        error: 'No active DevTools connection'
      };
    }

    try {
      const adapter = this.devtools.activeAdapter.instance;
      
      if (!adapter.takeScreenshot) {
        return {
          success: false,
          error: 'Screenshot not supported by current adapter'
        };
      }

      const screenshot = await adapter.takeScreenshot({
        format,
        quality,
        fullPage,
        element
      });

      if (filename && screenshot.success) {
        // Save to file if filename provided
        const fs = require('fs');
        const path = require('path');
        
        const filepath = path.resolve(filename);
        const buffer = Buffer.from(screenshot.data, 'base64');
        fs.writeFileSync(filepath, buffer);
        
        return {
          success: true,
          message: `Screenshot saved to ${filepath}`,
          filepath,
          size: buffer.length
        };
      }

      return {
        success: screenshot.success,
        data: screenshot.data,
        format,
        error: screenshot.error
      };

    } catch (error) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  async getConsoleHistory(params = {}) {
    const { limit = 50, level = 'all' } = params;
    
    // This would collect from the event history
    const history = this.devtools.getConsoleHistory?.(limit, level) || [];
    
    return {
      success: true,
      history,
      count: history.length,
      filters: { limit, level }
    };
  }

  async getWebSocketFrames(params = {}) {
    const { limit = 100, direction = 'all' } = params;
    
    const frames = this.devtools.getWebSocketFrames?.(limit, direction) || [];
    
    return {
      success: true,
      frames,
      count: frames.length,
      filters: { limit, direction }
    };
  }

  async executeScript(params) {
    const { script, context = 'page' } = params;
    
    if (!script) {
      return {
        success: false,
        error: 'Script parameter required'
      };
    }

    try {
      const result = await this.devtools.send('Runtime.evaluate', {
        expression: script,
        returnByValue: true,
        includeCommandLineAPI: true
      });

      return {
        success: true,
        result: result.result?.value,
        type: result.result?.type,
        error: result.exceptionDetails?.exception?.description
      };

    } catch (error) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  async inspectElement(params) {
    const { selector, action = 'highlight' } = params;
    
    if (!selector) {
      return {
        success: false,
        error: 'Selector parameter required'
      };
    }

    try {
      // Use DevTools to find and inspect element
      const script = `
        const element = document.querySelector('${selector.replace(/'/g, "\\'")}');
        if (!element) {
          throw new Error('Element not found');
        }
        
        const rect = element.getBoundingClientRect();
        const styles = window.getComputedStyle(element);
        
        ({
          tagName: element.tagName,
          id: element.id,
          className: element.className,
          textContent: element.textContent?.substring(0, 100),
          rect: {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height
          },
          styles: {
            display: styles.display,
            position: styles.position,
            zIndex: styles.zIndex,
            backgroundColor: styles.backgroundColor,
            color: styles.color
          }
        })
      `;

      const result = await this.executeScript({ script });
      
      if (action === 'highlight' && result.success) {
        // Add highlight script
        await this.executeScript({
          script: `
            const element = document.querySelector('${selector.replace(/'/g, "\\'")}');
            if (element) {
              element.style.outline = '3px solid #ff0000';
              element.style.backgroundColor = 'rgba(255, 0, 0, 0.1)';
              setTimeout(() => {
                element.style.outline = '';
                element.style.backgroundColor = '';
              }, 3000);
            }
          `
        });
      }

      return {
        success: result.success,
        element: result.result,
        selector,
        action,
        error: result.error
      };

    } catch (error) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  async getPerformanceMetrics() {
    try {
      await this.devtools.send('Performance.enable');
      const metrics = await this.devtools.send('Performance.getMetrics');
      
      return {
        success: true,
        metrics: metrics.metrics,
        timestamp: new Date().toISOString()
      };

    } catch (error) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  async getNetworkActivity(params = {}) {
    const { limit = 50, type = 'all' } = params;
    
    // This would require maintaining network request history
    const activity = this.devtools.getNetworkActivity?.(limit, type) || [];
    
    return {
      success: true,
      activity,
      count: activity.length,
      filters: { limit, type }
    };
  }

  getCapabilities() {
    const status = this.devtools.getStatus();
    const adapter = this.devtools.activeAdapter?.instance;
    
    return {
      connected: status.connected,
      adapter: status.activeAdapter,
      features: {
        screenshot: !!(adapter?.takeScreenshot),
        console: !!(adapter?.handleEvent),
        websocket: !!(adapter?.handleEvent),
        script_execution: !!(adapter?.send),
        element_inspection: !!(adapter?.send),
        performance: !!(adapter?.send),
        network: !!(adapter?.handleEvent)
      }
    };
  }

  getHelp() {
    return {
      name: this.name,
      description: this.description,
      actions: {
        connect: 'Connect to browser DevTools (adapter: auto|chrome|webkit|firefox)',
        disconnect: 'Disconnect from DevTools',
        status: 'Get connection status and capabilities',
        screenshot: 'Take screenshot (format: png|jpeg, quality: 1-100, fullPage: true|false)',
        console: 'Get console log history (limit: number, level: log|error|warn|all)',
        websocket: 'Get WebSocket frame history (limit: number, direction: sent|received|all)',
        execute: 'Execute JavaScript in browser (script: string)',
        inspect: 'Inspect DOM element (selector: string, action: highlight|info)',
        performance: 'Get performance metrics',
        network: 'Get network activity (limit: number, type: xhr|fetch|all)'
      },
      examples: [
        'devtools connect',
        'devtools screenshot {"filename": "test.png", "fullPage": true}',
        'devtools execute {"script": "console.log(\\'Hello from DevTools\\')"}',
        'devtools inspect {"selector": "#my-element", "action": "highlight"}',
        'devtools console {"limit": 20, "level": "error"}'
      ]
    };
  }
}

module.exports = DevToolsCommand;