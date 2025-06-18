// Check Server Reboot and File Handling Setup
console.log("🔄 Checking server reboot and file handling setup...");

// Check WebSocket connection state
if (window.ws) {
    console.log("📡 WebSocket connection status:");
    console.log("   🔗 URL:", window.ws.url);
    console.log("   📊 Ready state:", window.ws.readyState);
    console.log("   🔄 Buffer:", window.ws.bufferedAmount);
    
    // Send a connection probe to see server response
    const connectionProbe = {
        type: 'connection_probe',
        client_type: 'browser',
        capabilities: ['file_save', 'screenshot_capture'],
        timestamp: Date.now(),
        message: 'Client requesting server file handling setup'
    };
    
    console.log("📤 Sending connection probe to server...");
    window.ws.send(JSON.stringify(connectionProbe));
    
    // Test file server readiness
    const fileServerTest = {
        type: 'file_server_test',
        test_type: 'readiness_check',
        timestamp: Date.now()
    };
    
    console.log("🧪 Testing file server readiness...");
    window.ws.send(JSON.stringify(fileServerTest));
    
} else {
    console.log("❌ No WebSocket connection");
}

// Check if server has proper file handling setup
const serverCheck = {
    hasNodeFS: typeof require !== 'undefined',
    hasFileHandling: !!window.handleFileOperations,
    hasDirectorySetup: !!window.ensureDirectories,
    hasCommandProcessor: !!window.processFileCommands
};

console.log("🔍 Server file handling capabilities:");
for (const [key, value] of Object.entries(serverCheck)) {
    console.log(`   ${key}: ${value ? '✅' : '❌'}`);
}

// Look for server initialization functions
const serverFunctions = [];
for (let key in window) {
    if (key.includes('init') || key.includes('setup') || key.includes('start')) {
        serverFunctions.push(key);
    }
}

if (serverFunctions.length > 0) {
    console.log("🔧 Found server setup functions:");
    serverFunctions.forEach(func => console.log("   -", func));
}

// Check for file system directories that should exist
const expectedDirs = [
    '.continuum',
    '.continuum/screenshots', 
    '.continuum/files',
    '.continuum/logs'
];

console.log("📁 Expected server directories:");
expectedDirs.forEach(dir => {
    console.log("   📂", dir, "(should be created on server startup)");
});

// Return status for server reboot handling
JSON.stringify({
    status: "SERVER_REBOOT_CHECK_COMPLETE",
    websocketConnected: !!window.ws && window.ws.readyState === WebSocket.OPEN,
    fileHandlingCapabilities: serverCheck,
    serverFunctions: serverFunctions,
    expectedDirectories: expectedDirs,
    recommendation: "Server should initialize file handling on client connect",
    timestamp: Date.now()
});