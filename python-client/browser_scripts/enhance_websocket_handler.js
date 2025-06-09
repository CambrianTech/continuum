// Enhance WebSocket Handler for Screenshot Processing
console.log("🔧 Enhancing WebSocket message handler for screenshot processing...");

// Check if handleWebSocketMessage exists
if (typeof handleWebSocketMessage === 'function') {
    console.log("✅ Found existing handleWebSocketMessage function");
    
    // Store original function
    const originalHandler = handleWebSocketMessage;
    
    // Create enhanced handler
    window.handleWebSocketMessage = function(event) {
        console.log("📥 Enhanced WebSocket handler received message");
        
        try {
            const data = JSON.parse(event.data);
            console.log("📦 Message type:", data.type);
            
            // Handle screenshot data specifically
            if (data.type === 'screenshot_data') {
                console.log("📸 SCREENSHOT DATA DETECTED!");
                console.log("   🏷️ Filename:", data.filename);
                console.log("   📐 Dimensions:", data.metadata?.dimensions);
                console.log("   💾 Byte size:", data.metadata?.byteSize);
                console.log("   ⏰ Timestamp:", data.timestamp);
                
                // Process screenshot data
                const result = processScreenshotData(data);
                console.log("   🔄 Processing result:", result);
                
                return result;
            } else {
                // Call original handler for other message types
                return originalHandler.call(this, event);
            }
            
        } catch (error) {
            console.log("❌ Enhanced handler error:", error);
            // Fallback to original handler
            return originalHandler.call(this, event);
        }
    };
    
    console.log("✅ WebSocket handler enhanced");
    
} else {
    console.log("❌ handleWebSocketMessage function not found");
}

// Create screenshot data processor
function processScreenshotData(data) {
    console.log("🔄 Processing screenshot data...");
    
    try {
        // Extract base64 data
        const base64Data = data.data.split(',')[1];
        const byteSize = Math.round((base64Data.length * 3) / 4);
        
        console.log("   📊 Data extracted:");
        console.log("   📏 Base64 length:", base64Data.length);
        console.log("   💾 Estimated bytes:", byteSize);
        
        // Since we can't save files directly, use alternative storage
        // Store in localStorage for now (could be enhanced to send to server endpoint)
        const storageKey = `screenshot_${data.timestamp}`;
        const metadata = {
            filename: data.filename,
            timestamp: data.timestamp,
            dimensions: data.metadata?.dimensions,
            byteSize: byteSize,
            source: data.source,
            saved: new Date().toISOString()
        };
        
        // Store metadata (not the full image data due to size limits)
        localStorage.setItem(storageKey, JSON.stringify(metadata));
        console.log("   ✅ Screenshot metadata saved to localStorage");
        
        // Log successful processing
        console.log("   🎉 Screenshot processing complete!");
        
        // Could be enhanced to:
        // 1. Send to server endpoint for file saving
        // 2. Use IndexedDB for larger storage
        // 3. Trigger AI analysis of the screenshot
        // 4. Update UI with screenshot status
        
        return {
            success: true,
            message: "Screenshot processed and metadata saved",
            filename: data.filename,
            byteSize: byteSize
        };
        
    } catch (error) {
        console.log("   ❌ Screenshot processing failed:", error);
        return {
            success: false,
            message: "Screenshot processing failed: " + error.message
        };
    }
}

// Test the enhanced handler
console.log("🧪 Testing enhanced WebSocket handler...");

// Create test screenshot data
const testScreenshotData = {
    type: 'screenshot_data',
    filename: 'test_enhanced_handler.png',
    data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
    timestamp: Date.now(),
    source: 'test_enhancement',
    metadata: {
        dimensions: { width: 1, height: 1 },
        byteSize: 100,
        format: 'image/png'
    }
};

// Test processing
const testEvent = {
    data: JSON.stringify(testScreenshotData)
};

if (typeof handleWebSocketMessage === 'function') {
    console.log("🔄 Testing enhanced handler...");
    const result = handleWebSocketMessage(testEvent);
    console.log("🧪 Test result:", result);
}

// Return status
JSON.stringify({
    status: "WEBSOCKET_HANDLER_ENHANCED",
    originalHandlerFound: typeof originalHandler !== 'undefined',
    enhancedHandlerActive: typeof handleWebSocketMessage === 'function',
    processingCapable: typeof processScreenshotData === 'function',
    testCompleted: true,
    timestamp: Date.now()
});