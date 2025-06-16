/**
 * Browser Command Handler
 * Handles elegant universal commands like continuum.command.screenshot()
 * ALL ACTIONS ARE LOGGED FOR DEBUGGING
 */

console.log('📋 Loading Browser Command Handler...');

// Command handler initialization
let commandHandlerInitialized = false;

function initializeCommandHandler() {
    if (commandHandlerInitialized) {
        console.log('⚠️ Command handler already initialized');
        return;
    }
    
    console.log('🚀 Initializing Browser Command Handler...');
    
    // Add WebSocket message listener for commands
    if (window.ws) {
        console.log('✅ WebSocket found, adding command message listener');
        
        window.ws.addEventListener('message', handleCommandMessage);
        console.log('📋 Command message listener added to WebSocket');
        
        commandHandlerInitialized = true;
        console.log('🎯 Browser Command Handler initialized successfully');
    } else {
        console.error('❌ No WebSocket found - cannot initialize command handler');
    }
}

function handleCommandMessage(event) {
    try {
        const data = JSON.parse(event.data);
        console.log('📨 Browser received WebSocket message:', data.type);
        
        if (data.type === 'command' && data.command === 'screenshot') {
            console.log('📸 SCREENSHOT COMMAND RECEIVED:', data.params);
            handleScreenshotCommand(data.params);
        } else {
            // Not a command message - ignore quietly
        }
        
    } catch (error) {
        // Ignore JSON parse errors from non-command messages
        console.log('📋 Message parse error (likely non-JSON):', error.message);
    }
}

function handleScreenshotCommand(params) {
    console.log('📸 Processing screenshot command with params:', params);
    
    const {
        selector = 'body',
        scale = 1.0,
        filename = `screenshot_${Date.now()}.png`,
        manual = false,
        source = 'unknown',
        timestamp = Date.now()
    } = params;
    
    console.log(`📸 Screenshot details: ${selector} -> ${filename} (scale: ${scale}, manual: ${manual})`);
    console.log(`📸 Source: ${source}, Timestamp: ${timestamp}`);
    
    // Find target element
    console.log(`🔍 Finding element with selector: ${selector}`);
    let targetElement = document.querySelector(selector);
    
    if (!targetElement) {
        console.warn(`⚠️ Element not found: ${selector}, falling back to body`);
        targetElement = document.body;
    } else {
        console.log(`✅ Found element: ${selector}`, {
            width: targetElement.offsetWidth,
            height: targetElement.offsetHeight
        });
    }
    
    // Check for html2canvas
    if (typeof html2canvas === 'undefined') {
        console.error('❌ html2canvas not available - cannot take screenshot');
        return;
    }
    
    console.log('📸 Starting html2canvas capture...');
    
    // Execute screenshot via modular ScreenshotCommand client
    if (typeof window.ScreenshotCommandClient !== 'undefined') {
        console.log('📸 Executing screenshot via ScreenshotCommand client...');
        window.ScreenshotCommandClient.handleCommand(params).then(() => {
            console.log('✅ Screenshot command executed successfully');
        }).catch(error => {
            console.error('❌ Screenshot command execution failed:', error);
            console.error('❌ Error details:', error.message, error.stack);
        });
    } else {
        console.error('❌ ScreenshotCommandClient not available - cannot execute screenshot');
        console.log('🔍 Available modular commands:', {
            ScreenshotCommandClient: typeof window.ScreenshotCommandClient
        });
    }
}

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function() {
        console.log('📋 DOM loaded - initializing command handler');
        setTimeout(initializeCommandHandler, 100); // Small delay to ensure WebSocket is ready
    });
} else {
    console.log('📋 DOM already loaded - initializing command handler');
    setTimeout(initializeCommandHandler, 100);
}

console.log('📋 Browser Command Handler script loaded');