/**
 * Browser JavaScript Command
 * Executes JavaScript code in connected browsers
 */

class BrowserJSCommand {
  static getDefinition() {
    return {
      name: 'BROWSER_JS',
      category: 'Core',
      icon: '💻',
      description: 'Execute JavaScript in browser',
      params: '<javascript_code> [encoding]',
      examples: [
        '{"params": "Y29uc29sZS5sb2coJ3Rlc3QnKQ==", "encoding": "base64"}',
        '{"params": "ZG9jdW1lbnQudGl0bGUgPSAnTmV3IFRpdGxlJw==", "encoding": "base64"}',
        '{"params": "ZG9jdW1lbnQucXVlcnlTZWxlY3RvcignLmJ0bicpLmNsaWNrKCk=", "encoding": "base64"}'
      ],
      usage: 'Execute JavaScript in connected browsers. REQUIRES base64 encoding for safety.'
    };
  }
  
  static async execute(params, continuum, encoding = 'base64') {
    console.log('💻 JavaScript command executed with params:', params);
    console.log('💻 Encoding:', encoding);
    
    try {
      // Enforce base64-only encoding for deep space probe safety
      if (encoding !== 'base64') {
        return {
          executed: false,
          error: 'Only base64 encoding is supported. Use base64 to avoid breaking probe communication.',
          encoding_required: 'base64'
        };
      }
      
      let jsCode;
      try {
        jsCode = Buffer.from(params, 'base64').toString('utf8');
        console.log('💻 Decoded base64 JavaScript:', jsCode);
      } catch (error) {
        return {
          executed: false,
          error: 'Invalid base64 encoding: ' + error.message,
          error_stack: error.stack
        };
      }
      
      // Validate JavaScript (basic check)
      if (!jsCode || jsCode.trim() === '') {
        return {
          executed: false,
          error: 'Empty JavaScript code provided'
        };
      }
      
      // Send to browsers via WebSocket and wait for response
      if (continuum.webSocketServer) {
        const executionId = `exec_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        
        // Create promise to wait for browser response
        const executionPromise = new Promise((resolve, reject) => {
          const timeout = setTimeout(() => {
            continuum.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
            reject(new Error('Browser execution timeout (10s)'));
          }, 10000);
          
          const responseHandler = (result) => {
            clearTimeout(timeout);
            continuum.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
            resolve(result);
          };
          
          continuum.webSocketServer.on(`js_result_${executionId}`, responseHandler);
        });
        
        // Send command with execution ID
        continuum.webSocketServer.broadcast({
          type: 'execute_js',
          data: {
            command: jsCode,
            timestamp: new Date().toISOString(),
            encoding: encoding,
            executionId: executionId
          }
        });
        
        // Wait for browser response
        try {
          const browserResult = await executionPromise;
          
          return {
            executed: browserResult.success,
            message: browserResult.success ? 'JavaScript executed successfully' : 'JavaScript execution failed',
            code: jsCode,
            encoding: encoding,
            timestamp: new Date().toISOString(),
            browserResponse: browserResult,
            output: browserResult.output || [],
            result: browserResult.result,
            error: browserResult.error || null
          };
        } catch (timeoutError) {
          return {
            executed: false,
            error: timeoutError.message,
            code: jsCode,
            encoding: encoding,
            timestamp: new Date().toISOString(),
            note: 'Browser did not respond within timeout period'
          };
        }
      }
      
      return {
        executed: false,
        error: 'WebSocket server not available'
      };
      
    } catch (error) {
      console.error('💻 JavaScript execution error:', error);
      return {
        executed: false,
        error: 'Failed to execute JavaScript: ' + error.message,
        error_stack: error.stack
      };
    }
  }
  
  static validateJavaScript(code) {
    // Basic JavaScript validation
    try {
      // Check for obvious syntax issues
      if (code.includes('</script>')) {
        return { valid: false, error: 'Script tags not allowed' };
      }
      
      // Check for potentially dangerous code (basic)
      const dangerousPatterns = [
        /eval\s*\(/,
        /Function\s*\(/,
        /setTimeout\s*\(\s*['"`][^'"`]*['"`]/,
        /setInterval\s*\(\s*['"`][^'"`]*['"`]/
      ];
      
      for (const pattern of dangerousPatterns) {
        if (pattern.test(code)) {
          return { valid: false, error: 'Potentially unsafe JavaScript detected' };
        }
      }
      
      return { valid: true };
    } catch (error) {
      return { valid: false, error: 'JavaScript validation failed: ' + error.message };
    }
  }
  
  static generateSafeWrapper(code) {
    // Wrap JavaScript in try-catch for safe execution
    return `
      try {
        console.log('🔧 Executing JavaScript from Continuum...');
        ${code}
        console.log('✅ JavaScript executed successfully');
      } catch (error) {
        console.error('❌ JavaScript execution failed:', error.message);
        console.error('Stack:', error.stack);
      }
    `;
  }
}

module.exports = BrowserJSCommand;