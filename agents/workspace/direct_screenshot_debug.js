#!/usr/bin/env node
/**
 * Direct Screenshot Debug - Test screenshot via direct WebSocket
 */

import { quickJsExecute } from './ClientConnection.js';

async function directScreenshotDebug() {
    console.log("🔧 DIRECT SCREENSHOT DEBUG TEST");
    console.log("Testing screenshot via direct browser console commands...");
    
    // Test 1: Check WebSocket connection
    const wsTest = await quickJsExecute(`
        console.log("🔍 WebSocket connection test:");
        console.log("window.ws exists:", !!window.ws);
        console.log("WebSocket readyState:", window.ws ? window.ws.readyState : "undefined");
        console.log("WebSocket.OPEN constant:", WebSocket.OPEN);
        
        "WS_TEST_COMPLETE";
    `);
    
    if (wsTest.success) {
        console.log("✅ WebSocket test executed");
        console.log("Console output:", wsTest.output?.length || 0, "entries");
    }
    
    // Test 2: Try direct screenshot command via WebSocket
    const directScreenshotTest = await quickJsExecute(`
        console.log("🔍 Sending direct screenshot command via WebSocket...");
        
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            // Send screenshot command directly
            const command = {
                type: 'task',
                role: 'system',
                task: '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
            };
            
            console.log("📤 Sending screenshot command:", JSON.stringify(command));
            window.ws.send(JSON.stringify(command));
            console.log("✅ Screenshot command sent");
            
            "SCREENSHOT_COMMAND_SENT";
        } else {
            console.log("❌ WebSocket not available for screenshot command");
            "WEBSOCKET_NOT_AVAILABLE";
        }
    `);
    
    if (directScreenshotTest.success) {
        console.log("✅ Direct screenshot command sent");
        
        // Wait for potential response
        console.log("⏰ Waiting for screenshot response...");
        await new Promise(resolve => setTimeout(resolve, 3000));
        
        // Check for any console updates
        const followupTest = await quickJsExecute(`
            console.log("🔍 Checking for screenshot response...");
            "FOLLOWUP_CHECK";
        `);
        
        if (followupTest.output && followupTest.output.length > 0) {
            console.log("📋 Follow-up console output:");
            followupTest.output.forEach((entry, index) => {
                console.log(`  ${index + 1}. [${entry.level}] ${entry.message}`);
            });
        }
    }
    
    console.log("\n🎯 DIRECT SCREENSHOT DEBUG COMPLETE");
    console.log("Check server console for debug messages from command routing.");
}

directScreenshotDebug().catch(console.error);