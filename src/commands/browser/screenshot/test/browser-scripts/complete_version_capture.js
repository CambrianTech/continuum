// Complete Version Screenshot Capture with File Saving
(function() {
console.log("📸 Complete version screenshot capture system...");

// Step 1: Implement file saving function
function saveFileFromServer(data) {
    console.log("💾 Processing file save command...");
    console.log("   🏷️ Filename:", data.filename);
    
    try {
        let fileData = data.data;
        let mimeType = 'application/octet-stream';
        
        // Handle base64 data
        if (typeof fileData === 'string' && fileData.startsWith('data:')) {
            const parts = fileData.split(',');
            const header = parts[0];
            const base64Data = parts[1];
            
            const mimeMatch = header.match(/data:([^;]+)/);
            if (mimeMatch) {
                mimeType = mimeMatch[1];
            }
            
            const binaryString = atob(base64Data);
            const bytes = new Uint8Array(binaryString.length);
            for (let i = 0; i < binaryString.length; i++) {
                bytes[i] = binaryString.charCodeAt(i);
            }
            fileData = new Blob([bytes], { type: mimeType });
        }
        
        // Create download link
        const url = URL.createObjectURL(fileData);
        const downloadLink = document.createElement('a');
        downloadLink.href = url;
        downloadLink.download = data.filename;
        downloadLink.style.display = 'none';
        
        document.body.appendChild(downloadLink);
        downloadLink.click();
        
        setTimeout(() => {
            document.body.removeChild(downloadLink);
            URL.revokeObjectURL(url);
        }, 100);
        
        console.log("   ✅ File download triggered");
        
        return {
            success: true,
            message: `File ${data.filename} saved successfully`,
            size: fileData.size,
            mimeType: mimeType
        };
        
    } catch (error) {
        console.log("   ❌ File save failed:", error);
        return {
            success: false,
            message: `File save failed: ${error.message}`
        };
    }
}

// Step 2: Check requirements
if (typeof html2canvas === 'undefined') {
    console.log("❌ html2canvas not available");
    return JSON.stringify({
        error: "html2canvas not available",
        status: "FAILED"
    });
}

const versionBadge = document.querySelector(".version-badge");
if (!versionBadge) {
    console.log("❌ Version badge not found");
    return JSON.stringify({
        error: "Version badge not found",
        status: "FAILED"
    });
}

const version = versionBadge.textContent.trim();
console.log("✅ Found version badge:", version);

// Step 3: Capture screenshot
console.log("📸 Capturing version screenshot...");

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
    
    console.log("📊 Screenshot details:");
    console.log("   📐 Canvas:", canvas.width + "x" + canvas.height);
    console.log("   💾 Size:", byteSize, "bytes");
    console.log("   🏷️ File:", filename);
    console.log("   📄 Version:", version);
    
    // Save file
    const fileSaveData = {
        type: 'file_save',
        filename: filename,
        data: dataURL,
        source: 'version_capture',
        destination: '.continuum/screenshots/',
        metadata: {
            version: version,
            dimensions: { width: canvas.width, height: canvas.height },
            byteSize: byteSize,
            timestamp: timestamp
        }
    };
    
    console.log("💾 Saving file...");
    const saveResult = saveFileFromServer(fileSaveData);
    
    if (saveResult.success) {
        console.log("🎉 VERSION SCREENSHOT SAVED!");
        console.log("   📁 Filename:", filename);
        console.log("   📊 File size:", saveResult.size, "bytes");
        console.log("   🎯 MIME type:", saveResult.mimeType);
        
        // Send to server via WebSocket
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            console.log("📤 Sending to server...");
            window.ws.send(JSON.stringify(fileSaveData));
            console.log("✅ Sent to server via WebSocket");
        } else {
            console.log("⚠️ WebSocket not available for server send");
        }
        
        // Store success info globally for verification
        window.lastVersionScreenshot = {
            filename: filename,
            version: version,
            size: saveResult.size,
            timestamp: timestamp,
            success: true
        };
        
        console.log("✅ COMPLETE SUCCESS - Version screenshot captured and saved!");
        
    } else {
        console.log("❌ File save failed:", saveResult.message);
    }
    
}).catch(function(error) {
    console.log("❌ Screenshot capture failed:", error);
});

// Return immediate status
return JSON.stringify({
    status: "VERSION_CAPTURE_INITIATED",
    version: version,
    message: "Version screenshot capture in progress...",
    timestamp: Date.now()
});
})();