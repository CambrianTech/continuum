/**
 * Browser JavaScript Command - TypeScript Implementation
 * Executes JavaScript code in connected browsers with full type safety
 */
import { BaseCommand } from '../../core/BaseCommand';
/**
 * Browser JavaScript Command - Executes JavaScript in connected browsers
 * Supports base64 encoding for security and auto-conversion for convenience
 */
export class BrowserJSCommand extends BaseCommand {
    static getDefinition() {
        return {
            name: 'browserJs',
            category: 'Browser',
            icon: '💻',
            description: 'Execute JavaScript in browser',
            params: '<javascript_code> [encoding]',
            examples: [
                '{"script": "Y29uc29sZS5sb2coJ3Rlc3QnKQ==", "encoding": "base64"}',
                '{"script": "ZG9jdW1lbnQudGl0bGUgPSAnTmV3IFRpdGxlJw==", "encoding": "base64"}',
                '{"script": "ZG9jdW1lbnQucXVlcnlTZWxlY3RvcignLmJ0bicpLmNsaWNrKCk=", "encoding": "base64"}'
            ],
            usage: 'Execute JavaScript in connected browsers. Supports base64 encoding for safety and auto-conversion.'
        };
    }
    static async execute(params, context) {
        this.logExecution('BrowserJS', params, context);
        try {
            const { script, encoding, jsCode } = this.processScriptParams(params);
            if (!jsCode || jsCode.trim() === '') {
                return this.createErrorResult('Empty JavaScript code provided');
            }
            // Validate JavaScript for basic safety
            const validation = this.validateJavaScript(jsCode);
            if (!validation.valid) {
                return this.createErrorResult(`JavaScript validation failed: ${validation.error}`);
            }
            // Execute in browser via WebSocket
            const browserResult = await this.executeBrowserJavaScript(jsCode, encoding, context);
            return this.createSuccessResult('JavaScript execution completed', browserResult);
        }
        catch (error) {
            console.error('💻 JavaScript execution error:', error);
            return this.createErrorResult('Failed to execute JavaScript', error instanceof Error ? error.message : String(error));
        }
    }
    /**
     * Process and normalize script parameters from various input formats
     */
    static processScriptParams(params) {
        let scriptContent;
        let actualEncoding = 'auto';
        // Handle object parameters (from TypeScript CommandProcessor)
        if (typeof params === 'object' && params.script) {
            scriptContent = params.script;
            actualEncoding = params.encoding || 'auto';
            console.log('💻 Using object params - script:', scriptContent, 'encoding:', actualEncoding);
        }
        // Handle JSON string parameters (legacy compatibility)
        else if (typeof params === 'string' && params.startsWith('{')) {
            try {
                const paramObj = JSON.parse(params);
                if (paramObj.script) {
                    scriptContent = paramObj.script;
                    actualEncoding = paramObj.encoding || 'auto';
                    console.log('💻 Using JSON string params - script:', scriptContent, 'encoding:', actualEncoding);
                }
                else {
                    scriptContent = params;
                }
            }
            catch (e) {
                // Not JSON, treat as raw script
                scriptContent = params;
            }
        }
        // Handle raw string parameters
        else if (typeof params === 'string') {
            scriptContent = params;
        }
        else {
            throw new Error('Invalid parameters format');
        }
        // Auto-convert to base64 for probe safety if needed
        if (actualEncoding !== 'base64') {
            console.log('💻 Auto-converting to base64 for probe safety');
            // Check if scriptContent is already base64 encoded
            try {
                const decoded = Buffer.from(scriptContent, 'base64').toString('utf-8');
                const reencoded = Buffer.from(decoded).toString('base64');
                if (reencoded === scriptContent) {
                    console.log('💻 Input appears to be base64 already');
                    actualEncoding = 'base64';
                }
                else {
                    throw new Error('Not base64');
                }
            }
            catch (e) {
                // Not base64, auto-convert plain text
                console.log('💻 Converting plain JavaScript to base64');
                scriptContent = Buffer.from(scriptContent).toString('base64');
                actualEncoding = 'base64';
            }
        }
        // Decode the JavaScript code
        let jsCode;
        try {
            jsCode = Buffer.from(scriptContent, 'base64').toString('utf8');
            console.log('💻 Decoded base64 JavaScript:', jsCode);
        }
        catch (error) {
            throw new Error(`Invalid base64 encoding: ${error instanceof Error ? error.message : String(error)}`);
        }
        return {
            script: scriptContent,
            encoding: actualEncoding,
            jsCode
        };
    }
    /**
     * Execute JavaScript in browser via WebSocket connection
     */
    static async executeBrowserJavaScript(jsCode, encoding, context) {
        const continuum = context?.continuum;
        if (!continuum?.webSocketServer) {
            return {
                executed: false,
                error: 'WebSocket server not available'
            };
        }
        const executionId = `exec_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        // Create promise to wait for browser response
        const executionPromise = new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                continuum.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
                reject(new Error('Browser execution timeout (10s)'));
            }, 10000);
            const responseHandler = (result) => {
                console.log(`💻 BROWSER_JS: Received result for execution ${executionId}:`, result);
                clearTimeout(timeout);
                continuum.webSocketServer.removeListener(`js_result_${executionId}`, responseHandler);
                resolve(result);
            };
            console.log(`💻 BROWSER_JS: Setting up listener for js_result_${executionId}`);
            continuum.webSocketServer.on(`js_result_${executionId}`, responseHandler);
        });
        // Send command to browser tabs
        const message = JSON.stringify({
            type: 'execute_js',
            data: {
                command: jsCode,
                timestamp: new Date().toISOString(),
                encoding: encoding,
                executionId: executionId
            }
        });
        // Send via TabManager if available, otherwise broadcast
        const tabManager = continuum.webSocketServer.tabManager;
        if (tabManager && tabManager.activeTabs) {
            let sentToTabs = 0;
            for (const [tabId, tabData] of tabManager.activeTabs) {
                if (tabData.ws && tabData.ws.readyState === tabData.ws.OPEN) {
                    try {
                        tabData.ws.send(message);
                        sentToTabs++;
                    }
                    catch (error) {
                        console.error(`❌ Failed to send JavaScript to tab ${tabId}:`, error);
                    }
                }
            }
            console.log(`💻 JavaScript sent to ${sentToTabs} browser tabs`);
        }
        else {
            console.warn('⚠️ No TabManager available - falling back to broadcast');
            continuum.webSocketServer.broadcast({
                type: 'execute_js',
                data: {
                    command: jsCode,
                    timestamp: new Date().toISOString(),
                    encoding: encoding,
                    executionId: executionId
                }
            });
        }
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
        }
        catch (timeoutError) {
            return {
                executed: false,
                error: timeoutError instanceof Error ? timeoutError.message : String(timeoutError),
                code: jsCode,
                encoding: encoding,
                timestamp: new Date().toISOString(),
                note: 'Browser did not respond within timeout period'
            };
        }
    }
    /**
     * Validate JavaScript code for basic security and syntax
     */
    static validateJavaScript(code) {
        try {
            // Check for obvious security issues
            if (code.includes('</script>')) {
                return { valid: false, error: 'Script tags not allowed' };
            }
            // Check for potentially dangerous code patterns
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
        }
        catch (error) {
            return {
                valid: false,
                error: `JavaScript validation failed: ${error instanceof Error ? error.message : String(error)}`
            };
        }
    }
    /**
     * Generate safe wrapper for JavaScript execution (utility method)
     */
    static generateSafeWrapper(code) {
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
export default BrowserJSCommand;
//# sourceMappingURL=BrowserJSCommand.js.map