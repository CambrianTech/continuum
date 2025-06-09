/**
 * Continuum Browser API
 * Client-side interface for browser connections to Continuum server
 */

// Wait for DOM and other scripts to be ready before initializing
function initializeContinuum() {
    window.continuum = {
    version: '0.2.1987', // Will be updated dynamically
    clientType: 'browser',
    connected: false,
    
    start: function() {
        console.log('🚀 window.continuum.start() called');
        
        return new Promise((resolve, reject) => {
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                // Send client initialization to create server-side BrowserClientConnection
                const initMessage = {
                    type: 'client_initialize',
                    clientType: 'browser',
                    capabilities: ['screenshot', 'dom_access', 'js_execution', 'validation'],
                    timestamp: Date.now()
                };
                
                try {
                    // Set up listener for server confirmation
                    const handleConnectionConfirm = (event) => {
                        try {
                            const data = JSON.parse(event.data);
                            if (data.type === 'client_connection_confirmed' && data.clientType === 'browser') {
                                console.log('✅ Server confirmed browser client connection');
                                window.continuum.connected = true;
                                window.ws.removeEventListener('message', handleConnectionConfirm);
                                resolve(true);
                            }
                        } catch (e) {
                            // Ignore parse errors for other messages
                        }
                    };
                    
                    // Listen for server confirmation
                    window.ws.addEventListener('message', handleConnectionConfirm);
                    
                    // Send initialization message
                    window.ws.send(JSON.stringify(initMessage));
                    console.log('📤 Browser client initialization sent to server - waiting for confirmation...');
                    
                    // Timeout after 10 seconds
                    setTimeout(() => {
                        window.ws.removeEventListener('message', handleConnectionConfirm);
                        reject(new Error('Server confirmation timeout'));
                    }, 10000);
                    
                } catch (error) {
                    console.error('❌ Failed to send initialization message:', error);
                    reject(error);
                }
            } else {
                console.warn('❌ WebSocket not ready for continuum.start()');
                reject(new Error('WebSocket not ready'));
            }
        });
    },
    
    api: {
        screenshot: {
            take: function(name = 'browser-validation') {
                const timestamp = Date.now();
                const filename = `${name}-${timestamp}.png`;
                console.log(`📸 Taking screenshot: ${filename}`);
                
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    const command = {
                        type: 'task',
                        role: 'system',
                        task: `[CMD:SCREENSHOT] {"format": "png", "filename": "${filename}"}`
                    };
                    window.ws.send(JSON.stringify(command));
                    return filename;
                } else {
                    console.warn('❌ WebSocket not connected for screenshot');
                    return null;
                }
            }
        },
        
        validation: {
            run: function() {
                console.log('🔥 Manual validation triggered');
                return runBrowserValidation();
            }
        }
    }
    };

    console.log('✅ window.continuum initialized');
}

// Browser validation function - runs automatically via server trigger
function runBrowserValidation() {
    console.log('🔥 BROWSER VALIDATION STARTED');
    console.log('⏰ Timestamp:', new Date().toISOString());
    console.log('🌐 User Agent:', navigator.userAgent);
    console.log('📍 URL:', window.location.href);
    console.log('📊 Document ready state:', document.readyState);
    
    // WebSocket validation
    console.log('🔗 WebSocket Status:');
    if (window.ws) {
        console.log('  ✅ WebSocket exists');
        console.log('  📊 Ready State:', window.ws.readyState);
        console.log('  🔗 URL:', window.ws.url);
    } else {
        console.log('  ❌ No WebSocket found');
    }
    
    // Continuum API validation
    console.log('🔧 Continuum API Status:');
    if (window.continuum) {
        console.log('  ✅ window.continuum exists');
        console.log('  📦 Version:', window.continuum.version);
        console.log('  🔗 Connected:', window.continuum.connected);
    } else {
        console.log('  ❌ window.continuum not found');
    }
    
    // Version badge detection
    console.log('🏷️ Version Badge Detection:');
    const versionBadge = document.querySelector('.version-badge');
    if (versionBadge) {
        console.log('  ✅ Version badge found');
        console.log('  📝 Text:', versionBadge.textContent.trim());
        console.log('  📐 Dimensions:', {
            width: versionBadge.offsetWidth,
            height: versionBadge.offsetHeight
        });
    } else {
        console.log('  ⚠️ Version badge not found');
    }
    
    // Test error generation
    console.warn('⚠️ TEST WARNING from browser validation');
    console.error('🔴 TEST ERROR from browser validation');
    
    // Test version reading
    const versionText = versionBadge ? versionBadge.textContent.trim() : 'NO_VERSION_FOUND';
    console.log('📋 VERSION_READ_RESULT:', versionText);
    
    // Auto screenshot if possible
    if (typeof html2canvas !== 'undefined' && versionBadge && window.ws && window.ws.readyState === WebSocket.OPEN) {
        console.log('📸 Auto-capturing validation screenshot...');
        html2canvas(versionBadge, {
            allowTaint: true,
            useCORS: true,
            scale: 1
        }).then(canvas => {
            console.log('  ✅ Validation screenshot successful!');
            console.log('  📐 Canvas size:', canvas.width + 'x' + canvas.height);
            
            const dataURL = canvas.toDataURL('image/png');
            const timestamp = Date.now();
            const filename = `validation-screenshot-${timestamp}.png`;
            
            const screenshotData = {
                type: 'screenshot_data',
                filename: filename,
                dataURL: dataURL,
                timestamp: timestamp,
                source: 'browser_validation',
                dimensions: {
                    width: canvas.width,
                    height: canvas.height
                }
            };
            
            console.log('📤 SENDING VALIDATION SCREENSHOT TO SERVER');
            window.ws.send(JSON.stringify(screenshotData));
            console.log('  ✅ Validation screenshot data sent to server');
            
        }).catch(error => {
            console.log('  ❌ Validation screenshot failed:', error.message);
        });
    }
    
    console.log('🎯 BROWSER VALIDATION COMPLETE');
    return true;
}

// Promise-based WebSocket readiness check
function waitForWebSocket() {
    return new Promise((resolve, reject) => {
        console.log('⏳ Waiting for WebSocket connection...');
        
        // Check if WebSocket is already ready
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            console.log('✅ WebSocket already connected');
            resolve(window.ws);
            return;
        }
        
        // Wait for WebSocket to be assigned and connected
        const checkInterval = setInterval(() => {
            if (window.ws) {
                if (window.ws.readyState === WebSocket.OPEN) {
                    console.log('✅ WebSocket connection established');
                    clearInterval(checkInterval);
                    resolve(window.ws);
                } else if (window.ws.readyState === WebSocket.CLOSED || window.ws.readyState === WebSocket.CLOSING) {
                    console.error('❌ WebSocket connection failed');
                    clearInterval(checkInterval);
                    reject(new Error('WebSocket connection failed'));
                }
                // Continue waiting if CONNECTING (1)
            }
        }, 50); // Check every 50ms
        
        // Timeout after 10 seconds
        setTimeout(() => {
            clearInterval(checkInterval);
            reject(new Error('WebSocket connection timeout'));
        }, 10000);
    });
}

// Promise-based continuum API readiness check
function waitForContinuumAPI() {
    return new Promise((resolve, reject) => {
        console.log('⏳ Waiting for window.continuum to be initialized...');
        
        // Check if continuum is already ready
        if (window.continuum && window.continuum.start) {
            console.log('✅ window.continuum already initialized');
            resolve(window.continuum);
            return;
        }
        
        // Wait for continuum to be initialized
        const checkInterval = setInterval(() => {
            if (window.continuum && window.continuum.start) {
                console.log('✅ window.continuum initialization complete');
                clearInterval(checkInterval);
                resolve(window.continuum);
            }
        }, 50); // Check every 50ms
        
        // Timeout after 5 seconds
        setTimeout(() => {
            clearInterval(checkInterval);
            reject(new Error('window.continuum initialization timeout'));
        }, 5000);
    });
}

// Auto-initialize when connection banner is received
document.addEventListener('continuum-ready', async function() {
    console.log('🎯 Continuum ready event received - waiting for dependencies...');
    console.log('🔍 DEBUG: window.continuum exists:', !!window.continuum);
    console.log('🔍 DEBUG: continuum.start exists:', !!(window.continuum && window.continuum.start));
    
    try {
        // Wait for both WebSocket and continuum API to be ready
        console.log('⏳ Waiting for WebSocket and continuum API...');
        const [ws, continuum] = await Promise.all([
            waitForWebSocket(),
            waitForContinuumAPI()
        ]);
        
        console.log('🚀 All dependencies ready - calling continuum.start()');
        const result = await continuum.start();
        console.log('📤 continuum.start() completed:', result);
        console.log('🎯 Browser client fully connected and validated!');
        
    } catch (error) {
        console.error('❌ Failed to wait for dependencies:', error.message);
    }
});

// Initialize when DOM is ready and after other scripts load
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeContinuum);
} else {
    // DOM already loaded
    initializeContinuum();
}

console.log('✅ continuum-api.js loaded');