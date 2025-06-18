/**
 * Promise-based Browser JavaScript Command
 * Modern replacement for BrowserJSCommand with promise support
 */

const PromiseJSExecutor = require('../../../tools/PromiseJSExecutor.cjs');

class PromiseJSCommand {
  static getDefinition() {
    return {
      name: 'promiseJs',
      category: 'Core',
      icon: '⚡',
      description: 'Execute JavaScript in browser with promise support',
      params: '<javascript_code> [options]',
      examples: [
        '{"code": "return document.title", "expectReturn": true}',
        '{"code": "console.log(\'Hello\'); document.body.style.background = \'red\'"}',
        '{"code": "document.querySelector(\'.btn\').click()"}',
        '{"selector": "agent-selector", "query": true}'
      ],
      usage: 'Execute JavaScript with modern promise support, return values, and DOM querying.'
    };
  }
  
  static async execute(params, continuum, options = {}) {
    console.log('⚡ Promise JS command executed with params:', params);
    
    try {
      if (!continuum.webSocketServer) {
        return {
          success: false,
          error: 'WebSocket server not available'
        };
      }

      // Create PromiseJSExecutor instance
      const jsExecutor = new PromiseJSExecutor(continuum.webSocketServer);
      
      // Handle different parameter formats
      let result;
      
      if (typeof params === 'string') {
        // Simple string - execute as-is
        result = await jsExecutor.execute(params, options);
      } else if (typeof params === 'object') {
        if (params.query && params.selector) {
          // DOM query mode
          result = {
            success: true,
            result: await jsExecutor.queryDOM(params.selector, params.property),
            operation: 'query'
          };
        } else if (params.waitFor) {
          // Wait for element mode
          result = {
            success: true,
            result: await jsExecutor.waitForElement(params.waitFor, params.timeout),
            operation: 'waitFor'
          };
        } else if (params.batch) {
          // Batch execution mode
          result = {
            success: true,
            result: await jsExecutor.batch(params.batch),
            operation: 'batch'
          };
        } else if (params.code) {
          // Code execution with options
          const execOptions = {
            timeout: params.timeout || 10000,
            expectReturn: params.expectReturn || false
          };
          result = await jsExecutor.execute(params.code, execOptions);
        } else {
          return {
            success: false,
            error: 'Invalid parameters. Expected {code: "...", expectReturn?: boolean} or {selector: "...", query: true}'
          };
        }
      } else {
        return {
          success: false,
          error: 'Parameters must be a string (JavaScript code) or object with execution options'
        };
      }
      
      return {
        executed: result.success,
        message: result.success ? 'JavaScript executed successfully' : 'JavaScript execution failed',
        result: result.result,
        output: result.output || [],
        error: result.error || null,
        executionTime: result.executionTime || 0,
        timestamp: result.timestamp,
        promiseMode: true
      };
      
    } catch (error) {
      console.error('⚡ Promise JS execution error:', error);
      return {
        executed: false,
        error: 'Failed to execute JavaScript: ' + error.message,
        error_stack: error.stack,
        promiseMode: true
      };
    }
  }
  
  // Utility methods for common operations
  static async queryAgents(continuum) {
    const jsExecutor = new PromiseJSExecutor(continuum.webSocketServer);
    return await jsExecutor.queryDOM('agent-selector .agent-item');
  }
  
  static async getPageTitle(continuum) {
    const jsExecutor = new PromiseJSExecutor(continuum.webSocketServer);
    return await jsExecutor.getValue('document.title');
  }
  
  static async clickElement(continuum, selector) {
    const jsExecutor = new PromiseJSExecutor(continuum.webSocketServer);
    const code = `document.querySelector("${selector}")?.click()`;
    return await jsExecutor.run(code);
  }
}

module.exports = PromiseJSCommand;