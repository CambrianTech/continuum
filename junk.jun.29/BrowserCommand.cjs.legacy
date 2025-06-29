/**
 * Browser Command - WebSocket coordination and browser communication
 * Orchestrates browser-side operations through WebSocket connections
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class BrowserCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'browser',
      description: 'Coordinate browser operations through WebSocket connections',
      icon: '🌐',
      category: 'core',
      parameters: {
        action: {
          type: 'string',
          required: true,
          description: 'Browser action: screenshot, click, scroll, navigate, evaluate'
        },
        params: {
          type: 'object',
          required: false,
          description: 'Parameters for the browser action'
        },
        timeout: {
          type: 'number',
          required: false,
          default: 10000,
          description: 'Timeout in milliseconds for browser operation'
        },
        waitForReady: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Wait for browser to be ready before executing'
        }
      },
      examples: [
        {
          description: 'Take a screenshot via WebSocket',
          usage: 'browser screenshot --params {"selector": "body"}'
        },
        {
          description: 'Click an element through browser',
          usage: 'browser click --params {"selector": ".button", "x": 100, "y": 200}'
        },
        {
          description: 'Evaluate JavaScript in browser',
          usage: 'browser evaluate --params {"script": "document.title"}'
        },
        {
          description: 'Navigate to URL',
          usage: 'browser navigate --params {"url": "https://example.com"}'
        }
      ]
    };
  }

  static async execute(paramsString, context) {
    try {
      const { action, params = {}, timeout = 10000, waitForReady = true } = this.parseParams(paramsString);

      console.log(`🌐 Browser: Executing ${action} with timeout ${timeout}ms`);

      if (!context || !context.webSocketServer) {
        return this.createErrorResult('No WebSocket server available for browser communication');
      }

      // Check if browser is connected
      const connectedClients = context.webSocketServer.getConnectedClients();
      if (connectedClients.length === 0) {
        return this.createErrorResult('No browser clients connected to WebSocket server');
      }

      console.log(`🌐 Browser: Found ${connectedClients.length} connected client(s)`);

      // Wait for browser ready state if requested
      if (waitForReady) {
        console.log('🌐 Browser: Waiting for ready state...');
        // In a real implementation, this would check browser readiness
      }

      // Create browser command message
      const browserMessage = {
        type: 'browser_command',
        action,
        params,
        requestId: this.generateRequestId(),
        timestamp: new Date().toISOString(),
        timeout
      };

      // Send command to browser(s)
      const responses = await this.sendToBrowsers(context.webSocketServer, browserMessage, timeout);

      if (responses.length === 0) {
        return this.createErrorResult('No responses received from browser clients');
      }

      // Process responses (for now, return the first successful response)
      const successfulResponse = responses.find(r => r.success);
      if (successfulResponse) {
        console.log(`✅ Browser: ${action} completed successfully`);
        return this.createSuccessResult(successfulResponse.data, `Browser ${action} completed`);
      } else {
        const errors = responses.map(r => r.error || 'Unknown error').join(', ');
        return this.createErrorResult(`Browser ${action} failed: ${errors}`);
      }

    } catch (error) {
      console.error(`❌ Browser command failed: ${error.message}`);
      return this.createErrorResult(`Browser operation failed: ${error.message}`);
    }
  }

  static async sendToBrowsers(webSocketServer, message, timeout) {
    return new Promise((resolve) => {
      const responses = [];
      const requestId = message.requestId;
      
      // Set up response handler
      const responseHandler = (responseMessage) => {
        if (responseMessage.requestId === requestId) {
          responses.push(responseMessage);
          webSocketServer.off('browser_response', responseHandler);
          resolve(responses);
        }
      };

      // Listen for responses
      webSocketServer.on('browser_response', responseHandler);

      // Send message to all connected browsers
      webSocketServer.broadcast(message);

      // Set timeout
      setTimeout(() => {
        webSocketServer.off('browser_response', responseHandler);
        resolve(responses);
      }, timeout);
    });
  }

  static generateRequestId() {
    return `browser_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  // Browser action helpers
  static async screenshot(selector = 'body', context) {
    return this.execute(JSON.stringify({
      action: 'screenshot',
      params: { selector },
      timeout: 15000
    }), context);
  }

  static async click(selector, context) {
    return this.execute(JSON.stringify({
      action: 'click',
      params: { selector },
      timeout: 5000
    }), context);
  }

  static async evaluate(script, context) {
    return this.execute(JSON.stringify({
      action: 'evaluate',
      params: { script },
      timeout: 10000
    }), context);
  }

  static async navigate(url, context) {
    return this.execute(JSON.stringify({
      action: 'navigate',
      params: { url },
      timeout: 30000
    }), context);
  }

  static async waitForElement(selector, timeout = 10000, context) {
    return this.execute(JSON.stringify({
      action: 'waitForElement',
      params: { selector, timeout },
      timeout: timeout + 1000
    }), context);
  }

  static async scrollTo(x, y, context) {
    return this.execute(JSON.stringify({
      action: 'scroll',
      params: { x, y },
      timeout: 5000
    }), context);
  }
}

module.exports = BrowserCommand;