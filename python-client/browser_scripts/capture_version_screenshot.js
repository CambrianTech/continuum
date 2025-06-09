// Capture Version Screenshot with File Saving
console.log("📸 Capturing version screenshot with file saving...");

// First ensure our file saver is ready
if (typeof saveFileFromServer !== 'function') {
    console.log("❌ File saver not available - need to run generic_file_saver.js first");
    return JSON.stringify({
        error: "File saver not ready",
        status: "FAILED"
    });
}

// Check for html2canvas
if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");
    return JSON.stringify({
        error: "html2canvas not available", 
        status: "FAILED"
    });
}

// Find version badge
const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({
        error: "Version badge not found",
        status: "FAILED"
    });
}

console.log("✅ Version badge found:", versionBadge.textContent.trim());
console.log("📸 Starting screenshot capture...");

// Capture screenshot
html2canvas(versionBadge, {
    allowTaint: true,
    useCORS: true,
    scale: 2,
    backgroundColor: "#ffffff"
}).then(function(canvas) {
    const dataURL = canvas.toDataURL('image/png');
    const timestamp = Date.now();
    const version = versionBadge.textContent.trim();
    const filename = `version_${version}_${timestamp}.png`;
    
    // Calculate data info
    const base64Data = dataURL.split(',')[1];
    const byteSize = Math.round((base64Data.length * 3) / 4);
    
    console.log("📊 Screenshot captured:");
    console.log("   📐 Dimensions:", canvas.width + "x" + canvas.height);
    console.log("   💾 Data size:", byteSize, "bytes");
    console.log("   🏷️ Filename:", filename);
    console.log("   📄 Version:", version);
    
    // Create file save data
    const fileSaveData = {
        type: 'file_save',
        filename: filename,
        data: dataURL,
        source: 'version_capture',
        destination: '.continuum/screenshots/',
        metadata: {
            version: version,
            dimensions: {
                width: canvas.width,
                height: canvas.height
            },
            byteSize: byteSize,
            timestamp: timestamp,
            captureType: 'version_badge'
        }
    };
    
    console.log("💾 Triggering file save...");
    
    // Save the file using our generic file saver
    const saveResult = saveFileFromServer(fileSaveData);
    
    if (saveResult.success) {
        console.log("✅ VERSION SCREENSHOT SAVED SUCCESSFULLY!");
        console.log("   📁 File:", filename);
        console.log("   📐 Size:", saveResult.size, "bytes");
        console.log("   🎯 MIME:", saveResult.mimeType);
        
        // Also send via WebSocket for server processing
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            console.log("📤 Sending to server via WebSocket...");
            window.ws.send(JSON.stringify(fileSaveData));
            console.log("✅ Sent to server");
        }
        
        return JSON.stringify({
            status: "SUCCESS",
            filename: filename,
            version: version,
            dimensions: {
                width: canvas.width,
                height: canvas.height
            },
            byteSize: byteSize,
            saved: true,
            sentToServer: !!(window.ws && window.ws.readyState === WebSocket.OPEN)
        });
        
    } else {
        console.log("❌ File save failed:", saveResult.message);
        return JSON.stringify({
            status: "SAVE_FAILED",
            error: saveResult.message
        });
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot capture failed:", error);
    return JSON.stringify({
        status: "CAPTURE_FAILED",
        error: error.message
    });
});

// Return immediate status (actual result will come from promise)
JSON.stringify({
    status: "CAPTURE_INITIATED",
    message: "Version screenshot capture in progress...",
    timestamp: Date.now()
});