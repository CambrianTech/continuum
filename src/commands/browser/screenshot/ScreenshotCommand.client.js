/**
 * ScreenshotCommand Client-Side Implementation
 * Companion to ScreenshotCommand.cjs - integrates with fluent API command system
 * Part of the modular command architecture: server, client, and fluent API together
 */

class ScreenshotCommandClient {
    /**
     * Handle screenshot command from server via fluent API command system
     * Integrates with the elegant continuum.screenshot() fluent API
     */
    static async handleCommand(params) {
        console.log('📸 ScreenshotClient: Handling server command', params);
        
        const {
            selector = 'body',
            scale = 1.0,
            filename = `screenshot_${Date.now()}.png`,
            manual = false,
            source = 'unknown',
            timestamp = Date.now()
        } = params;
        
        console.log(`📸 ScreenshotClient: Processing ${selector} → ${filename}`);
        
        // Find target element
        let targetElement = document.querySelector(selector);
        if (!targetElement) {
            console.warn(`⚠️ Element not found: ${selector}, falling back to body`);
            targetElement = document.body;
        }
        
        // Check dependencies
        if (typeof window.ScreenshotUtils === 'undefined') {
            throw new Error('ScreenshotUtils not available');
        }
        
        if (typeof html2canvas === 'undefined') {
            throw new Error('html2canvas not available');
        }
        
        console.log('📸 ScreenshotClient: Delegating to ScreenshotUtils...');
        
        // Delegate to core ScreenshotUtils for execution
        return await window.ScreenshotUtils.takeScreenshotAndSend(targetElement, {
            filename: filename,
            source: source,
            selector: selector
        }, {
            scale: scale,
            source: 'client_orchestration'
        });
    }
    
    /**
     * Handle screenshot command from continuum API
     * Routes through proper command system
     */
    static async handleAPICall(params) {
        console.log('📸 ScreenshotClient: Handling API call', params);
        
        const {
            selector = 'body',
            name_prefix = 'screenshot',
            scale = 1.0,
            manual = false
        } = params;
        
        // Generate filename
        const timestamp = Date.now();
        const filename = `${name_prefix}_${timestamp}.png`;
        
        console.log('📸 ScreenshotClient: Routing API call through command system...');
        
        // Route through server command system (proper orchestration)
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            const screenshotMessage = {
                type: 'command',
                command: 'screenshot',
                params: {
                    selector: selector,
                    filename: filename,
                    scale: scale,
                    source: 'continuum_api',
                    manual: manual,
                    timestamp: timestamp
                }
            };
            
            console.log('📤 ScreenshotClient: Sending to server for orchestration');
            window.ws.send(JSON.stringify(screenshotMessage));
            
            return {
                success: true,
                filename: filename,
                message: 'Screenshot command routed through server orchestration'
            };
        } else {
            throw new Error('WebSocket not available for command routing');
        }
    }
}

// Make available globally
window.ScreenshotCommandClient = ScreenshotCommandClient;

// Register with continuum
if (typeof window.continuum !== 'undefined' && window.continuum.fileVersions) {
    window.continuum.fileVersions['ScreenshotCommand.client.js'] = '1.0.0';
    console.log('📦 ScreenshotCommand.client.js v1.0.0 registered with continuum');
}

console.log('📸 ScreenshotCommand client-side implementation loaded');