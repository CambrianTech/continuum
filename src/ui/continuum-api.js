/**
 * Continuum Browser API
 * Client-side interface for browser connections to Continuum server
 */

window.continuum = {
    version: '0.2.1987', // Will be updated dynamically
    clientType: 'browser',
    connected: false,
    
    start: function() {
        console.log('🚀 window.continuum.start() called');
        
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            // Send client initialization to create server-side BrowserClientConnection
            const initMessage = {
                type: 'client_initialize',
                clientType: 'browser',
                capabilities: ['screenshot', 'dom_access', 'js_execution', 'validation'],
                timestamp: Date.now()
            };
            
            window.ws.send(JSON.stringify(initMessage));
            console.log('📤 Browser client initialization sent to server');
            window.continuum.connected = true;
            return true;
        } else {
            console.warn('❌ WebSocket not ready for continuum.start()');
            return false;
        }
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

// Auto-initialize when connection banner is received
document.addEventListener('continuum-ready', function() {
    console.log('🎯 Continuum ready event - calling continuum.start()');
    window.continuum.start();
});

console.log('✅ continuum-api.js loaded');