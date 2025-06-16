// Check Command Execution Infrastructure
console.log("🔍 Checking server-side command execution infrastructure...");

// Check for command execution patterns
const executionInfo = {
    commandExecutors: [],
    fileOperations: [],
    screenshotSaving: [],
    webSocketProcessing: []
};

// Look for command execution functions
for (let key in window) {
    if (key.includes('execute') || key.includes('Execute')) {
        executionInfo.commandExecutors.push(key);
        console.log("⚡ Found executor:", key);
    }
    if (key.includes('save') || key.includes('Save') || key.includes('write') || key.includes('Write')) {
        executionInfo.fileOperations.push(key);
        console.log("💾 Found file operation:", key);
    }
    if (key.includes('process') || key.includes('Process') || key.includes('handle') || key.includes('Handle')) {
        executionInfo.webSocketProcessing.push(key);
        console.log("🔄 Found processor:", key);
    }
}

// Check for specific screenshot saving functions
if (window.saveScreenshotData) {
    executionInfo.screenshotSaving.push('saveScreenshotData');
    console.log("📸 Found screenshot saver: saveScreenshotData");
}

if (window.handleScreenshotMessage) {
    executionInfo.screenshotSaving.push('handleScreenshotMessage');
    console.log("📸 Found screenshot handler: handleScreenshotMessage");
}

// Check if there's a message processing queue or handler
if (window.messageQueue) {
    console.log("📋 Found message queue");
    executionInfo.hasMessageQueue = true;
}

if (window.commandQueue) {
    console.log("⚡ Found command queue");
    executionInfo.hasCommandQueue = true;
}

// Test if we can create a command processor for screenshot data
const testCommand = {
    type: 'screenshot_data',
    filename: 'test_file.png',
    data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
    timestamp: Date.now()
};

console.log("🧪 Testing command processing capability...");

// Check if we can access file system or Node.js APIs
const nodeCapabilities = {
    fs: typeof require !== 'undefined' ? !!require('fs') : false,
    path: typeof require !== 'undefined' ? !!require('path') : false,
    process: typeof process !== 'undefined'
};

if (nodeCapabilities.fs) {
    console.log("✅ File system access available");
} else {
    console.log("❌ No file system access");
}

// Check for existing screenshot saving implementation
if (window.fs || (typeof require !== 'undefined' && require('fs'))) {
    console.log("✅ Could implement file saving for screenshots");
    executionInfo.canSaveFiles = true;
    
    // Try to access the screenshots directory
    try {
        const fs = window.fs || require('fs');
        const path = window.path || require('path');
        const screenshotsDir = path.join('.continuum', 'screenshots');
        console.log("📁 Screenshots directory path:", screenshotsDir);
        executionInfo.screenshotsPath = screenshotsDir;
    } catch (e) {
        console.log("⚠️ Could not access screenshots directory");
    }
} else {
    console.log("❌ Cannot save files - no fs access");
    executionInfo.canSaveFiles = false;
}

console.log("📊 Command execution infrastructure:", JSON.stringify(executionInfo, null, 2));

// Return comprehensive status
JSON.stringify({
    status: "COMMAND_EXECUTION_CHECK_COMPLETE",
    infrastructure: executionInfo,
    nodeCapabilities: nodeCapabilities,
    canProcessScreenshots: executionInfo.canSaveFiles && executionInfo.screenshotSaving.length > 0,
    timestamp: Date.now()
});