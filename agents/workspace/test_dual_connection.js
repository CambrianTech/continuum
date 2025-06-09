#!/usr/bin/env node
/**
 * Test dual connection - Claude connecting through both Python and JavaScript
 */

import { ClaudeAgentConnection, quickAgentMessage } from './ClientConnection.js';

async function testDualConnection() {
    console.log("🔄 Testing Claude connection through both Python and JavaScript...");
    
    // Test JavaScript connection
    console.log("\n📱 JavaScript ClientConnection Test:");
    try {
        const jsResult = await quickAgentMessage("Hello! I'm Claude connecting through JavaScript ClientConnection to test cross-platform compatibility.", "GeneralAI");
        console.log("✅ JavaScript result:", JSON.stringify(jsResult, null, 2));
    } catch (error) {
        console.log("❌ JavaScript error:", error.message);
    }
    
    // Test Claude-specific connection
    console.log("\n🤖 Claude-specific Connection Test:");
    try {
        const claude = new ClaudeAgentConnection();
        await claude.connect();
        
        const testResult = await claude.askQuestion("Can you confirm you received my connection test?");
        console.log("✅ Claude connection result:", JSON.stringify(testResult, null, 2));
        
        await claude.disconnect();
    } catch (error) {
        console.log("❌ Claude connection error:", error.message);
    }
    
    console.log("\n🎯 Dual connection test complete!");
}

testDualConnection().catch(console.error);