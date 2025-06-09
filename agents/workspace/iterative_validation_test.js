#!/usr/bin/env node
/**
 * Iterative Validation Test - Claude and Browser
 * ============================================
 * 
 * Systematically test each validation milestone for both client types
 * Identify and iterate through validation problems one by one
 */

import { 
    BrowserClientConnection, 
    ClaudeAgentConnection,
    validateBrowserClient,
    validateClaudeClient 
} from './ClientConnection.js';

class IterativeValidationTester {
    constructor() {
        this.browserClient = null;
        this.claudeClient = null;
        this.testResults = {
            browser: {},
            claude: {}
        };
    }

    async runIterativeTests() {
        console.log("🔄 ITERATIVE VALIDATION TEST - Claude & Browser");
        console.log("=" * 60);
        console.log("Testing validation problems one by one...\n");

        // Test 1: Basic Connection
        await this.testBasicConnections();
        
        // Test 2: Browser JavaScript Execution
        await this.testBrowserJavaScriptExecution();
        
        // Test 3: Browser Console Capture
        await this.testBrowserConsoleCapture();
        
        // Test 4: Browser Error Systems
        await this.testBrowserErrorSystems();
        
        // Test 5: Claude Communication
        await this.testClaudeCommunication();
        
        // Test 6: Claude Feature Declaration
        await this.testClaudeFeatureDeclaration();
        
        // Test 7: Browser Screenshot Capability
        await this.testBrowserScreenshotCapability();
        
        // Test 8: Cross-validation
        await this.testCrossValidation();
        
        // Generate final report
        this.generateIterativeReport();
    }

    async testBasicConnections() {
        console.log("🔌 TEST 1: Basic Connection Validation");
        console.log("-" * 40);
        
        // Browser connection
        try {
            this.browserClient = new BrowserClientConnection();
            const browserConnected = await this.browserClient.connect();
            this.testResults.browser.connection = {
                success: browserConnected,
                details: browserConnected ? "Connected successfully" : "Connection failed"
            };
            console.log(`📱 Browser: ${browserConnected ? '✅ CONNECTED' : '❌ FAILED'}`);
        } catch (error) {
            this.testResults.browser.connection = { success: false, error: error.message };
            console.log(`📱 Browser: ❌ ERROR - ${error.message}`);
        }
        
        // Claude connection
        try {
            this.claudeClient = new ClaudeAgentConnection();
            const claudeConnected = await this.claudeClient.connect();
            this.testResults.claude.connection = {
                success: claudeConnected,
                details: claudeConnected ? "Connected successfully" : "Connection failed"
            };
            console.log(`🤖 Claude: ${claudeConnected ? '✅ CONNECTED' : '❌ FAILED'}`);
        } catch (error) {
            this.testResults.claude.connection = { success: false, error: error.message };
            console.log(`🤖 Claude: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testBrowserJavaScriptExecution() {
        console.log("💻 TEST 2: Browser JavaScript Execution");
        console.log("-" * 40);
        
        if (!this.browserClient || !this.testResults.browser.connection.success) {
            console.log("❌ Skipping - Browser not connected");
            this.testResults.browser.jsExecution = { success: false, error: "No connection" };
            console.log();
            return;
        }
        
        try {
            const testJs = 'console.log("JavaScript execution test"); "JS_EXECUTION_SUCCESS";';
            const result = await this.browserClient.executeJs(testJs);
            
            this.testResults.browser.jsExecution = {
                success: result.success,
                result: result.result,
                hasOutput: result.output && result.output.length > 0
            };
            
            console.log(`📱 JavaScript Execution: ${result.success ? '✅ SUCCESS' : '❌ FAILED'}`);
            console.log(`📱 Return Value: ${result.result}`);
            console.log(`📱 Console Output: ${result.output ? result.output.length + ' entries' : 'None'}`);
            
        } catch (error) {
            this.testResults.browser.jsExecution = { success: false, error: error.message };
            console.log(`📱 JavaScript Execution: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testBrowserConsoleCapture() {
        console.log("📋 TEST 3: Browser Console Capture");
        console.log("-" * 40);
        
        if (!this.browserClient || !this.testResults.browser.connection.success) {
            console.log("❌ Skipping - Browser not connected");
            this.testResults.browser.consoleCapture = { success: false, error: "No connection" };
            console.log();
            return;
        }
        
        try {
            const consoleTestJs = `
                console.log("Console capture test");
                console.error("Test error message");  
                console.warn("Test warning message");
                "CONSOLE_CAPTURE_TEST";
            `;
            
            const result = await this.browserClient.captureConsoleOutput(consoleTestJs);
            
            this.testResults.browser.consoleCapture = {
                success: result.success,
                totalMessages: result.console ? result.console.total : 0,
                errors: result.console ? result.console.errors.length : 0,
                warnings: result.console ? result.console.warnings.length : 0,
                logs: result.console ? result.console.logs.length : 0
            };
            
            console.log(`📱 Console Capture: ${result.success ? '✅ SUCCESS' : '❌ FAILED'}`);
            if (result.success) {
                console.log(`📱 Total Messages: ${result.console.total}`);
                console.log(`📱 Errors: ${result.console.errors.length}`);
                console.log(`📱 Warnings: ${result.console.warnings.length}`);
                console.log(`📱 Logs: ${result.console.logs.length}`);
            }
            
        } catch (error) {
            this.testResults.browser.consoleCapture = { success: false, error: error.message };
            console.log(`📱 Console Capture: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testBrowserErrorSystems() {
        console.log("🚨 TEST 4: Browser Error Systems");
        console.log("-" * 40);
        
        if (!this.browserClient || !this.testResults.browser.connection.success) {
            console.log("❌ Skipping - Browser not connected");
            this.testResults.browser.errorSystems = { success: false, error: "No connection" };
            console.log();
            return;
        }
        
        try {
            const result = await this.browserClient.validateErrorSystems();
            
            this.testResults.browser.errorSystems = result;
            
            console.log(`📱 Error Systems: ${result.success ? '✅ SUCCESS' : '❌ FAILED'}`);
            console.log(`📱 Milestone: ${result.milestone}`);
            console.log(`📱 Errors Detected: ${result.errorsDetected || 0}`);
            console.log(`📱 Warnings Detected: ${result.warningsDetected || 0}`);
            
        } catch (error) {
            this.testResults.browser.errorSystems = { success: false, error: error.message };
            console.log(`📱 Error Systems: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testClaudeCommunication() {
        console.log("💬 TEST 5: Claude Communication");
        console.log("-" * 40);
        
        if (!this.claudeClient || !this.testResults.claude.connection.success) {
            console.log("❌ Skipping - Claude not connected");
            this.testResults.claude.communication = { success: false, error: "No connection" };
            console.log();
            return;
        }
        
        try {
            const testMessage = "This is a validation test. Please respond with 'VALIDATION_SUCCESS' if you received this.";
            const result = await this.claudeClient.sendMessage(testMessage);
            
            this.testResults.claude.communication = {
                success: result.success,
                responseReceived: !!result.response,
                responseLength: result.response ? result.response.length : 0,
                validationFound: result.response ? result.response.includes('VALIDATION') : false
            };
            
            console.log(`🤖 Claude Communication: ${result.success ? '✅ SUCCESS' : '❌ FAILED'}`);
            console.log(`🤖 Response Received: ${result.response ? 'Yes' : 'No'}`);
            console.log(`🤖 Response Length: ${result.response ? result.response.length + ' chars' : '0'}`);
            
        } catch (error) {
            this.testResults.claude.communication = { success: false, error: error.message };
            console.log(`🤖 Claude Communication: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testClaudeFeatureDeclaration() {
        console.log("🎛️ TEST 6: Claude Feature Declaration");
        console.log("-" * 40);
        
        try {
            const features = ClaudeAgentConnection.getClientFeatures();
            
            this.testResults.claude.featureDeclaration = {
                success: true,
                capabilities: features.capabilities,
                restrictions: features.restrictions,
                menuItems: features.menuItems
            };
            
            console.log(`🤖 Feature Declaration: ✅ SUCCESS`);
            console.log(`🤖 Capabilities: ${features.capabilities.join(', ')}`);
            console.log(`🤖 Restrictions: ${features.restrictions.join(', ')}`);
            console.log(`🤖 Menu Items: ${features.menuItems.length} items`);
            
        } catch (error) {
            this.testResults.claude.featureDeclaration = { success: false, error: error.message };
            console.log(`🤖 Feature Declaration: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testBrowserScreenshotCapability() {
        console.log("📸 TEST 7: Browser Screenshot Capability");
        console.log("-" * 40);
        
        if (!this.browserClient || !this.testResults.browser.connection.success) {
            console.log("❌ Skipping - Browser not connected");
            this.testResults.browser.screenshot = { success: false, error: "No connection" };
            console.log();
            return;
        }
        
        try {
            const result = await this.browserClient.captureScreenshot();
            
            this.testResults.browser.screenshot = {
                success: result.success,
                screenshotPath: result.screenshotPath,
                fileSize: result.fileSize
            };
            
            console.log(`📱 Screenshot: ${result.success ? '✅ SUCCESS' : '❌ FAILED'}`);
            if (result.success) {
                console.log(`📱 Screenshot Path: ${result.screenshotPath}`);
                console.log(`📱 File Size: ${result.fileSize} bytes`);
            } else {
                console.log(`📱 Error: ${result.error}`);
            }
            
        } catch (error) {
            this.testResults.browser.screenshot = { success: false, error: error.message };
            console.log(`📱 Screenshot: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    async testCrossValidation() {
        console.log("🔄 TEST 8: Cross-Validation");
        console.log("-" * 40);
        
        // Browser full validation
        try {
            console.log("Running full browser validation...");
            const browserValidation = await validateBrowserClient();
            this.testResults.browser.fullValidation = browserValidation;
            console.log(`📱 Full Browser Validation: ${browserValidation.overallSuccess ? '✅ SUCCESS' : '❌ FAILED'}`);
            console.log(`📱 Success Rate: ${browserValidation.successRate || 'Unknown'}`);
        } catch (error) {
            this.testResults.browser.fullValidation = { success: false, error: error.message };
            console.log(`📱 Full Browser Validation: ❌ ERROR - ${error.message}`);
        }
        
        // Claude full validation
        try {
            console.log("Running full Claude validation...");
            const claudeValidation = await validateClaudeClient();
            this.testResults.claude.fullValidation = claudeValidation;
            console.log(`🤖 Full Claude Validation: ${claudeValidation.success ? '✅ SUCCESS' : '❌ FAILED'}`);
        } catch (error) {
            this.testResults.claude.fullValidation = { success: false, error: error.message };
            console.log(`🤖 Full Claude Validation: ❌ ERROR - ${error.message}`);
        }
        
        console.log();
    }

    generateIterativeReport() {
        console.log("📊 ITERATIVE VALIDATION REPORT");
        console.log("=" * 60);
        
        console.log("\n🌐 BROWSER CLIENT RESULTS:");
        console.log("-" * 30);
        for (const [testName, result] of Object.entries(this.testResults.browser)) {
            const status = result.success ? "✅ PASS" : "❌ FAIL";
            console.log(`  ${testName}: ${status}`);
            if (!result.success && result.error) {
                console.log(`    Error: ${result.error}`);
            }
        }
        
        console.log("\n🤖 CLAUDE CLIENT RESULTS:");
        console.log("-" * 30);
        for (const [testName, result] of Object.entries(this.testResults.claude)) {
            const status = result.success ? "✅ PASS" : "❌ FAIL";
            console.log(`  ${testName}: ${status}`);
            if (!result.success && result.error) {
                console.log(`    Error: ${result.error}`);
            }
        }
        
        // Calculate overall success
        const browserPassed = Object.values(this.testResults.browser).filter(r => r.success).length;
        const browserTotal = Object.keys(this.testResults.browser).length;
        const claudePassed = Object.values(this.testResults.claude).filter(r => r.success).length;
        const claudeTotal = Object.keys(this.testResults.claude).length;
        
        console.log("\n📈 SUMMARY:");
        console.log(`Browser Success Rate: ${browserPassed}/${browserTotal} (${((browserPassed/browserTotal)*100).toFixed(1)}%)`);
        console.log(`Claude Success Rate: ${claudePassed}/${claudeTotal} (${((claudePassed/claudeTotal)*100).toFixed(1)}%)`);
        console.log(`Overall Success Rate: ${browserPassed + claudePassed}/${browserTotal + claudeTotal} (${(((browserPassed + claudePassed)/(browserTotal + claudeTotal))*100).toFixed(1)}%)`);
    }

    async cleanup() {
        if (this.browserClient) {
            await this.browserClient.disconnect();
        }
        if (this.claudeClient) {
            await this.claudeClient.disconnect();
        }
    }
}

// Run the iterative validation test
async function main() {
    const tester = new IterativeValidationTester();
    
    try {
        await tester.runIterativeTests();
    } catch (error) {
        console.error("❌ Test runner error:", error);
    } finally {
        await tester.cleanup();
    }
}

main().catch(console.error);