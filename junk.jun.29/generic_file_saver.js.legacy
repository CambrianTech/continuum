// Generic File Saving Command Processor
console.log("💾 Implementing generic file saving command processor...");

// Generic file saving function that handles any blob data
function saveFileFromServer(data) {
    console.log("💾 Processing file save command...");
    console.log("   📁 Type:", data.type);
    console.log("   🏷️ Filename:", data.filename);
    console.log("   📏 Data size:", data.data ? data.data.length : 0, "chars");
    
    try {
        // Extract the actual file data (could be base64 or blob)
        let fileData = data.data;
        let mimeType = 'application/octet-stream'; // default
        
        // Handle base64 data (like images)
        if (typeof fileData === 'string' && fileData.startsWith('data:')) {
            const parts = fileData.split(',');
            const header = parts[0]; // data:image/png;base64
            const base64Data = parts[1];
            
            // Extract MIME type
            const mimeMatch = header.match(/data:([^;]+)/);
            if (mimeMatch) {
                mimeType = mimeMatch[1];
            }
            
            // Convert base64 to blob
            const binaryString = atob(base64Data);
            const bytes = new Uint8Array(binaryString.length);
            for (let i = 0; i < binaryString.length; i++) {
                bytes[i] = binaryString.charCodeAt(i);
            }
            fileData = new Blob([bytes], { type: mimeType });
            
            console.log("   🔄 Converted base64 to blob");
            console.log("   📊 MIME type:", mimeType);
            console.log("   💾 Blob size:", fileData.size, "bytes");
        }
        
        // Create download link (browser-based file saving)
        const url = URL.createObjectURL(fileData);
        const downloadLink = document.createElement('a');
        downloadLink.href = url;
        downloadLink.download = data.filename;
        downloadLink.style.display = 'none';
        
        // Add to DOM and trigger download
        document.body.appendChild(downloadLink);
        downloadLink.click();
        
        // Clean up
        setTimeout(() => {
            document.body.removeChild(downloadLink);
            URL.revokeObjectURL(url);
        }, 100);
        
        console.log("   ✅ File download triggered");
        
        // Store save record for tracking
        const saveRecord = {
            filename: data.filename,
            size: fileData.size,
            mimeType: mimeType,
            timestamp: Date.now(),
            source: data.source || 'server',
            destination: data.destination || 'downloads'
        };
        
        // Save to localStorage for tracking
        const saveHistory = JSON.parse(localStorage.getItem('fileSaveHistory') || '[]');
        saveHistory.push(saveRecord);
        localStorage.setItem('fileSaveHistory', JSON.stringify(saveHistory.slice(-50))); // Keep last 50
        
        console.log("   📋 Save record stored");
        
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

// Enhanced WebSocket message handler for file operations
function enhanceWebSocketForFileOps() {
    if (typeof handleWebSocketMessage === 'function') {
        console.log("🔧 Enhancing WebSocket handler for file operations...");
        
        const originalHandler = handleWebSocketMessage;
        
        window.handleWebSocketMessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                
                // Handle file save commands
                if (data.type === 'file_save' || data.type === 'save_file') {
                    console.log("💾 FILE SAVE COMMAND DETECTED");
                    return saveFileFromServer(data);
                }
                
                // Handle legacy screenshot_data as file save
                if (data.type === 'screenshot_data') {
                    console.log("📸 Converting screenshot_data to file_save");
                    const fileData = {
                        ...data,
                        type: 'file_save',
                        destination: '.continuum/screenshots/'
                    };
                    return saveFileFromServer(fileData);
                }
                
                // Handle other file types that might come from server
                if (data.type === 'document_save' || data.type === 'export_file') {
                    console.log("📄 DOCUMENT SAVE COMMAND");
                    return saveFileFromServer(data);
                }
                
                // Pass other messages to original handler
                return originalHandler.call(this, event);
                
            } catch (error) {
                console.log("❌ Enhanced handler error:", error);
                return originalHandler.call(this, event);
            }
        };
        
        console.log("✅ WebSocket handler enhanced for file operations");
        return true;
    } else {
        console.log("❌ No handleWebSocketMessage function found");
        return false;
    }
}

// Test the file saving system
function testFileSaving() {
    console.log("🧪 Testing generic file saving system...");
    
    // Test 1: Save a small text file
    const textFileData = {
        type: 'file_save',
        filename: 'test_document.txt',
        data: 'data:text/plain;base64,' + btoa('Hello from Continuum file saver!'),
        source: 'test',
        destination: 'downloads'
    };
    
    console.log("📝 Testing text file save...");
    const textResult = saveFileFromServer(textFileData);
    console.log("   Result:", textResult);
    
    // Test 2: Save a small image file (1x1 pixel)
    const imageFileData = {
        type: 'file_save', 
        filename: 'test_image.png',
        data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        source: 'test',
        destination: 'downloads'
    };
    
    console.log("🖼️ Testing image file save...");
    const imageResult = saveFileFromServer(imageFileData);
    console.log("   Result:", imageResult);
    
    return { textResult, imageResult };
}

// Initialize the file saving system
const initResult = enhanceWebSocketForFileOps();
const testResults = testFileSaving();

// Return comprehensive status
JSON.stringify({
    status: "GENERIC_FILE_SAVER_READY",
    webSocketEnhanced: initResult,
    testResults: testResults,
    capabilities: {
        base64ToBlob: true,
        browserDownload: true,
        mimeTypeDetection: true,
        saveTracking: true
    },
    timestamp: Date.now()
});