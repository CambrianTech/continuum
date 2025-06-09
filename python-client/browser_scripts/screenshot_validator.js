// Screenshot Validator - Capture screenshot with proper logging of bytes and dimensions
console.log("📸 Screenshot Validator - Starting capture...");

// Check if screenshot capability exists
if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");
    return JSON.stringify({
        status: "ERROR",
        message: "html2canvas not available"
    });
}

// Find version badge for screenshot
const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({
        status: "ERROR", 
        message: "Version badge not found"
    });
}

console.log("✅ Version badge found, capturing screenshot...");

// Capture screenshot with enhanced logging
html2canvas(versionBadge, {
    allowTaint: true,
    useCORS: true,
    scale: 2,
    backgroundColor: "#ffffff"
}).then(function(canvas) {
    const dataURL = canvas.toDataURL('image/png');
    const timestamp = Date.now();
    const filename = `claude_validation_${timestamp}.png`;
    
    // Calculate data size
    const base64Data = dataURL.split(',')[1];
    const byteSize = Math.round((base64Data.length * 3) / 4);
    const dimensions = {
        width: canvas.width,
        height: canvas.height
    };
    
    console.log("📊 Screenshot captured:");
    console.log("   📐 Dimensions:", dimensions.width + "x" + dimensions.height);
    console.log("   💾 Data size:", byteSize, "bytes");
    console.log("   🏷️ Filename:", filename);
    console.log("   ⏰ Timestamp:", timestamp);
    
    // Log first few chars of base64 data for verification
    const dataPreview = base64Data.substring(0, 50) + "...";
    console.log("   🔍 Data preview:", dataPreview);
    
    // Send via WebSocket if available
    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
        const message = {
            type: 'screenshot_data',
            filename: filename,
            data: dataURL,
            timestamp: timestamp,
            source: 'claude_validation',
            metadata: {
                dimensions: dimensions,
                byteSize: byteSize,
                format: 'image/png',
                scale: 2
            }
        };
        
        window.ws.send(JSON.stringify(message));
        console.log("📤 Screenshot sent to server:", filename);
        console.log("   📊 Message size:", JSON.stringify(message).length, "chars");
        
        return JSON.stringify({
            status: "SUCCESS",
            filename: filename,
            dimensions: dimensions,
            byteSize: byteSize,
            timestamp: timestamp,
            dataPreview: dataPreview
        });
        
    } else {
        console.log("❌ WebSocket not available");
        return JSON.stringify({
            status: "ERROR",
            message: "WebSocket not available",
            dimensions: dimensions,
            byteSize: byteSize
        });
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot capture failed:", error);
    return JSON.stringify({
        status: "ERROR",
        message: "Screenshot capture failed: " + error.message
    });
});

// Return pending status (actual result will come from promise)
JSON.stringify({
    status: "PENDING",
    message: "Screenshot capture in progress..."
});