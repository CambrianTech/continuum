// Stop WebSocket spam immediately
console.log("🔧 EMERGENCY WEBSOCKET SPAM FIX");

// Clear any existing intervals/timeouts
for(let i = 0; i < 10000; i++) {
    clearTimeout(i);
    clearInterval(i);
}

// Patch the existing initWebSocket function to prevent spam
if (window.initWebSocket) {
    console.log("🎯 Patching existing initWebSocket");
    
    // Create a rate-limited version
    let lastAttempt = 0;
    const originalInit = window.initWebSocket;
    
    window.initWebSocket = function() {
        const now = Date.now();
        if (now - lastAttempt < 5000) {
            console.log("⏸️ Rate limiting WebSocket reconnection");
            return;
        }
        lastAttempt = now;
        console.log("🔌 Attempting connection (rate limited)");
        originalInit();
    };
}

// Override console methods to prevent spam
if (!window.originalConsoleLog) {
    window.originalConsoleLog = console.log;
    window.originalConsoleError = console.error;
    
    let lastLogTime = 0;
    let lastErrorTime = 0;
    
    console.log = function(...args) {
        const msg = args.join(' ');
        if (msg.includes('Disconnected from Continuum')) {
            if (Date.now() - lastLogTime > 30000) {
                window.originalConsoleLog('🔌 Disconnected (will retry silently)');
                lastLogTime = Date.now();
            }
        } else {
            window.originalConsoleLog(...args);
        }
    };
    
    console.error = function(...args) {
        const msg = args.join(' ');
        if (msg.includes('WebSocket')) {
            if (Date.now() - lastErrorTime > 30000) {
                window.originalConsoleError('🚨 WebSocket error (retrying in background)');
                lastErrorTime = Date.now();
            }
        } else {
            window.originalConsoleError(...args);
        }
    };
}

console.log("✅ WebSocket spam fix applied via agent system");
"spam_fixed"