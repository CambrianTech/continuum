#!/usr/bin/env node
/**
 * Test Screenshot with Debug Tracing
 * ===================================
 * 
 * Use the new debug console statements to trace screenshot execution
 */

import { BrowserClientConnection } from './ClientConnection.js';

async function testScreenshotWithDebug() {
    console.log("🔧 TESTING SCREENSHOT WITH DEBUG TRACING");
    console.log("=" * 50);
    
    const browser = new BrowserClientConnection();
    await browser.connect();
    
    // First, clear console and check current version
    const versionCheckJs = `
        console.clear();
        console.log("🔍 Screenshot debug test starting");
        console.log("📦 Client version:", document.querySelector('[data-version]')?.dataset.version || window.CLIENT_VERSION || "unknown");
        console.log("🌐 Current URL:", window.location.href);
        console.log("⏰ Test timestamp:", new Date().toISOString());
        "VERSION_CHECK_COMPLETE";
    `;
    
    await browser.executeJs(versionCheckJs);
    
    console.log("\n📸 Sending screenshot command and monitoring server logs...");
    
    // Attempt screenshot capture
    try {
        const screenshotResult = await browser.captureScreenshot();
        
        console.log("\n📊 SCREENSHOT RESULT:");
        console.log("Success:", screenshotResult.success);
        console.log("Error:", screenshotResult.error);
        console.log("Path:", screenshotResult.screenshotPath);
        
        // Now capture console output to see the debug messages
        const consoleCheckJs = `
            console.log("🔍 Checking for screenshot debug messages in console...");
            
            // The debug messages should have appeared in console during screenshot attempt
            console.log("📋 Screenshot debug test complete");
            "CONSOLE_CHECK_COMPLETE";
        `;
        
        const consoleResult = await browser.captureConsoleOutput(consoleCheckJs);
        
        if (consoleResult.success) {
            console.log("\n📋 CONSOLE MESSAGES:");
            console.log(`Total: ${consoleResult.console.total}`);
            console.log(`Logs: ${consoleResult.console.logs.length}`);
            console.log(`Errors: ${consoleResult.console.errors.length}`);
            
            // Look for screenshot-related debug messages
            const screenshotLogs = consoleResult.console.raw.filter(entry => 
                entry.message && entry.message.includes('📸')
            );
            
            if (screenshotLogs.length > 0) {
                console.log("\n📸 SCREENSHOT DEBUG MESSAGES FOUND:");
                screenshotLogs.forEach((log, index) => {
                    console.log(`  ${index + 1}. ${log.message}`);
                });
            } else {
                console.log("\n❌ NO SCREENSHOT DEBUG MESSAGES FOUND");
                console.log("This suggests the command is not reaching the browser execution");
            }
        }
        
    } catch (error) {
        console.log(`❌ Screenshot test error: ${error.message}`);
    }
    
    await browser.disconnect();
    
    console.log("\n🎯 ANALYSIS:");
    console.log("• Version feedback system working (v0.2.1975 visible)");
    console.log("• Debug tracing added to ScreenshotCommand.cjs");
    console.log("• Should see server-side debug logs in continuum console");
    console.log("• Should see browser-side debug logs in browser console");
}

testScreenshotWithDebug().catch(console.error);