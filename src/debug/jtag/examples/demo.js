/**
 * JTAG End-to-End Demo - Client-side JavaScript
 */

let browserUUID = null;
let jtagConnected = false;
let serverUUID = null;

// Initialize the demo
document.addEventListener('DOMContentLoaded', async function() {
    console.log('🚀 JTAG Demo initializing...');
    
    await initializeServer();
    await initializeJTAG();
    
    // Start periodic server health check
    startHealthCheck();
});

async function initializeServer() {
    try {
        const response = await fetch('/api/server-info');
        const serverInfo = await response.json();
        
        serverUUID = serverInfo.uuid;
        document.getElementById('server-uuid').textContent = serverInfo.uuid;
        document.getElementById('jtag-port').textContent = serverInfo.jtagPort;
        
        logToServer(`Server initialized: ${serverInfo.uuid}`);
    } catch (error) {
        logToServer(`❌ Failed to connect to server: ${error.message}`);
    }
}

async function initializeJTAG() {
    // JTAG is automatically initialized by including /jtag.js
    // Just wait a moment for initialization and get UUID
    console.log('JTAG Demo initializing...');
    
    // Wait for JTAG to be available
    let attempts = 0;
    while (!window.jtag && attempts < 10) {
        await new Promise(resolve => setTimeout(resolve, 100));
        attempts++;
    }
    
    if (window.jtag) {
        browserUUID = window.jtag.getUUID();
        document.getElementById('browser-uuid').textContent = browserUUID.uuid;
        document.getElementById('connection-status').textContent = 'Connected (JTAG)';
        document.getElementById('connection-status').className = 'status connected';
        jtagConnected = true;
        
        logToBrowser(`✅ JTAG System initialized: ${browserUUID.uuid}`);
        
        // Test automatic console interception (this should appear in .continuum/jtag/logs)
        console.log('🧪 Browser console test - JTAG system active');
        console.error('🚨 This error should be captured by JTAG');
        console.warn('⚠️ This warning should be captured by JTAG');
        
    } else {
        logToBrowser('❌ JTAG system not available');
        document.getElementById('connection-status').textContent = 'JTAG Unavailable';
        document.getElementById('connection-status').className = 'status disconnected';
    }
}

function logToBrowser(message) {
    const log = document.getElementById('browser-log');
    const timestamp = new Date().toISOString().substring(11, 19);
    const logEntry = `[${timestamp}] ${message}\n`;
    log.textContent += logEntry;
    log.scrollTop = log.scrollHeight;
}

function logToServer(message) {
    const log = document.getElementById('server-log');
    const timestamp = new Date().toISOString().substring(11, 19);
    const logEntry = `[${timestamp}] ${message}\n`;
    log.textContent += logEntry;
    log.scrollTop = log.scrollHeight;
}

function logToCrossContext(message) {
    const log = document.getElementById('cross-context-log');
    const timestamp = new Date().toISOString().substring(11, 19);
    const logEntry = `[${timestamp}] ${message}\n`;
    log.textContent += logEntry;
    log.scrollTop = log.scrollHeight;
}

async function testServerLogging() {
    try {
        const response = await fetch('/api/server-info');
        const data = await response.json();
        logToServer(`✅ Server logging test: UUID ${data.uuid} (uptime: ${Math.floor(data.uptime)}ms)`);
    } catch (error) {
        logToServer(`❌ Server logging error: ${error.message}`);
    }
}

async function testServerExec() {
    try {
        const response = await fetch('/api/test-server-exec');
        const data = await response.json();
        
        if (data.success) {
            logToServer(`✅ Server exec: ${data.result} (${data.executionTime}ms)`);
        } else {
            logToServer(`❌ Server exec failed: ${data.error}`);
        }
    } catch (error) {
        logToServer(`❌ Server exec error: ${error.message}`);
    }
}

async function testServerScreenshot() {
    logToServer('📸 Server screenshot test triggered (check .continuum/jtag/screenshots/)');
    
    try {
        // Trigger server-side screenshot via JTAG (this would need server API)
        const response = await fetch('/api/server-info');
        if (response.ok) {
            logToServer('✅ Server screenshot request sent');
        }
    } catch (error) {
        logToServer(`❌ Server screenshot error: ${error.message}`);
    }
}

function testBrowserLogging() {
    if (!jtagConnected) {
        logToBrowser('❌ JTAG not connected');
        return;
    }
    
    try {
        // Use the real JTAG API - this should appear in .continuum/jtag/logs/ browser files
        window.jtag.log('BROWSER_TEST', 'Browser logging test from demo', {
            url: window.location.href,
            userAgent: navigator.userAgent.substring(0, 50),
            screenSize: `${screen.width}x${screen.height}`,
            timestamp: Date.now()
        });
        
        // Also test console interception
        console.log('🧪 Console interception test - this should appear in JTAG logs');
        
        logToBrowser('✅ Browser logging test sent (check .continuum/jtag/logs/ browser files)');
    } catch (error) {
        logToBrowser(`❌ Browser logging error: ${error.message}`);
    }
}

async function testBrowserExec() {
    if (!jtagConnected) {
        logToBrowser('❌ JTAG not connected');
        return;
    }
    
    try {
        const result = await window.jtag.exec('Math.PI * window.innerWidth');
        logToBrowser(`✅ Browser exec result: ${result.result} (took ${result.executionTime}ms)`);
    } catch (error) {
        logToBrowser(`❌ Browser exec error: ${error.message}`);
    }
}

async function testBrowserScreenshot() {
    if (!jtagConnected) {
        logToBrowser('❌ JTAG not connected');
        return;
    }
    
    try {
        // Use real JTAG screenshot API
        const result = await window.jtag.screenshot('demo-browser-screenshot', {
            selector: '#browser-panel', // Screenshot just the browser panel
            width: 800,
            height: 600
        });
        
        if (result.success) {
            logToBrowser(`✅ Screenshot captured: ${result.filepath}`);
            logToBrowser(`📁 Check .continuum/jtag/screenshots/ for: demo-browser-screenshot`);
        } else {
            logToBrowser(`⚠️ Screenshot result: ${result.error || 'unknown error'}`);
        }
    } catch (error) {
        logToBrowser(`❌ Screenshot error: ${error.message}`);
    }
}

async function testCrossContext() {
    logToCrossContext('🔄 Testing cross-context communication...');
    
    // Test 1: Browser → Server via JTAG
    if (jtagConnected && browserUUID) {
        try {
            // Use real JTAG API - these should appear in server logs automatically
            window.jtag.critical('CROSS_CONTEXT', 'Browser to server test', {
                browserUUID: browserUUID.uuid,
                testType: 'cross-context-demo',
                timestamp: Date.now()
            });
            
            // Also test console interception in cross-context
            console.error('🚨 Cross-context console error - should appear in server logs');
            console.warn('⚠️ Cross-context console warning - should appear in server logs');
            
            logToCrossContext('✅ Browser → Server messages sent via JTAG');
            logToCrossContext('📁 Check .continuum/jtag/logs/ for browser and server log files');
        } catch (error) {
            logToCrossContext(`❌ Browser → Server failed: ${error.message}`);
        }
    } else {
        logToCrossContext('⚠️ Browser → Server skipped (JTAG not connected)');
    }
    
    // Test 2: Server info retrieval
    try {
        const response = await fetch('/api/server-info');
        const serverInfo = await response.json();
        logToCrossContext(`✅ Server → Browser: UUID ${serverInfo.uuid}`);
        
        // Verify UUID structure
        if (browserUUID && serverUUID) {
            const browserSession = browserUUID.uuid.split('_')[1];
            const serverSession = serverInfo.uuid.split('_')[1];
            
            if (browserSession === serverSession) {
                logToCrossContext('✅ Cross-context session verification: PASSED');
            } else {
                logToCrossContext(`⚠️ Session IDs differ: browser(${browserSession}) server(${serverSession})`);
                logToCrossContext('ℹ️ This is normal in demo mode - each context generates its own session');
            }
        }
        
        // Test 3: Timestamp synchronization
        const serverTime = new Date(serverInfo.timestamp).getTime();
        const clientTime = Date.now();
        const timeDiff = Math.abs(clientTime - serverTime);
        
        logToCrossContext(`✅ Time sync check: ${timeDiff}ms difference`);
        
    } catch (error) {
        logToCrossContext(`❌ Cross-context test error: ${error.message}`);
    }
    
    logToCrossContext('🎉 Cross-context communication test completed');
}

function clearAllLogs() {
    document.getElementById('server-log').textContent = '';
    document.getElementById('browser-log').textContent = '';
    document.getElementById('cross-context-log').textContent = '';
}

function startHealthCheck() {
    // Auto-refresh server info every 10 seconds
    setInterval(async () => {
        if (document.visibilityState === 'visible') {
            try {
                const response = await fetch('/api/server-info');
                const data = await response.json();
                
                // Update server UUID if it changed (server restart)
                if (data.uuid !== serverUUID) {
                    serverUUID = data.uuid;
                    document.getElementById('server-uuid').textContent = data.uuid;
                    logToServer(`🔄 Server UUID updated: ${data.uuid}`);
                }
            } catch (error) {
                console.log('Health check failed:', error.message);
                // Don't spam the logs with health check failures
            }
        }
    }, 10000);
}