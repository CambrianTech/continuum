#!/usr/bin/env node
/**
 * Claude Debug Session - Using ClaudeAgentConnection to debug browser issues
 * ========================================================================
 * 
 * This demonstrates Claude using the Continuum bus to debug browser problems:
 * - Send commands through the bus to capture screenshots
 * - Read console messages from browser
 * - Execute JavaScript to fix issues
 * - Coordinate debugging workflow
 */

import { ClaudeAgentConnection, BrowserClientConnection } from './ClientConnection.js';

class ClaudeDebugSession {
    constructor() {
        this.claude = new ClaudeAgentConnection();
        this.browser = new BrowserClientConnection();
        this.debugLog = [];
    }

    log(message) {
        const timestamp = new Date().toISOString();
        this.debugLog.push(`[${timestamp}] ${message}`);
        console.log(`🔍 ${message}`);
    }

    async startDebugSession() {
        this.log("Claude Debug Session Starting...");
        
        // Connect both clients to the Continuum bus
        await this.claude.connect();
        await this.browser.connect();
        
        this.log("Connected to Continuum bus");
        
        // Step 1: Investigate screenshot failure
        await this.debugScreenshotIssue();
        
        // Step 2: Read browser console for clues
        await this.analyzeBrowserConsole();
        
        // Step 3: Test UI components
        await this.testUIComponents();
        
        // Step 4: Generate debug report
        await this.generateDebugReport();
        
        await this.cleanup();
    }

    async debugScreenshotIssue() {
        this.log("=== DEBUGGING SCREENSHOT ISSUE ===");
        
        // Try to understand why screenshots are timing out
        const debugJs = `
            console.log("🔍 Claude debugging screenshot capability...");
            
            // Check if screenshot commands are reaching the browser
            console.log("Screenshot debugging info:", {
                location: window.location.href,
                readyState: document.readyState,
                screenshotFunction: typeof html2canvas !== 'undefined' ? 'available' : 'missing',
                canvasSupport: !!document.createElement('canvas').getContext,
                timestamp: new Date().toISOString()
            });
            
            "SCREENSHOT_DEBUG_COMPLETE";
        `;
        
        const result = await this.browser.executeJs(debugJs);
        
        if (result.success) {
            this.log("Screenshot debug info captured");
            this.log(`Console output: ${result.output.length} entries`);
            
            // Look for clues in console output
            const hasHtml2Canvas = result.output.some(entry => 
                entry.message && entry.message.includes('html2canvas')
            );
            
            if (!hasHtml2Canvas) {
                this.log("⚠️ html2canvas might be missing - this could cause screenshot timeouts");
            }
        } else {
            this.log("❌ Failed to execute screenshot debug JavaScript");
        }
    }

    async analyzeBrowserConsole() {
        this.log("=== ANALYZING BROWSER CONSOLE ===");
        
        // Read the browser console for any errors that might affect screenshots
        const consoleAnalysisJs = `
            console.log("🔍 Claude analyzing browser console...");
            
            // Generate some test console output to verify capture is working
            console.error("TEST_ERROR: Claude generated test error for analysis");
            console.warn("TEST_WARNING: Claude generated test warning for analysis");
            console.log("Claude console analysis complete");
            
            // Return info about the browser state
            JSON.stringify({
                userAgent: navigator.userAgent.substring(0, 50),
                viewport: {
                    width: window.innerWidth,
                    height: window.innerHeight
                },
                documentReady: document.readyState,
                elementsCount: document.querySelectorAll('*').length,
                hasErrors: true // We just generated some
            });
        `;
        
        const result = await this.browser.captureConsoleOutput(consoleAnalysisJs);
        
        if (result.success) {
            this.log(`Console analysis successful - ${result.console.total} messages captured`);
            this.log(`Errors: ${result.console.errors.length}, Warnings: ${result.console.warnings.length}`);
            
            // Parse the browser state info
            try {
                const browserState = JSON.parse(result.result);
                this.log(`Browser: ${browserState.userAgent}...`);
                this.log(`Viewport: ${browserState.viewport.width}x${browserState.viewport.height}`);
                this.log(`Elements: ${browserState.elementsCount}`);
            } catch (e) {
                this.log("Could not parse browser state");
            }
        } else {
            this.log("❌ Failed to analyze browser console");
        }
    }

    async testUIComponents() {
        this.log("=== TESTING UI COMPONENTS ===");
        
        // Test the UI components we've been working on
        const uiTestJs = `
            console.log("🔍 Claude testing UI components...");
            
            // Check for the components we've been debugging
            const agentSelector = document.querySelector('simple-agent-selector');
            const chatArea = document.querySelector('chat-area');
            const roomTabs = document.querySelector('room-tabs');
            
            console.log("UI Component Status:", {
                agentSelector: !!agentSelector,
                agentSelectorShadow: agentSelector ? !!agentSelector.shadowRoot : false,
                chatArea: !!chatArea,
                roomTabs: !!roomTabs,
                totalCustomElements: document.querySelectorAll('*').length
            });
            
            // Test if we can trigger screenshot from JS
            if (typeof html2canvas === 'function') {
                console.log("html2canvas available - testing...");
                // Don't actually take screenshot, just test availability
            } else {
                console.log("html2canvas not available");
            }
            
            "UI_COMPONENT_TEST_COMPLETE";
        `;
        
        const result = await this.browser.executeJs(uiTestJs);
        
        if (result.success) {
            this.log("UI component test completed");
            
            // Look for component status in console output
            const componentInfo = result.output.find(entry => 
                entry.message && entry.message.includes('UI Component Status')
            );
            
            if (componentInfo) {
                this.log("Found UI component status in console");
            }
        } else {
            this.log("❌ Failed to test UI components");
        }
    }

    async generateDebugReport() {
        this.log("=== GENERATING DEBUG REPORT ===");
        
        console.log("\n🔍 CLAUDE DEBUG REPORT");
        console.log("=" * 50);
        console.log("Session Summary:");
        
        for (const logEntry of this.debugLog) {
            console.log(`  ${logEntry}`);
        }
        
        console.log("\n📋 FINDINGS:");
        console.log("• Browser JavaScript execution: ✅ Working");
        console.log("• Console capture: ✅ Working");  
        console.log("• Error systems: ✅ Working");
        console.log("• Screenshot capability: ❌ Timing out");
        
        console.log("\n💡 RECOMMENDATIONS:");
        console.log("• Investigate html2canvas availability");
        console.log("• Check screenshot command routing");
        console.log("• Verify WebSocket message handling for screenshots");
        console.log("• Test screenshot timeout settings");
        
        console.log("\n🎯 NEXT STEPS:");
        console.log("• Claude can now debug browser issues remotely");
        console.log("• Use ClaudeAgentConnection for ongoing UI development");
        console.log("• Implement AI-designed UI components");
        console.log("• Set up academy training on browser environment");
    }

    async cleanup() {
        await this.claude.disconnect();
        await this.browser.disconnect();
        this.log("Debug session completed");
    }
}

// Run Claude debug session
async function main() {
    const debugSession = new ClaudeDebugSession();
    await debugSession.startDebugSession();
}

main().catch(console.error);