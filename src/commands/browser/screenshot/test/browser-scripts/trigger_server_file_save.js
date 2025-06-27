// Trigger Server-Side File Save for Screenshots
(function() {
console.log("💾 Implementing server-side file save for screenshots...");

// Check version badge and capture screenshot
const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({error: "No version badge found"});
}

if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");
    return JSON.stringify({error: "html2canvas not available"});
}

const version = versionBadge.textContent.trim();
console.log("✅ Capturing screenshot for version:", version);

// Capture and send to server for file saving
html2canvas(versionBadge, {
    allowTaint: true,
    useCORS: true,
    scale: 2,
    backgroundColor: "#ffffff"
}).then(function(canvas) {
    
    const dataURL = canvas.toDataURL('image/png');
    const timestamp = Date.now();
    const filename = `version_${version}_${timestamp}.png`;
    const base64Data = dataURL.split(',')[1];
    const byteSize = Math.round((base64Data.length * 3) / 4);
    
    console.log("📸 Screenshot captured:");
    console.log("   📐 Dimensions:", canvas.width + "x" + canvas.height);
    console.log("   💾 Size:", byteSize, "bytes");
    console.log("   🏷️ Filename:", filename);
    
    // Create server file save command
    const serverFileSaveCommand = {
        type: 'server_file_save',
        command: 'save_screenshot',
        data: {
            filename: filename,
            content: base64Data,
            mimeType: 'image/png',
            directory: '.continuum/screenshots',
            metadata: {
                version: version,
                dimensions: {
                    width: canvas.width,
                    height: canvas.height
                },
                byteSize: byteSize,
                timestamp: timestamp,
                source: 'version_capture'
            }
        }
    };
    
    console.log("📤 Sending server file save command...");
    console.log("   📁 Directory: .continuum/screenshots");
    console.log("   🏷️ Filename:", filename);
    console.log("   📊 Data size:", base64Data.length, "chars base64");
    
    // Send via WebSocket to server
    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
        window.ws.send(JSON.stringify(serverFileSaveCommand));
        console.log("✅ Server file save command sent via WebSocket");
        
        // Store for verification
        window.lastServerFileSave = {
            filename: filename,
            version: version,
            byteSize: byteSize,
            timestamp: timestamp,
            sent: true
        };
        
        console.log("🎯 SERVER FILE SAVE INITIATED");
        console.log("   The server should now:");
        console.log("   1. Receive the server_file_save command");
        console.log("   2. Create .continuum/screenshots/ directory if needed");
        console.log("   3. Decode base64 data to binary");
        console.log("   4. Write", filename, "to filesystem");
        console.log("   5. Confirm save success");
        
    } else {
        console.log("❌ WebSocket not available");
        return JSON.stringify({
            error: "WebSocket not connected",
            status: "FAILED"
        });
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot capture failed:", error);
});

// Return immediate status
return JSON.stringify({
    status: "SERVER_FILE_SAVE_INITIATED",
    version: version,
    message: "Screenshot captured and server file save command sent",
    timestamp: Date.now()
});
})();