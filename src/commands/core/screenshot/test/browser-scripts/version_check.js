// Version Check Script - Execute in browser via bus command
console.log("🔍 Version Check Script - Starting...");

// Get current version from badge
const versionBadge = document.querySelector(".version-badge");
const currentVersion = versionBadge ? versionBadge.textContent.trim() : "NO_VERSION";

console.log("📋 Current version:", currentVersion);

// Test all client capabilities
const capabilities = {
    version: currentVersion,
    versionBadgeFound: !!versionBadge,
    websocketConnected: !!window.ws && window.ws.readyState === WebSocket.OPEN,
    consoleWorking: !!console,
    domAccess: !!document && !!document.querySelector,
    timestamp: new Date().toISOString(),
    url: window.location.href,
    title: document.title
};

console.log("✅ Version check complete");
console.log("📊 Capabilities:", JSON.stringify(capabilities, null, 2));

// Return results for Python client
"VERSION_CHECK_COMPLETE_" + currentVersion;