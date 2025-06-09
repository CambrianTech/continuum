#!/usr/bin/env node
/**
 * Direct Screenshot Test - Using console feedback to debug screenshot
 * ===================================================================
 * 
 * Directly trigger screenshot via browser console and trace execution
 */

import { quickJsExecute } from './ClientConnection.js';

async function directScreenshotTest() {
    console.log("🔧 DIRECT SCREENSHOT TEST");
    console.log("=" * 50);
    
    // First test - check if we can execute JS at all
    console.log("📝 Testing basic JavaScript execution...");
    
    const basicTest = await quickJsExecute(`
        console.log("🔍 Direct screenshot test starting");
        console.log("📦 Version:", document.querySelector('[data-version]')?.dataset.version || "unknown");
        console.log("🌐 URL:", window.location.href);
        console.log("📡 WebSocket available:", typeof WebSocket !== 'undefined');
        console.log("🎨 html2canvas available:", typeof html2canvas !== 'undefined');
        "BASIC_TEST_COMPLETE";
    `);
    
    if (!basicTest.success) {
        console.log("❌ Basic JavaScript execution failed:", basicTest.error);
        return;
    }
    
    console.log("✅ Basic JavaScript execution working");
    console.log(`Console output: ${basicTest.output ? basicTest.output.length : 0} entries`);
    
    // Now try to manually trigger a screenshot command
    console.log("\n📸 Attempting manual screenshot trigger...");
    
    const screenshotTest = await quickJsExecute(`
        console.log("📸 Starting manual screenshot test...");
        
        // Try to trigger screenshot via WebSocket if available
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            console.log("📡 WebSocket connection found, sending screenshot command...");
            
            const screenshotCommand = {
                type: 'task',
                role: 'system',
                task: '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
            };
            
            window.ws.send(JSON.stringify(screenshotCommand));
            console.log("📤 Screenshot command sent via WebSocket");
            
            // Wait a moment and check if any response comes back
            setTimeout(() => {
                console.log("⏰ Screenshot command timeout check");
            }, 2000);
            
        } else {
            console.log("❌ No WebSocket connection available");
            if (!window.ws) {
                console.log("   window.ws is undefined");
            } else {
                console.log("   WebSocket readyState:", window.ws.readyState);
            }
        }
        
        // Also try direct html2canvas approach
        if (typeof html2canvas !== 'undefined') {
            console.log("🎨 html2canvas available, trying direct capture...");
            
            html2canvas(document.body, {
                height: window.innerHeight,
                width: window.innerWidth,
                useCORS: true
            }).then(function(canvas) {
                console.log("✅ Direct html2canvas capture successful");
                console.log("📊 Canvas size:", canvas.width + "x" + canvas.height);
                
                const dataUrl = canvas.toDataURL('image/png');
                console.log("📊 Data URL length:", dataUrl.length);
                
            }).catch(function(error) {
                console.error("❌ Direct html2canvas capture failed:", error.message);
            });
            
        } else {
            console.log("❌ html2canvas not available for direct capture");
        }
        
        "SCREENSHOT_TEST_INITIATED";
    `);
    
    if (screenshotTest.success) {
        console.log("✅ Screenshot test initiated");
        console.log(`Console output: ${screenshotTest.output ? screenshotTest.output.length : 0} entries`);
        
        // Show the console output to see what happened
        if (screenshotTest.output && screenshotTest.output.length > 0) {
            console.log("\n📋 CONSOLE OUTPUT:");
            screenshotTest.output.forEach((entry, index) => {
                console.log(`  ${index + 1}. [${entry.level}] ${entry.message}`);
            });
        }
        
        // Wait a moment then check for any additional console messages
        console.log("\n⏰ Waiting for screenshot processing...");
        await new Promise(resolve => setTimeout(resolve, 3000));
        
        const followupCheck = await quickJsExecute(`
            console.log("🔍 Checking for screenshot completion messages...");
            "FOLLOWUP_CHECK_COMPLETE";
        `);
        
        if (followupCheck.output && followupCheck.output.length > 0) {
            console.log("\n📋 FOLLOWUP CONSOLE OUTPUT:");
            followupCheck.output.forEach((entry, index) => {
                console.log(`  ${index + 1}. [${entry.level}] ${entry.message}`);
            });
        }
        
    } else {
        console.log("❌ Screenshot test failed:", screenshotTest.error);
    }
    
    console.log("\n🎯 ANALYSIS:");
    console.log("• Used direct browser console to test screenshot functionality");
    console.log("• Checked WebSocket availability and connection state");
    console.log("• Tested both command routing and direct html2canvas approaches");
    console.log("• Should reveal exactly where the screenshot flow breaks");
}

directScreenshotTest().catch(console.error);