// Trace Screenshot Flow - Debug where screenshot data is lost
console.log("🔍 Tracing screenshot data flow...");

// Step 1: Check WebSocket connection state
console.log("📡 WebSocket Status:");
if (window.ws) {
    console.log("   ✅ WebSocket exists");
    console.log("   📊 Ready state:", window.ws.readyState);
    console.log("   🔗 URL:", window.ws.url);
    
    // Add event listeners to track messages
    const originalSend = window.ws.send;
    window.ws.send = function(data) {
        console.log("📤 WebSocket SEND intercepted:");
        console.log("   📏 Data length:", data.length);
        console.log("   🔍 Data type:", typeof data);
        
        try {
            const parsed = JSON.parse(data);
            console.log("   📦 Message type:", parsed.type);
            if (parsed.type === 'screenshot_data') {
                console.log("   📸 SCREENSHOT MESSAGE DETECTED!");
                console.log("   🏷️ Filename:", parsed.filename);
                console.log("   📐 Dimensions:", parsed.metadata?.dimensions);
                console.log("   💾 Byte size:", parsed.metadata?.byteSize);
                console.log("   📏 Total message size:", data.length, "chars");
            }
        } catch (e) {
            console.log("   ⚠️ Non-JSON data");
        }
        
        // Call original send
        return originalSend.call(this, data);
    };
    
    // Track incoming messages
    window.ws.addEventListener('message', function(event) {
        console.log("📥 WebSocket MESSAGE received:");
        console.log("   📏 Data length:", event.data.length);
        try {
            const parsed = JSON.parse(event.data);
            console.log("   📦 Message type:", parsed.type || 'unknown');
        } catch (e) {
            console.log("   ⚠️ Non-JSON message");
        }
    });
    
} else {
    console.log("   ❌ No WebSocket connection");
    return JSON.stringify({error: "No WebSocket"});
}

// Step 2: Test screenshot capture with full logging
if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");
    return JSON.stringify({error: "No html2canvas"});
}

const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({error: "No version badge"});
}

console.log("📸 Starting screenshot capture with tracing...");

html2canvas(versionBadge, {
    allowTaint: true,
    useCORS: true,
    scale: 2,
    backgroundColor: "#ffffff"
}).then(function(canvas) {
    console.log("✅ Screenshot canvas created:");
    console.log("   📐 Canvas size:", canvas.width + "x" + canvas.height);
    
    // Convert to data URL
    const dataURL = canvas.toDataURL('image/png');
    console.log("✅ Data URL created:");
    console.log("   📏 Data URL length:", dataURL.length);
    
    const base64Data = dataURL.split(',')[1];
    const byteSize = Math.round((base64Data.length * 3) / 4);
    const timestamp = Date.now();
    const filename = `claude_trace_${timestamp}.png`;
    
    console.log("📊 Screenshot data prepared:");
    console.log("   🏷️ Filename:", filename);
    console.log("   📐 Dimensions:", canvas.width + "x" + canvas.height);
    console.log("   💾 Byte size:", byteSize);
    console.log("   ⏰ Timestamp:", timestamp);
    console.log("   🔍 Base64 preview:", base64Data.substring(0, 50) + "...");
    
    // Create message
    const message = {
        type: 'screenshot_data',
        filename: filename,
        data: dataURL,
        timestamp: timestamp,
        source: 'claude_trace',
        metadata: {
            dimensions: {width: canvas.width, height: canvas.height},
            byteSize: byteSize,
            format: 'image/png',
            scale: 2
        }
    };
    
    const messageStr = JSON.stringify(message);
    console.log("📦 Message prepared:");
    console.log("   📏 Total message size:", messageStr.length, "chars");
    console.log("   🔍 Message structure:", Object.keys(message));
    
    // Send via WebSocket with error handling
    try {
        console.log("📤 Sending screenshot message...");
        window.ws.send(messageStr);
        console.log("✅ Message sent successfully");
        
        // Wait a moment to see if server responds
        setTimeout(function() {
            console.log("⏰ 2 seconds passed - checking for server response...");
        }, 2000);
        
    } catch (error) {
        console.log("❌ WebSocket send failed:", error);
        console.log("   📊 Error details:", error.message);
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot capture failed:", error);
});

// Return immediate status
JSON.stringify({
    status: "SCREENSHOT_TRACE_INITIATED",
    websocket: !!window.ws,
    websocketState: window.ws ? window.ws.readyState : null,
    html2canvas: typeof html2canvas !== 'undefined',
    versionBadge: !!document.querySelector(".version-badge"),
    timestamp: Date.now()
});