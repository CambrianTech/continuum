// Test Bus File Command Processing
console.log("🧪 Testing bus file command processing...");

// First, let's see what bus commands are available
console.log("📋 Testing known working bus commands first...");

// We know [CMD:BROWSER_JS] works, let's test similar patterns
const testCommands = [
    '[CMD:SAVE_FILE]',
    '[CMD:FILE_SAVE]', 
    '[CMD:WRITE_FILE]',
    '[CMD:SCREENSHOT_SAVE]',
    '[CMD:EXPORT_FILE]'
];

console.log("🔍 Available commands to test:", testCommands);

// Try a simple file save command
const simpleFileTest = {
    filename: 'test_bus_file.txt',
    directory: '.continuum/screenshots',
    content: btoa('Hello from bus file command test!'), // base64 encode
    mimeType: 'text/plain'
};

const testCommand = `[CMD:SAVE_FILE] ${JSON.stringify(simpleFileTest)}`;
console.log("🧪 Test command:", testCommand.substring(0, 100) + "...");

// Try to send it
if (window.ws && window.ws.readyState === WebSocket.OPEN) {
    console.log("📤 Sending test file save command...");
    
    const message = {
        type: 'task',
        role: 'system',
        task: testCommand
    };
    
    window.ws.send(JSON.stringify(message));
    console.log("✅ Test command sent");
    
    // Also test if we can capture a screenshot and get the data
    const versionBadge = document.querySelector(".version-badge");
    if (versionBadge && typeof html2canvas !== 'undefined') {
        console.log("📸 Testing screenshot capture for bus command...");
        
        html2canvas(versionBadge, {
            allowTaint: true,
            useCORS: true,
            scale: 1, // smaller for testing
            backgroundColor: "#ffffff"
        }).then(function(canvas) {
            const dataURL = canvas.toDataURL('image/png');
            const base64Data = dataURL.split(',')[1];
            const timestamp = Date.now();
            const filename = `test_version_${timestamp}.png`;
            
            console.log("📸 Screenshot captured for bus test:");
            console.log("   📐 Size:", canvas.width + "x" + canvas.height);
            console.log("   💾 Base64 length:", base64Data.length);
            console.log("   🏷️ Filename:", filename);
            
            const screenshotFileTest = {
                filename: filename,
                directory: '.continuum/screenshots',
                content: base64Data,
                mimeType: 'image/png'
            };
            
            const screenshotCommand = `[CMD:SAVE_FILE] ${JSON.stringify(screenshotFileTest)}`;
            
            console.log("📤 Sending screenshot bus command...");
            const screenshotMessage = {
                type: 'task',
                role: 'system', 
                task: screenshotCommand
            };
            
            window.ws.send(JSON.stringify(screenshotMessage));
            console.log("✅ Screenshot bus command sent");
            
        }).catch(function(error) {
            console.log("❌ Screenshot failed:", error);
        });
    }
    
} else {
    console.log("❌ WebSocket not available");
}

// Return test status
JSON.stringify({
    status: "BUS_FILE_COMMAND_TEST_SENT",
    commands_tested: testCommands,
    websocket_ready: !!(window.ws && window.ws.readyState === WebSocket.OPEN),
    test_files: ['test_bus_file.txt', 'test_version_screenshot.png'],
    timestamp: Date.now()
});