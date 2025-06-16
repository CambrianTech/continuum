// Check Server Logs for Screenshot Messages
console.log("🔍 Checking server logs for screenshot messages...");

// First, send a test WebSocket message to verify connectivity
if (window.ws && window.ws.readyState === WebSocket.OPEN) {
    const testMessage = {
        type: 'debug_trace',
        message: 'Claude checking server logs for screenshot data',
        timestamp: Date.now()
    };
    
    console.log("📤 Sending test message to server...");
    window.ws.send(JSON.stringify(testMessage));
    console.log("✅ Test message sent");
    
    // Check WebSocket state
    console.log("📊 WebSocket details:");
    console.log("   🔗 URL:", window.ws.url);
    console.log("   📊 Ready state:", window.ws.readyState);
    console.log("   🔄 Buffer amount:", window.ws.bufferedAmount);
    
} else {
    console.log("❌ WebSocket not ready");
    console.log("   📊 State:", window.ws ? window.ws.readyState : 'no ws');
}

// Look for server console access
if (window.serverConsole) {
    console.log("✅ Server console found");
    const logs = window.serverConsole.getLogs ? window.serverConsole.getLogs() : [];
    console.log("📋 Server logs found:", logs.length);
} else {
    console.log("⚠️ No server console access");
}

// Check for any error handlers or message logs
if (window.ws) {
    // Add error handler to see if messages fail
    window.ws.addEventListener('error', function(error) {
        console.log("❌ WebSocket error detected:", error);
    });
    
    window.ws.addEventListener('close', function(event) {
        console.log("🔌 WebSocket closed:", event.code, event.reason);
    });
}

// Return status
JSON.stringify({
    status: "SERVER_LOG_CHECK_COMPLETE",
    websocketReady: !!(window.ws && window.ws.readyState === WebSocket.OPEN),
    websocketUrl: window.ws ? window.ws.url : null,
    serverConsole: !!window.serverConsole,
    timestamp: Date.now()
});