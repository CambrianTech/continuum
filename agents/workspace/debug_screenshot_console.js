#!/usr/bin/env node
/**
 * Debug Screenshot Console - Check what happens in browser console during screenshot
 * ===============================================================================
 */

import { BrowserClientConnection } from './ClientConnection.js';

async function debugScreenshotConsole() {
    console.log("🔍 DEBUGGING SCREENSHOT CONSOLE MESSAGES");
    console.log("=" * 50);
    
    const browser = new BrowserClientConnection();
    await browser.connect();
    
    // First, clear console and set up monitoring
    const setupJs = `
        console.clear();
        console.log("🔧 Screenshot debug monitoring started");
        console.log("📊 Initial state:", {
            readyState: document.readyState,
            html2canvas: typeof html2canvas !== 'undefined',
            websocketState: 'connected'
        });
        "MONITORING_READY";
    `;
    
    await browser.executeJs(setupJs);
    
    console.log("\n📸 Sending screenshot command and monitoring console...");
    
    // Send screenshot command manually and monitor what happens
    const screenshotDebugJs = `
        console.log("📸 About to receive screenshot command...");
        
        // Monitor for any screenshot-related activity
        let screenshotActivity = [];
        
        // Override console methods to track screenshot activity
        const originalLog = console.log;
        const originalError = console.error;
        
        console.log = function(...args) {
            const message = args.join(' ');
            if (message.includes('screenshot') || message.includes('SCREENSHOT') || message.includes('html2canvas')) {
                screenshotActivity.push({
                    type: 'log',
                    message: message,
                    timestamp: new Date().toISOString()
                });
            }
            originalLog.apply(console, args);
        };
        
        console.error = function(...args) {
            const message = args.join(' ');
            if (message.includes('screenshot') || message.includes('SCREENSHOT') || message.includes('html2canvas')) {
                screenshotActivity.push({
                    type: 'error', 
                    message: message,
                    timestamp: new Date().toISOString()
                });
            }
            originalError.apply(console, args);
        };
        
        // Wait a moment then return the activity
        setTimeout(() => {
            console.log("📊 Screenshot activity captured:", screenshotActivity);
        }, 1000);
        
        "SCREENSHOT_MONITORING_ACTIVE";
    `;
    
    await browser.executeJs(screenshotDebugJs);
    
    // Now attempt the screenshot and immediately capture console
    console.log("Attempting screenshot capture...");
    
    // Use the WebSocket directly to send screenshot command
    try {
        const taskMessage = {
            type: 'task',
            role: 'system', 
            task: '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
        };
        
        browser.websocket.send(JSON.stringify(taskMessage));
        console.log("✅ Screenshot command sent via WebSocket");
        
        // Wait a moment then check console
        await new Promise(resolve => setTimeout(resolve, 2000));
        
        // Capture console after screenshot command
        const consoleCheckJs = `
            console.log("🔍 Checking console after screenshot command...");
            
            // Check for any error messages or activity
            const consoleEntries = [];
            
            // Check if there are any screenshot-related errors
            if (window.screenshotActivity) {
                console.log("📊 Screenshot activity found:", window.screenshotActivity);
            }
            
            // Look for specific error patterns
            const errorPatterns = [
                'html2canvas',
                'screenshot',
                'timeout',
                'not found',
                'undefined'
            ];
            
            console.log("🔍 Looking for screenshot command acknowledgment...");
            console.log("📡 WebSocket readyState:", window.location.protocol.includes('ws') ? 'WebSocket protocol' : 'HTTP protocol');
            
            "CONSOLE_CHECK_COMPLETE";
        `;
        
        const consoleResult = await browser.captureConsoleOutput(consoleCheckJs);
        
        if (consoleResult.success) {
            console.log("\n📋 CONSOLE MESSAGES CAPTURED:");
            console.log(`Total messages: ${consoleResult.console.total}`);
            console.log(`Errors: ${consoleResult.console.errors.length}`);
            console.log(`Warnings: ${consoleResult.console.warnings.length}`);
            console.log(`Logs: ${consoleResult.console.logs.length}`);
            
            // Look for screenshot-related messages
            const screenshotMessages = consoleResult.console.raw.filter(entry => 
                entry.message && (
                    entry.message.includes('screenshot') ||
                    entry.message.includes('SCREENSHOT') ||
                    entry.message.includes('html2canvas') ||
                    entry.message.includes('CMD:')
                )
            );
            
            if (screenshotMessages.length > 0) {
                console.log("\n📸 SCREENSHOT-RELATED CONSOLE MESSAGES:");
                screenshotMessages.forEach((msg, index) => {
                    console.log(`  ${index + 1}. [${msg.level}] ${msg.message}`);
                });
            } else {
                console.log("\n❌ NO SCREENSHOT-RELATED MESSAGES FOUND IN CONSOLE");
                console.log("This suggests the screenshot command is not reaching the browser");
            }
            
            // Show all error messages to see what's happening
            if (consoleResult.console.errors.length > 0) {
                console.log("\n🚨 CONSOLE ERRORS:");
                consoleResult.console.errors.forEach((error, index) => {
                    console.log(`  ${index + 1}. ${error.message}`);
                });
            }
        }
        
    } catch (error) {
        console.log(`❌ Error sending screenshot command: ${error.message}`);
    }
    
    await browser.disconnect();
    
    console.log("\n🎯 CONCLUSION:");
    console.log("Based on console monitoring, we can determine if:");
    console.log("• Screenshot commands are reaching the browser");
    console.log("• html2canvas is being invoked");
    console.log("• There are specific error messages");
    console.log("• The command acknowledgment is working");
}

debugScreenshotConsole().catch(console.error);