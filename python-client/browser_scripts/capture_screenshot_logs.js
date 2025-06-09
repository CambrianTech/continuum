// Capture Screenshot Console Logs - Get the data we just saw
console.log("🔍 Capturing recent screenshot console logs...");

// Check if we can access console history
if (console.history) {
    console.log("✅ Console history available");
    const recentLogs = console.history.slice(-20);
    console.log("📋 Recent console entries:", recentLogs.length);
    return JSON.stringify({
        status: "CONSOLE_HISTORY_FOUND",
        entries: recentLogs
    });
}

// Check for screenshot data in browser storage or variables
const screenshotInfo = {
    lastScreenshotFound: false,
    dimensions: null,
    byteSize: null,
    filename: null,
    timestamp: null
};

// Look for recent screenshot variables or data
if (window.lastScreenshotData) {
    screenshotInfo.lastScreenshotFound = true;
    screenshotInfo = {...screenshotInfo, ...window.lastScreenshotData};
    console.log("✅ Found stored screenshot data");
} else {
    console.log("⚠️ No stored screenshot data found");
}

// Check DOM for recent screenshots
const recentScreenshots = document.querySelectorAll('[data-screenshot-timestamp]');
if (recentScreenshots.length > 0) {
    console.log("✅ Found DOM screenshot markers:", recentScreenshots.length);
}

// Check WebSocket recent messages if logged
if (window.wsMessageLog) {
    const recentMessages = window.wsMessageLog.slice(-10);
    const screenshotMessages = recentMessages.filter(msg => 
        msg.type === 'screenshot_data' || 
        (msg.data && msg.data.includes('screenshot'))
    );
    
    if (screenshotMessages.length > 0) {
        console.log("✅ Found recent screenshot WebSocket messages:", screenshotMessages.length);
        screenshotInfo.webSocketMessages = screenshotMessages.length;
    }
}

console.log("📊 Screenshot capture summary:");
console.log("   Found recent data:", screenshotInfo.lastScreenshotFound);
console.log("   WebSocket state:", window.ws ? window.ws.readyState : 'no_ws');

return JSON.stringify({
    status: "SCREENSHOT_LOG_CAPTURE_COMPLETE", 
    screenshotInfo: screenshotInfo,
    consoleHistoryAvailable: !!console.history,
    timestamp: Date.now()
});