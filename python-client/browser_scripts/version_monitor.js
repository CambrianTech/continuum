// Version Monitor Script - Tracks version increments
console.log("👁️ Version Monitor - Checking for increments...");

// Get current version
const versionBadge = document.querySelector(".version-badge");
const currentVersion = versionBadge ? versionBadge.textContent.trim() : "NO_VERSION";

// Check stored previous version
const previousVersion = localStorage.getItem('continuum_last_version');

if (!previousVersion) {
    // First run - store current version
    localStorage.setItem('continuum_last_version', currentVersion);
    console.log("🏁 Initial version stored:", currentVersion);
    
    return JSON.stringify({
        status: "INITIAL_VERSION_STORED",
        version: currentVersion,
        timestamp: new Date().toISOString()
    });
    
} else if (currentVersion !== previousVersion) {
    // Version increment detected!
    console.log("🎉 VERSION INCREMENT DETECTED!");
    console.log("   Previous:", previousVersion);
    console.log("   Current:", currentVersion);
    
    // Update stored version
    localStorage.setItem('continuum_last_version', currentVersion);
    
    // Test capabilities after increment
    const postIncrementTest = {
        versionIncremented: true,
        previousVersion: previousVersion,
        currentVersion: currentVersion,
        capabilities: {
            websocket: !!window.ws && window.ws.readyState === WebSocket.OPEN,
            console: !!console,
            dom: !!document,
            versionBadge: !!versionBadge
        },
        allSystemsOnline: !!(window.ws && console && document && versionBadge),
        timestamp: new Date().toISOString()
    };
    
    console.log("✅ Post-increment validation:", JSON.stringify(postIncrementTest, null, 2));
    
    return JSON.stringify({
        status: "VERSION_INCREMENTED",
        data: postIncrementTest
    });
    
} else {
    // No change
    console.log("📊 Version stable:", currentVersion);
    
    return JSON.stringify({
        status: "VERSION_STABLE", 
        version: currentVersion,
        timestamp: new Date().toISOString()
    });
}