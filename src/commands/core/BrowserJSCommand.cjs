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
        '{"params": "console.log(\'test\')"}',
        '{"params": "Y29uc29sZS5sb2coJ3Rlc3QnKQ==", "encoding": "base64"}',
        '{"params": "document.title = \'New Title\'"}',
        '{"params": "document.querySelector(\'.btn\').click()"}'
      ],
      usage: 'Execute JavaScript in connected browsers. Use base64 encoding to avoid quote escaping issues.'
    };
  }
  
  static async execute(params, continuum, encoding = 'utf-8') {
    console.log('💻 JavaScript command executed with params:', params);
    console.log('💻 Encoding:', encoding);
    
    try {
      let jsCode = params;
      
      // Handle base64 encoding
      if (encoding === 'base64') {
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
      }
      
      // Validate JavaScript (basic check)
      if (!jsCode || jsCode.trim() === '') {
        return {
          executed: false,
          error: 'Empty JavaScript code provided'
        };
      }
      
      // Send to browsers via WebSocket
      if (continuum.webSocketServer) {
        continuum.webSocketServer.broadcast({
          type: 'execute_js',
          data: {
            command: jsCode,
            timestamp: new Date().toISOString(),
            encoding: encoding
          }
        });
        
        return {
          executed: true,
          message: 'JavaScript sent to browser (execution pending)',
          code: jsCode,
          encoding: encoding,
          timestamp: new Date().toISOString(),
          note: 'Watch console output to verify actual execution'
        };
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