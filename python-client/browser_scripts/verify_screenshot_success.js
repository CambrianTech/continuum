// Verify Screenshot Success
console.log("🔍 Checking if version screenshot was captured...");

// Check if screenshot was completed
if (window.lastVersionScreenshot) {
    console.log("✅ Found screenshot record:");
    console.log("   📁 Filename:", window.lastVersionScreenshot.filename);
    console.log("   📄 Version:", window.lastVersionScreenshot.version);
    console.log("   📊 Size:", window.lastVersionScreenshot.size, "bytes");
    console.log("   ⏰ Timestamp:", new Date(window.lastVersionScreenshot.timestamp).toLocaleString());
    console.log("   🎯 Success:", window.lastVersionScreenshot.success);
    
    return JSON.stringify({
        status: "SCREENSHOT_FOUND",
        screenshot: window.lastVersionScreenshot
    });
} else {
    console.log("⚠️ No screenshot record found");
}

// Check browser downloads or saved files
const fileSaveHistory = JSON.parse(localStorage.getItem('fileSaveHistory') || '[]');
const recentScreenshots = fileSaveHistory.filter(file => 
    file.filename.includes('version_') && 
    (Date.now() - new Date(file.timestamp).getTime()) < 300000 // Last 5 minutes
);

if (recentScreenshots.length > 0) {
    console.log("📋 Found recent version screenshots:");
    recentScreenshots.forEach(screenshot => {
        console.log("   📁", screenshot.filename);
        console.log("   📊", screenshot.size, "bytes");
        console.log("   ⏰", new Date(screenshot.timestamp).toLocaleString());
    });
    
    return JSON.stringify({
        status: "RECENT_SCREENSHOTS_FOUND", 
        screenshots: recentScreenshots
    });
}

// Check version badge and try a simple screenshot
const versionBadge = document.querySelector(".version-badge");
if (versionBadge) {
    const version = versionBadge.textContent.trim();
    console.log("✅ Version badge found:", version);
    
    // Check if html2canvas is available
    if (typeof html2canvas !== 'undefined') {
        console.log("✅ html2canvas available");
        return JSON.stringify({
            status: "READY_FOR_SCREENSHOT",
            version: version,
            html2canvas: true
        });
    } else {
        console.log("❌ html2canvas not available");
        return JSON.stringify({
            status: "MISSING_HTML2CANVAS",
            version: version
        });
    }
} else {
    console.log("❌ Version badge not found");
    return JSON.stringify({
        status: "NO_VERSION_BADGE"
    });
}

// Default return
JSON.stringify({
    status: "VERIFICATION_COMPLETE",
    timestamp: Date.now()
});