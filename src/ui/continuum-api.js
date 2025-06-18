/**
 * Continuum Browser API
 * Client-side interface for browser connections to Continuum server
 */

// Wait for DOM and other scripts to be ready before initializing
function initializeContinuum() {
    console.warn('🚀 CRITICAL: initializeContinuum() called - browser API starting...');

// Console forwarding to server for complete logging
function setupConsoleForwarding() {
    if (window.consoleForwardingSetup) return; // Avoid duplicate setup
    window.consoleForwardingSetup = true;
    
    // Store original console methods
    const originalConsole = {
        log: console.log,
        warn: console.warn,
        error: console.error,
        info: console.info
    };
    
    // Helper to send console messages to server
    function forwardToServer(level, args) {
        const message = {
            type: 'client_console_log',
            level: level,
            message: args.map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' '),
            timestamp: Date.now(),
            url: window.location.href,
            wsState: window.ws ? window.ws.readyState : 'NO_WS'
        };
        
        // Log EVERYTHING about WebSocket state
        if (!window.ws) {
            originalConsole.error('🚨 CONSOLE FORWARD FAIL: No window.ws exists!');
        } else if (window.ws.readyState !== WebSocket.OPEN) {
            originalConsole.error('🚨 CONSOLE FORWARD FAIL: WebSocket not open, state:', window.ws.readyState);
        } else {
            try {
                window.ws.send(JSON.stringify(message));
                // Don't log successful forwards to prevent feedback loop
            } catch (e) {
                originalConsole.error('🚨 CONSOLE FORWARD ERROR:', e);
            }
        }
    }
    
    // Override console methods to forward to server
    console.log = function(...args) {
        originalConsole.log.apply(console, args);
        forwardToServer('log', args);
    };
    
    console.warn = function(...args) {
        originalConsole.warn.apply(console, args);
        forwardToServer('warn', args);
    };
    
    console.error = function(...args) {
        originalConsole.error.apply(console, args);
        forwardToServer('error', args);
    };
    
    console.info = function(...args) {
        originalConsole.info.apply(console, args);
        forwardToServer('info', args);
    };
    
    // Store originals for potential restoration
    window.originalConsole = originalConsole;
}
    window.continuum = {
    version: '0.2.1987', // Will be updated dynamically
    fileVersions: {
        // Track individual file versions for public UI utilities only
        // ScreenshotUtils.js is private to screenshot command
    },
    clientType: 'browser',
    connected: false,
    
    // Version-aware JavaScript reloading
    checkVersions: function(serverVersions) {
        const needsUpdate = [];
        
        for (const [filename, serverVersion] of Object.entries(serverVersions)) {
            const currentVersion = this.fileVersions[filename];
            if (!currentVersion || currentVersion !== serverVersion) {
                needsUpdate.push({filename, currentVersion, serverVersion});
            }
        }
        
        return needsUpdate;
    },
    
    reloadScripts: function(filesToUpdate) {
        const promises = [];
        
        for (const file of filesToUpdate) {
            console.log(`🔄 Reloading ${file.filename}: ${file.currentVersion} → ${file.serverVersion}`);
            
            const promise = new Promise((resolve, reject) => {
                // Remove old script if it exists
                const oldScript = document.querySelector(`script[data-continuum-file="${file.filename}"]`);
                if (oldScript) {
                    oldScript.remove();
                }
                
                // Load new script
                const script = document.createElement('script');
                script.src = `/src/ui/utils/${file.filename}?v=${file.serverVersion}`;
                script.setAttribute('data-continuum-file', file.filename);
                
                script.onload = () => {
                    console.log(`✅ Reloaded ${file.filename} v${file.serverVersion}`);
                    this.fileVersions[file.filename] = file.serverVersion;
                    resolve();
                };
                
                script.onerror = () => {
                    console.error(`❌ Failed to reload ${file.filename}`);
                    reject(new Error(`Failed to reload ${file.filename}`));
                };
                
                document.head.appendChild(script);
            });
            
            promises.push(promise);
        }
        
        return Promise.all(promises);
    },
    
    start: function() {
        console.log('🚀 window.continuum.start() called');
        console.log('🔍 WEBSOCKET STATE CHECK:');
        console.log('  window.ws exists:', !!window.ws);
        if (window.ws) {
            console.log('  window.ws.readyState:', window.ws.readyState);
            console.log('  WebSocket.OPEN:', WebSocket.OPEN);
            console.log('  WebSocket.CONNECTING:', WebSocket.CONNECTING);
            console.log('  WebSocket.CLOSING:', WebSocket.CLOSING);
            console.log('  WebSocket.CLOSED:', WebSocket.CLOSED);
            console.log('  window.ws.url:', window.ws.url);
        }
        
        return new Promise((resolve, reject) => {
            console.log('🎯 CONTINUUM START: Checking WebSocket readiness...');
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                console.log('✅ CONTINUUM START: WebSocket is ready!');
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
    
    command: {
        screenshot: function(params = {}) {
            console.log(`📸 continuum.command.screenshot() - routing through fluent API system:`, params);
            
            // Route through fluent API command system (proper architecture)
            if (typeof window.ScreenshotCommandClient !== 'undefined') {
                return window.ScreenshotCommandClient.handleAPICall(params);
            } else {
                return Promise.reject(new Error('ScreenshotCommandClient not available in fluent API system'));
            }
        }
    },
    
    api: {
        screenshot: {
            take: function(name = 'browser-validation') {
                console.log(`📸 Legacy API: redirecting to continuum.command.screenshot()`);
                return window.continuum.command.screenshot({ name_prefix: name });
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

    // Set up console forwarding to server
    setupConsoleForwarding();
    
    console.warn('✅ CRITICAL: window.continuum initialization completed successfully!');
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
    
    // Auto screenshot if possible - USE PROPER COMMAND SYSTEM
    if (versionBadge && window.ws && window.ws.readyState === WebSocket.OPEN) {
        console.log('📸 Auto-capturing validation screenshot via proper command system...');
        
        // Use proper command routing instead of direct ScreenshotUtils access
        const message = {
            command: 'SCREENSHOT',
            params: 'selector .version-badge',
            source: 'browser_validation'
        };
        
        window.ws.send(JSON.stringify(message));
        console.log('  ✅ Validation screenshot request sent via proper command system');
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
try {
    console.warn('🔧 CRITICAL: continuum-api.js starting initialization process...');
    
    if (document.readyState === 'loading') {
        console.warn('⏳ DOM loading - will initialize on DOMContentLoaded');
        document.addEventListener('DOMContentLoaded', initializeContinuum);
    } else {
        // DOM already loaded
        console.warn('✅ DOM ready - initializing immediately');
        initializeContinuum();
    }
    
    console.warn('✅ CRITICAL: continuum-api.js loaded and initialization scheduled');
} catch (error) {
    console.error('❌ CRITICAL FAILURE: continuum-api.js initialization failed:', error.message);
    console.error('❌ Stack trace:', error.stack);
}