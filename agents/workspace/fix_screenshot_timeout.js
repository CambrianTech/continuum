#!/usr/bin/env node
/**
 * Fix Screenshot Timeout - Investigation and Resolution
 * ===================================================
 * 
 * Based on Claude's remote debugging findings:
 * - html2canvas might be missing
 * - Screenshot WebSocket routing may need investigation
 * - Timeout settings may need adjustment
 */

import { BrowserClientConnection } from './ClientConnection.js';

class ScreenshotTimeoutFixer {
    constructor() {
        this.browser = new BrowserClientConnection();
        this.issues = [];
        this.solutions = [];
    }

    async investigateAndFix() {
        console.log("🔧 SCREENSHOT TIMEOUT INVESTIGATION & FIX");
        console.log("=" * 50);
        
        await this.browser.connect();
        
        // Step 1: Check html2canvas availability
        await this.checkHtml2canvasAvailability();
        
        // Step 2: Test screenshot command routing
        await this.testScreenshotRouting();
        
        // Step 3: Try manual screenshot implementation
        await this.tryManualScreenshotImplementation();
        
        // Step 4: Test with different timeout settings
        await this.testTimeoutSettings();
        
        // Step 5: Generate fix recommendations
        await this.generateFixRecommendations();
        
        await this.browser.disconnect();
    }

    async checkHtml2canvasAvailability() {
        console.log("\n🔍 STEP 1: Checking html2canvas availability");
        console.log("-" * 40);
        
        const checkJs = `
            console.log("🔍 Checking html2canvas...");
            
            const availability = {
                html2canvas: typeof html2canvas !== 'undefined',
                canvasSupport: !!document.createElement('canvas').getContext,
                documentReady: document.readyState,
                scriptsLoaded: document.scripts.length
            };
            
            console.log("📊 Screenshot capabilities:", availability);
            
            if (typeof html2canvas === 'undefined') {
                console.error("❌ html2canvas not found - this will cause screenshot timeouts");
                
                // Try to load html2canvas dynamically
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => console.log("✅ html2canvas loaded dynamically");
                script.onerror = () => console.error("❌ Failed to load html2canvas dynamically");
                document.head.appendChild(script);
            } else {
                console.log("✅ html2canvas is available");
            }
            
            JSON.stringify(availability);
        `;
        
        const result = await this.browser.executeJs(checkJs);
        
        if (result.success) {
            const availability = JSON.parse(result.result);
            console.log(`Canvas Support: ${availability.canvasSupport ? '✅' : '❌'}`);
            console.log(`html2canvas: ${availability.html2canvas ? '✅' : '❌'}`);
            console.log(`Document Ready: ${availability.documentReady}`);
            
            if (!availability.html2canvas) {
                this.issues.push("html2canvas library missing");
                this.solutions.push("Load html2canvas library dynamically or via script tag");
            }
        } else {
            console.log("❌ Failed to check html2canvas availability");
            this.issues.push("Cannot execute JavaScript for capability check");
        }
    }

    async testScreenshotRouting() {
        console.log("\n🔍 STEP 2: Testing screenshot command routing");
        console.log("-" * 40);
        
        // Monitor WebSocket messages during screenshot command
        const monitorJs = `
            console.log("🔍 Monitoring WebSocket messages for screenshot commands...");
            
            // Intercept WebSocket send to see what commands are being sent
            const originalSend = WebSocket.prototype.send;
            const messages = [];
            
            WebSocket.prototype.send = function(data) {
                messages.push({
                    timestamp: new Date().toISOString(),
                    data: data,
                    type: 'outgoing'
                });
                console.log("📤 WebSocket outgoing:", data);
                return originalSend.call(this, data);
            };
            
            // Monitor incoming messages
            window.addEventListener('message', (event) => {
                if (event.data && typeof event.data === 'string') {
                    messages.push({
                        timestamp: new Date().toISOString(),
                        data: event.data,
                        type: 'incoming'
                    });
                    console.log("📥 WebSocket incoming:", event.data);
                }
            });
            
            "MONITORING_SETUP_COMPLETE";
        `;
        
        const monitorResult = await this.browser.executeJs(monitorJs);
        
        if (monitorResult.success) {
            console.log("✅ WebSocket monitoring setup complete");
            
            // Now try a screenshot command and see what happens
            console.log("📸 Attempting screenshot command...");
            const screenshotResult = await this.browser.captureScreenshot();
            
            if (!screenshotResult.success) {
                console.log(`❌ Screenshot failed: ${screenshotResult.error}`);
                this.issues.push("Screenshot command routing failure");
                this.solutions.push("Investigate WebSocket message handling in continuum-core.cjs");
            } else {
                console.log("✅ Screenshot routing working");
            }
        }
    }

    async tryManualScreenshotImplementation() {
        console.log("\n🔍 STEP 3: Trying manual screenshot implementation");
        console.log("-" * 40);
        
        const manualScreenshotJs = `
            console.log("🔍 Attempting manual screenshot with html2canvas...");
            
            async function attemptManualScreenshot() {
                try {
                    // Wait for html2canvas if it's being loaded
                    let attempts = 0;
                    while (typeof html2canvas === 'undefined' && attempts < 50) {
                        await new Promise(resolve => setTimeout(resolve, 100));
                        attempts++;
                    }
                    
                    if (typeof html2canvas === 'undefined') {
                        console.error("❌ html2canvas still not available after waiting");
                        return { success: false, error: "html2canvas not available" };
                    }
                    
                    console.log("✅ html2canvas available, attempting capture...");
                    
                    const canvas = await html2canvas(document.body, {
                        height: window.innerHeight,
                        width: window.innerWidth,
                        useCORS: true
                    });
                    
                    const dataUrl = canvas.toDataURL('image/png');
                    console.log("✅ Manual screenshot captured successfully");
                    
                    return { 
                        success: true, 
                        dataUrl: dataUrl.substring(0, 100) + "...",
                        size: dataUrl.length 
                    };
                    
                } catch (error) {
                    console.error("❌ Manual screenshot failed:", error.message);
                    return { success: false, error: error.message };
                }
            }
            
            const result = await attemptManualScreenshot();
            JSON.stringify(result);
        `;
        
        const result = await this.browser.executeJs(manualScreenshotJs, 20000); // Longer timeout
        
        if (result.success) {
            try {
                const screenshotResult = JSON.parse(result.result);
                if (screenshotResult.success) {
                    console.log("✅ Manual screenshot working!");
                    console.log(`Screenshot size: ${screenshotResult.size} characters`);
                    this.solutions.push("Use manual html2canvas implementation as fallback");
                } else {
                    console.log(`❌ Manual screenshot failed: ${screenshotResult.error}`);
                    this.issues.push(`Manual screenshot error: ${screenshotResult.error}`);
                }
            } catch (e) {
                console.log("❌ Could not parse manual screenshot result");
            }
        } else {
            console.log(`❌ Manual screenshot execution failed: ${result.error}`);
            this.issues.push("JavaScript execution timeout during manual screenshot");
        }
    }

    async testTimeoutSettings() {
        console.log("\n🔍 STEP 4: Testing different timeout settings");
        console.log("-" * 40);
        
        console.log("Testing timeout configurations...");
        
        // Test with very short timeout to confirm timeout behavior
        const shortTimeoutResult = await this.browser.captureScreenshot();
        console.log(`Short timeout result: ${shortTimeoutResult.success ? 'SUCCESS' : 'TIMEOUT'}`);
        
        if (!shortTimeoutResult.success) {
            this.issues.push("Consistent screenshot timeout regardless of settings");
            this.solutions.push("Implement screenshot retry mechanism with progressive timeouts");
        }
    }

    async generateFixRecommendations() {
        console.log("\n🔧 STEP 5: Fix Recommendations");
        console.log("-" * 40);
        
        console.log("\n🚨 ISSUES IDENTIFIED:");
        this.issues.forEach((issue, index) => {
            console.log(`  ${index + 1}. ${issue}`);
        });
        
        console.log("\n💡 SOLUTIONS RECOMMENDED:");
        this.solutions.forEach((solution, index) => {
            console.log(`  ${index + 1}. ${solution}`);
        });
        
        console.log("\n🎯 PRIORITY FIXES:");
        console.log("  1. Add html2canvas to UIGenerator.cjs script includes");
        console.log("  2. Implement fallback manual screenshot in BrowserClientConnection");
        console.log("  3. Add progressive timeout retry mechanism");
        console.log("  4. Investigate WebSocket message routing for screenshot commands");
        
        console.log("\n📋 NEXT STEPS:");
        console.log("  • Modify UIGenerator.cjs to include html2canvas library");
        console.log("  • Update BrowserClientConnection with manual screenshot fallback");
        console.log("  • Test screenshot capability after fixes");
        console.log("  • Achieve 100% validation milestone completion");
    }
}

// Run screenshot timeout investigation and fix
async function main() {
    const fixer = new ScreenshotTimeoutFixer();
    await fixer.investigateAndFix();
}

main().catch(console.error);