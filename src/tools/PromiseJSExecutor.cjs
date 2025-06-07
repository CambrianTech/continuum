/**
 * Promise-based JavaScript Execution Tool
 * Provides a clean API for agents to execute JavaScript with promise support
 */

class PromiseJSExecutor {
  constructor(webSocketServer) {
    this.webSocketServer = webSocketServer;
    this.executionCounter = 0;
  }

  /**
   * Execute JavaScript code with promise support
   * @param {string} jsCode - JavaScript code to execute
   * @param {Object} options - Execution options
   * @returns {Promise} Promise that resolves with execution result
   */
  async execute(jsCode, options = {}) {
    const {
      timeout = 10000,
      encoding = 'utf8',
      expectReturn = false
    } = options;

    if (!this.webSocketServer) {
      throw new Error('WebSocket server not available');
    }

    const executionId = `exec_${Date.now()}_${++this.executionCounter}`;
    
    // Create promise to wait for browser response
    const executionPromise = new Promise((resolve, reject) => {
      const timeoutHandle = setTimeout(() => {
        this.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
        reject(new Error(`JavaScript execution timeout (${timeout}ms)`));
      }, timeout);
      
      const responseHandler = (result) => {
        clearTimeout(timeoutHandle);
        this.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
        resolve(result);
      };
      
      this.webSocketServer.on(`js_result_${executionId}`, responseHandler);
    });

    // Prepare JavaScript code with promise wrapper if needed
    let finalCode = jsCode;
    if (expectReturn && !jsCode.trim().startsWith('return')) {
      // Wrap code that should return a value
      finalCode = `return (${jsCode});`;
    }

    // Send command with execution ID
    this.webSocketServer.broadcast({
      type: 'execute_js_promise',
      data: {
        command: finalCode,
        timestamp: new Date().toISOString(),
        encoding: encoding,
        executionId: executionId,
        promiseMode: true
      }
    });

    try {
      const browserResult = await executionPromise;
      
      return {
        success: browserResult.success,
        result: browserResult.result,
        output: browserResult.output || [],
        error: browserResult.error || null,
        executionTime: browserResult.executionTime || 0,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        result: null,
        output: [],
        error: error.message,
        executionTime: 0,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Execute JavaScript and expect a return value
   * @param {string} jsCode - JavaScript expression or code
   * @returns {Promise} Promise that resolves with the return value
   */
  async getValue(jsCode) {
    const result = await this.execute(jsCode, { expectReturn: true });
    if (result.success) {
      return result.result;
    } else {
      throw new Error(result.error || 'JavaScript execution failed');
    }
  }

  /**
   * Execute JavaScript primarily for side effects (DOM manipulation, etc.)
   * @param {string} jsCode - JavaScript code to execute
   * @returns {Promise} Promise that resolves when execution completes
   */
  async run(jsCode) {
    const result = await this.execute(jsCode);
    if (!result.success) {
      throw new Error(result.error || 'JavaScript execution failed');
    }
    return result.output;
  }

  /**
   * Query DOM elements and return data
   * @param {string} selector - CSS selector
   * @param {string} property - Property to extract (optional)
   * @returns {Promise} Promise that resolves with DOM data
   */
  async queryDOM(selector, property = null) {
    let jsCode;
    if (property) {
      jsCode = `
        const elements = Array.from(document.querySelectorAll("${selector}"));
        return elements.map(el => el.${property});
      `;
    } else {
      jsCode = `
        const elements = Array.from(document.querySelectorAll("${selector}"));
        return elements.map(el => ({
          tagName: el.tagName,
          textContent: el.textContent?.trim(),
          innerHTML: el.innerHTML,
          attributes: Array.from(el.attributes).reduce((acc, attr) => {
            acc[attr.name] = attr.value;
            return acc;
          }, {})
        }));
      `;
    }

    return await this.getValue(jsCode);
  }

  /**
   * Wait for an element to appear in the DOM
   * @param {string} selector - CSS selector to wait for
   * @param {number} timeout - Maximum wait time in ms
   * @returns {Promise} Promise that resolves when element appears
   */
  async waitForElement(selector, timeout = 5000) {
    const jsCode = `
      return new Promise((resolve, reject) => {
        const checkElement = () => {
          const element = document.querySelector("${selector}");
          if (element) {
            resolve(true);
          } else {
            setTimeout(checkElement, 100);
          }
        };
        
        setTimeout(() => reject(new Error('Element not found within timeout')), ${timeout});
        checkElement();
      });
    `;

    return await this.getValue(jsCode);
  }

  /**
   * Execute multiple JavaScript operations in sequence
   * @param {Array} operations - Array of {code, expectReturn} objects
   * @returns {Promise} Promise that resolves with array of results
   */
  async batch(operations) {
    const results = [];
    for (const op of operations) {
      const result = await this.execute(op.code, { expectReturn: op.expectReturn });
      results.push(result);
      if (!result.success) {
        break; // Stop on first failure
      }
    }
    return results;
  }
}

module.exports = PromiseJSExecutor;