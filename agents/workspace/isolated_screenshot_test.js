#!/usr/bin/env node
/**
 * Isolated Screenshot Test - Focus only on screenshot milestone
 */

import { BrowserClientConnection } from './ClientConnection.js';

async function isolatedScreenshotTest() {
    console.log("📸 ISOLATED SCREENSHOT MILESTONE TEST");
    console.log("=" * 50);
    console.log("Testing ONLY screenshot capability to isolate the issue...\n");
    
    const browser = new BrowserClientConnection();
    await browser.connect();
    
    console.log("🔍 Step 1: Check screenshot prerequisites in browser");
    const prereqResult = await browser.executeJs(`
        console.log("📸 MILESTONE TEST: Checking screenshot prerequisites");
        console.log("html2canvas available:", typeof html2canvas !== 'undefined');
        console.log("WebSocket available:", typeof WebSocket !== 'undefined');
        console.log("WebSocket connection:", window.ws ? window.ws.readyState : 'no ws');
        console.log("Document ready:", document.readyState);
        console.log("Canvas support:", !!document.createElement('canvas').getContext);
        
        const result = {
            html2canvas: typeof html2canvas !== 'undefined',
            websocket: typeof WebSocket !== 'undefined',
            wsConnection: window.ws ? window.ws.readyState === WebSocket.OPEN : false,
            documentReady: document.readyState === 'complete',
            canvasSupport: !!document.createElement('canvas').getContext
        };
        
        console.log("📊 Prerequisites result:", result);
        JSON.stringify(result);
    `);
    
    if (prereqResult.success) {
        const prereqs = JSON.parse(prereqResult.result);
        console.log("📊 Screenshot Prerequisites:");
        console.log(`  html2canvas: ${prereqs.html2canvas ? '✅' : '❌'}`);
        console.log(`  WebSocket: ${prereqs.websocket ? '✅' : '❌'}`);
        console.log(`  WS Connection: ${prereqs.wsConnection ? '✅' : '❌'}`);
        console.log(`  Document Ready: ${prereqs.documentReady ? '✅' : '❌'}`);
        console.log(`  Canvas Support: ${prereqs.canvasSupport ? '✅' : '❌'}`);
        
        const allPrereqsMet = Object.values(prereqs).every(v => v);
        console.log(`\n📋 All Prerequisites: ${allPrereqsMet ? '✅ MET' : '❌ MISSING'}`);
        
        if (allPrereqsMet) {
            console.log("\n🔍 Step 2: Test direct screenshot capture");
            
            // Test direct screenshot without going through command system
            const directScreenshotResult = await browser.executeJs(`
                console.log("📸 MILESTONE TEST: Attempting direct screenshot");
                
                (async function directScreenshotTest() {
                    try {
                        console.log("📸 Starting html2canvas capture...");
                        
                        const canvas = await html2canvas(document.body, {
                            useCORS: true,
                            allowTaint: true,
                            height: 400,
                            width: 600
                        });
                        
                        console.log("📸 Canvas created:", canvas.width + "x" + canvas.height);
                        
                        const dataUrl = canvas.toDataURL('image/png');
                        console.log("📸 DataURL created, length:", dataUrl.length);
                        
                        return {
                            success: true,
                            width: canvas.width,
                            height: canvas.height,
                            dataLength: dataUrl.length
                        };
                        
                    } catch (error) {
                        console.error("📸 Direct screenshot failed:", error.message);
                        return {
                            success: false,
                            error: error.message
                        };
                    }
                })();
            `, 15000); // Longer timeout for screenshot
            
            if (directScreenshotResult.success) {
                console.log("✅ Direct screenshot capability working!");
                console.log(`📊 Result: ${directScreenshotResult.result}`);
                
                console.log("\n🔍 Step 3: Test command-based screenshot");
                const commandScreenshotResult = await browser.captureScreenshot();
                
                console.log("\n📊 COMMAND SCREENSHOT RESULT:");
                console.log(`Success: ${commandScreenshotResult.success}`);
                console.log(`Error: ${commandScreenshotResult.error}`);
                console.log(`Path: ${commandScreenshotResult.screenshotPath}`);
                
                if (!commandScreenshotResult.success) {
                    console.log("\n❌ ISSUE IDENTIFIED: Direct screenshot works but command-based fails");
                    console.log("This indicates a problem in the command routing or WebSocket handling");
                    console.log("Check server console for debug messages about command processing");
                }
            } else {
                console.log("❌ Direct screenshot failed");
                console.log(`Error: ${directScreenshotResult.error}`);
            }
        }
    }
    
    await browser.disconnect();
    
    console.log("\n🎯 ISOLATED SCREENSHOT TEST COMPLETE");
    console.log("This focused test isolates whether the issue is:");
    console.log("1. Prerequisites missing (html2canvas, WebSocket, etc.)");
    console.log("2. Direct screenshot capability broken");
    console.log("3. Command routing/processing broken");
}

isolatedScreenshotTest().catch(console.error);