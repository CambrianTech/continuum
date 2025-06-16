// Bus Command for Server-Side File Save
console.log("🚌 Using bus command for server-side file save...");

// Check version badge
const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({error: "No version badge"});
}

if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");  
    return JSON.stringify({error: "html2canvas not available"});
}

const version = versionBadge.textContent.trim();
console.log("✅ Capturing for version:", version);

// Capture screenshot and send via bus command
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
    
    console.log("📸 Screenshot ready for bus command:");
    console.log("   📐 Size:", canvas.width + "x" + canvas.height);
    console.log("   💾 Bytes:", byteSize);
    console.log("   🏷️ File:", filename);
    
    // Send via bus command to server file processor
    const busFileCommand = {
        type: 'task',
        role: 'system', 
        task: `[CMD:SAVE_FILE] {"filename":"${filename}","directory":".continuum/screenshots","content":"${base64Data}","mimeType":"image/png","metadata":{"version":"${version}","dimensions":{"width":${canvas.width},"height":${canvas.height}},"byteSize":${byteSize},"timestamp":${timestamp}}}`
    };
    
    console.log("🚌 Sending bus file save command...");
    console.log("   📁 Target: .continuum/screenshots/" + filename);
    console.log("   📊 Command length:", busFileCommand.task.length);
    
    // Send through WebSocket (bus system)
    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
        window.ws.send(JSON.stringify(busFileCommand));
        console.log("✅ Bus file save command sent!");
        console.log("   🎯 Server should receive [CMD:SAVE_FILE] and write to filesystem");
        
        // Store for tracking
        window.lastBusFileSave = {
            filename: filename,
            version: version,
            byteSize: byteSize,
            timestamp: timestamp,
            commandSent: true
        };
        
    } else {
        console.log("❌ WebSocket not connected");
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot failed:", error);
});

// Return status
JSON.stringify({
    status: "BUS_FILE_SAVE_INITIATED",
    version: version,
    message: "Bus command [CMD:SAVE_FILE] sent to server",
    timestamp: Date.now()
});