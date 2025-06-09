// Test Screenshot Logging - Check server logs for screenshot data
console.log("🔍 Testing screenshot data flow to server...");

// First check WebSocket connection
if (!window.ws) {
    console.log("❌ No WebSocket connection found");
    return JSON.stringify({error: "No WebSocket connection"});
}

console.log("✅ WebSocket found, state:", window.ws.readyState);

// Test sending a simple message first
const testMessage = {
    type: 'test_message',
    timestamp: Date.now(),
    source: 'claude_test'
};

console.log("📤 Sending test message to server...");
window.ws.send(JSON.stringify(testMessage));

// Now try screenshot if html2canvas is available
if (typeof html2canvas !== 'undefined') {
    const versionBadge = document.querySelector(".version-badge");
    if (versionBadge) {
        console.log("📸 Taking screenshot for server logging test...");
        
        html2canvas(versionBadge, {
            allowTaint: true,
            useCORS: true,
            scale: 1,  // Smaller scale for testing
            backgroundColor: "#ffffff"
        }).then(function(canvas) {
            const dataURL = canvas.toDataURL('image/png');
            const timestamp = Date.now();
            const filename = `claude_test_${timestamp}.png`;
            
            // Calculate size info
            const base64Data = dataURL.split(',')[1];
            const byteSize = Math.round((base64Data.length * 3) / 4);
            
            console.log("📊 Screenshot info:");
            console.log("   Size:", canvas.width + "x" + canvas.height);
            console.log("   Bytes:", byteSize);
            console.log("   Data length:", dataURL.length);
            
            // Send to server
            const screenshotMessage = {
                type: 'screenshot_data',
                filename: filename,
                data: dataURL,
                timestamp: timestamp,
                source: 'claude_test',
                metadata: {
                    width: canvas.width,
                    height: canvas.height,
                    byteSize: byteSize
                }
            };
            
            console.log("📤 Sending screenshot to server...");
            console.log("   Message type:", screenshotMessage.type);
            console.log("   Filename:", screenshotMessage.filename);
            console.log("   Data preview:", dataURL.substring(0, 50) + "...");
            
            window.ws.send(JSON.stringify(screenshotMessage));
            console.log("✅ Screenshot message sent");
            
        }).catch(function(error) {
            console.log("❌ Screenshot failed:", error);
        });
    } else {
        console.log("❌ Version badge not found");
    }
} else {
    console.log("❌ html2canvas not available");
}

// Return immediate status
JSON.stringify({
    status: "SCREENSHOT_TEST_INITIATED",
    websocket: !!window.ws,
    html2canvas: typeof html2canvas !== 'undefined',
    timestamp: Date.now()
});