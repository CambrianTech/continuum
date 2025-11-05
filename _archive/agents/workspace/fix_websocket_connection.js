#!/usr/bin/env node
/**
 * Fix WebSocket Connection - Diagnose and fix browser WebSocket connectivity
 */

import { BrowserClientConnection } from './ClientConnection.js';

async function fixWebSocketConnection() {
    console.log("🔧 WEBSOCKET CONNECTION FIX");
    console.log("=" * 50);
    console.log("Diagnosing and fixing WebSocket connectivity issues...\n");
    
    const browser = new BrowserClientConnection();
    
    console.log("🔍 Step 1: Test direct browser connection to WebSocket server");
    const connectResult = await browser.connect();
    console.log(`Browser ClientConnection: ${connectResult ? '✅ SUCCESS' : '❌ FAILED'}`);
    
    if (connectResult) {
        console.log("\n🔍 Step 2: Check if browser has WebSocket reference to server");
        
        const wsCheckResult = await browser.executeJs(`
            console.log("🔧 WebSocket connection diagnosis:");
            console.log("window.ws exists:", !!window.ws);
            
            if (window.ws) {
                console.log("WebSocket URL:", window.ws.url);
                console.log("WebSocket readyState:", window.ws.readyState);
                console.log("WebSocket readyState meanings:");
                console.log("  0 = CONNECTING");
                console.log("  1 = OPEN");
                console.log("  2 = CLOSING"); 
                console.log("  3 = CLOSED");
                
                const result = {
                    exists: true,
                    url: window.ws.url,
                    readyState: window.ws.readyState,
                    isOpen: window.ws.readyState === WebSocket.OPEN
                };
                
                console.log("📊 WebSocket status:", result);
                JSON.stringify(result);
            } else {
                console.log("❌ window.ws is undefined - browser not connected to Continuum WebSocket server");
                JSON.stringify({ exists: false });
            }
        `);
        
        if (wsCheckResult.success) {
            const wsStatus = JSON.parse(wsCheckResult.result);
            
            if (!wsStatus.exists) {
                console.log("\n❌ PROBLEM: Browser has no WebSocket connection to Continuum server");
                console.log("💡 SOLUTION: Browser needs to connect to Continuum WebSocket first");
                
                console.log("\n🔍 Step 3: Establish WebSocket connection from browser to Continuum");
                
                const establishWsResult = await browser.executeJs(`
                    console.log("🔧 Establishing WebSocket connection to Continuum server...");
                    
                    try {
                        // Connect to Continuum WebSocket server
                        const ws = new WebSocket('ws://localhost:9000');
                        
                        ws.onopen = function() {
                            console.log("✅ WebSocket connected to Continuum server");
                            window.ws = ws;
                        };
                        
                        ws.onmessage = function(event) {
                            console.log("📥 WebSocket message:", event.data);
                        };
                        
                        ws.onerror = function(error) {
                            console.error("❌ WebSocket error:", error);
                        };
                        
                        ws.onclose = function() {
                            console.log("🔌 WebSocket connection closed");
                        };
                        
                        // Wait for connection
                        await new Promise((resolve, reject) => {
                            ws.onopen = resolve;
                            ws.onerror = reject;
                            setTimeout(reject, 5000); // 5 second timeout
                        });
                        
                        window.ws = ws;
                        
                        return {
                            success: true,
                            url: ws.url,
                            readyState: ws.readyState
                        };
                        
                    } catch (error) {
                        console.error("❌ Failed to establish WebSocket connection:", error.message);
                        return {
                            success: false,
                            error: error.message
                        };
                    }
                `, 10000);
                
                if (establishWsResult.success) {
                    const connectionResult = JSON.parse(establishWsResult.result);
                    if (connectionResult.success) {
                        console.log("✅ WebSocket connection established!");
                        console.log(`📊 Connection: ${connectionResult.url}, State: ${connectionResult.readyState}`);
                        
                        console.log("\n🔍 Step 4: Test screenshot capability now");
                        const screenshotTest = await browser.captureScreenshot();
                        
                        console.log("\n📊 SCREENSHOT TEST AFTER FIX:");
                        console.log(`Success: ${screenshotTest.success ? '✅' : '❌'}`);
                        console.log(`Error: ${screenshotTest.error || 'None'}`);
                        console.log(`Path: ${screenshotTest.screenshotPath || 'None'}`);
                        
                        return screenshotTest.success;
                    }
                }
            } else {
                console.log(`\n📊 WebSocket Status: ${wsStatus.isOpen ? '✅ OPEN' : '❌ NOT OPEN'}`);
                console.log(`URL: ${wsStatus.url}`);
                console.log(`ReadyState: ${wsStatus.readyState}`);
                
                if (wsStatus.isOpen) {
                    console.log("\n✅ WebSocket is connected - screenshot should work");
                    
                    const screenshotTest = await browser.captureScreenshot();
                    console.log("\n📊 SCREENSHOT TEST:");
                    console.log(`Success: ${screenshotTest.success ? '✅' : '❌'}`);
                    console.log(`Error: ${screenshotTest.error || 'None'}`);
                    
                    return screenshotTest.success;
                }
            }
        }
    }
    
    await browser.disconnect();
    
    console.log("\n🎯 WEBSOCKET CONNECTION FIX COMPLETE");
    return false;
}

fixWebSocketConnection().catch(console.error);