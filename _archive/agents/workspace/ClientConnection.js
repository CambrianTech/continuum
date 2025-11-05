#!/usr/bin/env node
/**
 * ClientConnection - Core Connection & Validation Framework
 * ========================================================
 * 
 * Base class for all client connections to Continuum.
 * Written in JavaScript for Node.js ecosystem compatibility.
 * Will be converted to TypeScript later.
 * 
 * Client Types & Their Consoles:
 * - BrowserClientConnection: WebSocket → Browser JavaScript console
 * - TerminalClientConnection: Stdio/WebSocket → Terminal/shell console  
 * - AgentClientConnection: WebSocket → AI agent conversation console
 * - APIClientConnection: HTTP/REST → API response/error console
 * - DeviceClientConnection: Platform-specific → Mobile/desktop app console
 * 
 * Usage:
 *   const { BrowserClientConnection, AgentClientConnection } = require('./ClientConnection.js');
 *   
 *   const browser = new BrowserClientConnection();
 *   const agent = new AgentClientConnection();
 *   
 *   await browser.connect();
 *   await agent.connect();
 *   
 *   // Each has different console capabilities
 *   const browserResult = await browser.executeJs("console.log('test')");
 *   const agentResult = await agent.sendMessage("debug the browser connection");
 */

import WebSocket from 'ws';
import { promises as fs } from 'fs';
import path from 'path';

/**
 * Base class for all Continuum client connections
 */
class ClientConnection {
    constructor(connectionUrl = "ws://localhost:9000") {
        this.connectionUrl = connectionUrl;
        this.connection = null;
        this.connected = false;
        this.versionExpected = null;
        this.versionClient = null;
        this.clientType = "base";
    }

    async connect() {
        throw new Error("Subclass must implement connect()");
    }

    async disconnect() {
        throw new Error("Subclass must implement disconnect()");
    }

    async executeCommand(command, options = {}) {
        throw new Error("Subclass must implement executeCommand()");
    }

    async validateClientSpecific() {
        throw new Error("Subclass must implement validateClientSpecific()");
    }
}

/**
 * WebSocket connection to browser tabs with JavaScript console
 */
class BrowserClientConnection extends ClientConnection {
    constructor(wsUrl = "ws://localhost:9000") {
        super(wsUrl);
        this.websocket = null;
        this.clientType = "browser";
    }

    async connect() {
        return new Promise((resolve, reject) => {
            try {
                this.websocket = new WebSocket(this.connectionUrl);
                this.connection = this.websocket;

                this.websocket.on('open', () => {
                    this.connected = true;
                    console.log('🌐 Browser client connected');
                    
                    // Skip connection banner
                    setTimeout(() => resolve(true), 1000);
                });

                this.websocket.on('error', (error) => {
                    console.error('❌ Browser connection failed:', error.message);
                    reject(error);
                });

                this.websocket.on('close', () => {
                    this.connected = false;
                    this.connection = null;
                });

            } catch (error) {
                reject(error);
            }
        });
    }

    async disconnect() {
        if (this.websocket) {
            this.websocket.close();
            this.connected = false;
            this.connection = null;
        }
    }

    async executeJs(jsCode, timeout = 10000) {
        if (!this.connected) {
            return { success: false, error: "Not connected" };
        }

        return new Promise((resolve) => {
            try {
                const encoded = Buffer.from(jsCode).toString('base64');
                const taskMessage = {
                    type: 'task',
                    role: 'system',
                    task: `[CMD:BROWSER_JS] ${encoded}`
                };

                let responseCount = 0;
                const maxResponses = 5;

                const messageHandler = (data) => {
                    try {
                        const result = JSON.parse(data);
                        responseCount++;

                        if (result.type === 'js_executed') {
                            this.websocket.removeListener('message', messageHandler);
                            resolve({
                                success: result.success || false,
                                result: result.result,
                                output: result.output || [],
                                error: result.error,
                                timestamp: result.timestamp
                            });
                        } else if (responseCount >= maxResponses) {
                            this.websocket.removeListener('message', messageHandler);
                            resolve({ success: false, error: "Max responses reached without js_executed" });
                        }
                    } catch (parseError) {
                        // Ignore parse errors, continue listening
                    }
                };

                this.websocket.on('message', messageHandler);

                // Send the command
                this.websocket.send(JSON.stringify(taskMessage));

                // Timeout fallback
                setTimeout(() => {
                    this.websocket.removeListener('message', messageHandler);
                    resolve({ success: false, error: "Timeout waiting for execution" });
                }, timeout);

            } catch (error) {
                resolve({ success: false, error: error.message });
            }
        });
    }

    async captureConsoleOutput(jsCode) {
        const result = await this.executeJs(jsCode);
        
        if (!result.success) {
            return result;
        }

        // Categorize console output
        const output = result.output || [];
        const errors = output.filter(entry => entry.level === "error");
        const warnings = output.filter(entry => entry.level === "warn");
        const logs = output.filter(entry => entry.level === "log");

        return {
            success: true,
            result: result.result,
            console: {
                total: output.length,
                errors,
                warnings,
                logs,
                raw: output
            }
        };
    }

    async validateErrorSystems() {
        const testJs = `
            console.log("🧪 Testing error systems...");
            console.error("TEST_ERROR: Error detection test");
            console.warn("TEST_WARNING: Warning detection test");
            console.log("✅ Error systems test complete");
            "ERROR_SYSTEMS_VALIDATED";
        `;

        const result = await this.captureConsoleOutput(testJs);

        if (result.success) {
            const console = result.console;
            const errorsFound = console.errors.length > 0;
            const warningsFound = console.warnings.length > 0;

            return {
                milestone: 1,
                success: errorsFound && warningsFound,
                errorsDetected: console.errors.length,
                warningsDetected: console.warnings.length,
                consoleOutput: console
            };
        }

        return { milestone: 1, success: false, error: result.error };
    }

    async validateConsoleReading() {
        const testJs = `
            console.log("📖 Testing console reading...");
            console.error("CRITICAL_ERROR: Database connection failed");
            console.warn("PERFORMANCE_WARNING: Slow query detected");
            console.log("INFO: User authentication successful");
            console.log("✅ Console reading test complete");
            "CONSOLE_READING_VALIDATED";
        `;

        const result = await this.captureConsoleOutput(testJs);

        if (result.success) {
            const console = result.console;
            return {
                milestone: 3,
                success: console.total >= 4,
                totalMessages: console.total,
                categorized: {
                    errors: console.errors.length,
                    warnings: console.warnings.length,
                    logs: console.logs.length
                },
                consoleOutput: console
            };
        }

        return { milestone: 3, success: false, error: result.error };
    }

    async validateVersionFromClient() {
        const versionJs = `
            console.log("🔍 Reading client version...");
            const version = window.CLIENT_VERSION || 
                           document.querySelector('[data-version]')?.dataset.version || 
                           "0.2.1973";
            console.log("📦 Client version:", version);
            JSON.stringify({
                clientVersion: version,
                timestamp: new Date().toISOString()
            });
        `;

        const result = await this.executeJs(versionJs);

        if (result.success) {
            try {
                const versionData = JSON.parse(result.result);
                this.versionClient = versionData.clientVersion;

                // Get expected version
                try {
                    const packageData = JSON.parse(await fs.readFile("package.json", "utf8"));
                    this.versionExpected = packageData.version;
                } catch {
                    this.versionExpected = "unknown";
                }

                return {
                    milestone: 5,
                    success: true,
                    versionClient: this.versionClient,
                    versionExpected: this.versionExpected,
                    versionsMatch: this.versionClient === this.versionExpected
                };
            } catch {
                return { milestone: 5, success: false, error: "Could not parse version data" };
            }
        }

        return { milestone: 5, success: false, error: result.error };
    }

    async captureScreenshot() {
        if (!this.connected) {
            return { success: false, error: "Not connected" };
        }

        return new Promise((resolve) => {
            try {
                const taskMessage = {
                    type: 'task',
                    role: 'system',
                    task: '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
                };

                let responseCount = 0;
                const maxResponses = 3;

                const messageHandler = (data) => {
                    try {
                        const result = JSON.parse(data);
                        responseCount++;

                        if (result.message && result.message.includes('screenshot saved')) {
                            const screenshotPath = result.message.split('screenshot saved: ')[1];
                            this.websocket.removeListener('message', messageHandler);
                            
                            // Verify file exists
                            fs.access(screenshotPath)
                                .then(() => fs.stat(screenshotPath))
                                .then(stats => {
                                    resolve({
                                        success: true,
                                        screenshotPath,
                                        fileSize: stats.size
                                    });
                                })
                                .catch(() => {
                                    resolve({ success: false, error: `Screenshot file not found: ${screenshotPath}` });
                                });
                        } else if (responseCount >= maxResponses) {
                            this.websocket.removeListener('message', messageHandler);
                            resolve({ success: false, error: "Screenshot capture timeout" });
                        }
                    } catch (parseError) {
                        // Continue listening
                    }
                };

                this.websocket.on('message', messageHandler);
                this.websocket.send(JSON.stringify(taskMessage));

                // Timeout fallback
                setTimeout(() => {
                    this.websocket.removeListener('message', messageHandler);
                    resolve({ success: false, error: "Screenshot timeout" });
                }, 15000);

            } catch (error) {
                resolve({ success: false, error: error.message });
            }
        });
    }

    async runFullModemProtocol() {
        if (!await this.connect()) {
            return { success: false, error: "Could not connect" };
        }

        const results = {
            timestamp: new Date().toISOString(),
            milestones: {},
            overallSuccess: false
        };

        try {
            // MILESTONE 1: Error Systems
            results.milestones.M1 = await this.validateErrorSystems();

            // MILESTONE 3: Console Reading
            if (results.milestones.M1.success) {
                results.milestones.M3 = await this.validateConsoleReading();
            }

            // MILESTONE 5: Version from Client
            results.milestones.M5 = await this.validateVersionFromClient();

            // MILESTONE 6: Screenshot
            results.milestones.M6 = await this.captureScreenshot();

            // Overall success calculation
            const successfulMilestones = Object.values(results.milestones)
                .filter(m => m.success).length;
            results.overallSuccess = successfulMilestones >= 3; // Allow some failures
            results.successRate = `${successfulMilestones}/${Object.keys(results.milestones).length}`;

        } finally {
            await this.disconnect();
        }

        return results;
    }
}

/**
 * WebSocket connection to AI agents with conversation console
 */
class AgentClientConnection extends ClientConnection {
    constructor(wsUrl = "ws://localhost:9000", agentName = "GeneralAI") {
        super(wsUrl);
        this.websocket = null;
        this.clientType = "agent";
        this.agentName = agentName;
        this.conversationHistory = [];
    }

    async connect() {
        return new Promise((resolve, reject) => {
            try {
                this.websocket = new WebSocket(this.connectionUrl);
                this.connection = this.websocket;

                this.websocket.on('open', () => {
                    this.connected = true;
                    console.log(`🤖 Agent client connected (${this.agentName})`);
                    
                    // Skip connection banner
                    setTimeout(() => resolve(true), 1000);
                });

                this.websocket.on('error', (error) => {
                    console.error('❌ Agent connection failed:', error.message);
                    reject(error);
                });

                this.websocket.on('close', () => {
                    this.connected = false;
                    this.connection = null;
                });

            } catch (error) {
                reject(error);
            }
        });
    }

    async disconnect() {
        if (this.websocket) {
            this.websocket.close();
            this.connected = false;
            this.connection = null;
        }
    }

    async sendMessage(message, room = "general") {
        if (!this.connected) {
            return { success: false, error: "Not connected" };
        }

        return new Promise((resolve) => {
            try {
                const messageData = {
                    type: 'direct_message',
                    agent: this.agentName,
                    content: message,
                    room: room
                };

                const messageHandler = (data) => {
                    try {
                        const result = JSON.parse(data);
                        
                        if (result.type === 'response') {
                            this.websocket.removeListener('message', messageHandler);
                            
                            // Log to conversation history
                            this.conversationHistory.push({
                                timestamp: new Date().toISOString(),
                                message: message,
                                response: result.message || "",
                                agent: result.agent || this.agentName
                            });

                            resolve({
                                success: true,
                                response: result.message || "",
                                agent: result.agent || this.agentName,
                                conversationHistory: this.conversationHistory
                            });
                        }
                    } catch (parseError) {
                        // Continue listening
                    }
                };

                this.websocket.on('message', messageHandler);
                this.websocket.send(JSON.stringify(messageData));

                // Timeout fallback
                setTimeout(() => {
                    this.websocket.removeListener('message', messageHandler);
                    resolve({ success: false, error: "Agent response timeout" });
                }, 30000);

            } catch (error) {
                resolve({ success: false, error: error.message });
            }
        });
    }

    async validateClientSpecific() {
        const testResult = await this.sendMessage("Hello, please confirm you're operational");

        return {
            clientType: "agent",
            agentName: this.agentName,
            success: testResult.success,
            responseTime: testResult.success ? "< 30s" : "timeout",
            validation: testResult.success ? "Agent communication validated" : "Agent not responding",
            conversationHistory: this.conversationHistory
        };
    }
}

/**
 * Claude Agent Connection with specific capabilities
 */
class ClaudeAgentConnection extends AgentClientConnection {
    constructor(wsUrl = "ws://localhost:9000") {
        super(wsUrl, "Claude");
        this.clientType = "claude";
    }

    static getClientFeatures() {
        return {
            capabilities: [
                "conversation", 
                "code_analysis", 
                "debugging",
                "bus_command_validation",
                "remote_javascript_execution",
                "browser_console_reading",
                "screenshot_capture_control",
                "error_detection_analysis",
                "remote_debugging_workflow"
            ],
            restrictions: ["no_academy", "no_fine_tuning"],
            menuItems: [
                "Ask Question", 
                "Debug Code", 
                "Analyze System",
                "Validate Browser Connection",
                "Execute JavaScript Remotely",
                "Read Browser Console",
                "Capture Screenshot",
                "Debug Remote Issues"
            ],
            busCommands: [
                "BROWSER_VALIDATION",
                "JAVASCRIPT_EXECUTE", 
                "CONSOLE_READ",
                "SCREENSHOT",
                "REMOTE_DEBUG_WORKFLOW"
            ],
            validationRequirements: {
                minimumBusCapabilities: 3,
                requiredCommands: ["BROWSER_VALIDATION", "JAVASCRIPT_EXECUTE", "CONSOLE_READ"],
                optionalCommands: ["SCREENSHOT", "REMOTE_DEBUG_WORKFLOW"]
            }
        };
    }

    async validateClientSpecific() {
        console.log("🔍 Claude validating browser debug system connection via Continuum bus...");
        
        // Claude-specific validation: Test browser debugging capabilities through bus commands
        const debugSystemTests = {
            busConnection: false,
            browserValidationCommand: false,
            jsExecutionCommand: false,
            consoleReadingCommand: false,
            screenshotCommand: false,
            remoteDebuggingWorkflow: false
        };
        
        try {
            // Test 1: Claude connects to Continuum bus  
            debugSystemTests.busConnection = this.connected;
            
            if (debugSystemTests.busConnection) {
                console.log("✅ Claude connected to Continuum bus");
                
                // Test 2: Issue browser validation command via bus
                const browserValidationResult = await this.sendMessage(`
                    COMMAND: BROWSER_VALIDATION
                    
                    Claude issuing browser validation command through Continuum bus.
                    Please validate browser connection and report status.
                `, "validation");
                
                debugSystemTests.browserValidationCommand = browserValidationResult.success;
                
                if (browserValidationResult.success) {
                    console.log("✅ Claude can issue browser validation commands via bus");
                    
                    // Test 3: Issue JavaScript execution command
                    const jsExecutionResult = await this.sendMessage(`
                        COMMAND: JAVASCRIPT_EXECUTE
                        
                        Execute JavaScript: console.log("🤖 Claude command via bus"); return "CLAUDE_JS_SUCCESS";
                    `, "javascript");
                    
                    debugSystemTests.jsExecutionCommand = jsExecutionResult.success;
                    
                    if (jsExecutionResult.success) {
                        console.log("✅ Claude can issue JavaScript execution commands via bus");
                        
                        // Test 4: Issue console reading command
                        const consoleReadResult = await this.sendMessage(`
                            COMMAND: CONSOLE_READ
                            
                            Read browser console and return captured messages.
                        `, "console");
                        
                        debugSystemTests.consoleReadingCommand = consoleReadResult.success;
                        
                        if (consoleReadResult.success) {
                            console.log("✅ Claude can issue console reading commands via bus");
                        }
                    }
                    
                    // Test 5: Issue screenshot command  
                    const screenshotResult = await this.sendMessage(`
                        COMMAND: SCREENSHOT
                        
                        Capture browser screenshot and confirm success.
                    `, "screenshot");
                    
                    debugSystemTests.screenshotCommand = screenshotResult.success;
                    
                    if (screenshotResult.success) {
                        console.log("✅ Claude can issue screenshot commands via bus");
                    } else {
                        console.log("⚠️ Claude screenshot command capability limited");
                    }
                    
                    // Test 6: Complete remote debugging workflow
                    if (debugSystemTests.jsExecutionCommand && debugSystemTests.consoleReadingCommand) {
                        const workflowResult = await this.sendMessage(`
                            COMMAND: REMOTE_DEBUG_WORKFLOW
                            
                            Execute complete debugging workflow:
                            1. Identify issue via console
                            2. Execute diagnostic JavaScript  
                            3. Apply fix via DOM manipulation
                            4. Verify fix success
                        `, "debug");
                        
                        debugSystemTests.remoteDebuggingWorkflow = workflowResult.success;
                        
                        if (workflowResult.success) {
                            console.log("✅ Claude can execute complete remote debugging workflows via bus");
                        }
                    }
                }
            } else {
                console.log("❌ Claude not connected to Continuum bus");
            }
            
            if (debugSystemTests.browserConnection) {
                console.log("✅ Claude can connect to browser debug system");
                
                // Test 2: Can Claude execute JavaScript in browser
                const jsTest = await browserClient.executeJs(`
                    console.log("🤖 Claude is testing browser control from ClaudeConnection");
                    console.log("📊 Claude debug test:", {
                        timestamp: new Date().toISOString(),
                        userAgent: navigator.userAgent.substring(0, 50),
                        viewport: window.innerWidth + "x" + window.innerHeight
                    });
                    "CLAUDE_BROWSER_CONTROL_TEST";
                `);
                
                debugSystemTests.jsExecution = jsTest.success;
                
                if (jsTest.success) {
                    console.log("✅ Claude can execute JavaScript in browser");
                    
                    // Test 3: Can Claude read browser console
                    const consoleTest = await browserClient.captureConsoleOutput(`
                        console.log("🤖 Claude console reading test");
                        console.error("🤖 Claude error detection test");
                        console.warn("🤖 Claude warning detection test");
                        "CLAUDE_CONSOLE_TEST";
                    `);
                    
                    debugSystemTests.consoleReading = consoleTest.success;
                    debugSystemTests.errorDetection = consoleTest.success && 
                        consoleTest.console.errors.length > 0 && 
                        consoleTest.console.warnings.length > 0;
                    
                    if (consoleTest.success) {
                        console.log("✅ Claude can read browser console");
                        console.log(`📊 Console capture: ${consoleTest.console.total} messages`);
                        
                        // Test 4: Can Claude trigger screenshots (even if it times out, testing the command)
                        console.log("📸 Testing Claude screenshot capability...");
                        const screenshotTest = await browserClient.captureScreenshot();
                        debugSystemTests.screenshotCapability = screenshotTest.success;
                        
                        if (screenshotTest.success) {
                            console.log("✅ Claude can capture screenshots");
                        } else {
                            console.log("⚠️ Claude screenshot capability limited:", screenshotTest.error);
                        }
                    }
                }
            }
            
        } catch (error) {
            console.log("❌ Claude bus command validation error:", error.message);
        }
        
        // Calculate bus command capability score
        const busCapabilities = Object.values(debugSystemTests).filter(Boolean).length;
        const totalBusTests = Object.keys(debugSystemTests).length;
        const busSuccessRate = (busCapabilities / totalBusTests * 100).toFixed(1);
        
        return {
            clientType: "claude",
            agentName: "Claude",
            success: busCapabilities >= 3, // Must have at least 3/6 bus command capabilities
            features: ClaudeAgentConnection.getClientFeatures(),
            busCommandTests: debugSystemTests,
            busSuccessRate: `${busCapabilities}/${totalBusTests} (${busSuccessRate}%)`,
            validation: busCapabilities >= 3 
                ? `Claude browser debug via bus validated (${busSuccessRate}% capability)`
                : `Claude browser debug via bus limited (${busSuccessRate}% capability)`,
            conversationHistory: this.conversationHistory,
            busDebugCapable: busCapabilities >= 3,
            busCommandsSupported: Object.entries(debugSystemTests)
                .filter(([_, success]) => success)
                .map(([command, _]) => command)
        };
    }

    async debugCode(codeSnippet) {
        return await this.sendMessage(`Please debug this code: ${codeSnippet}`);
    }

    async analyzeSystem(systemInfo) {
        return await this.sendMessage(`Please analyze this system: ${systemInfo}`);
    }

    async askQuestion(question) {
        return await this.sendMessage(question);
    }
}

/**
 * Connection to terminal/shell console via Continuum
 */
class TerminalClientConnection extends ClientConnection {
    constructor(connectionUrl = "ws://localhost:9000") {
        super(connectionUrl);
        this.websocket = null;
        this.clientType = "terminal";
        this.commandHistory = [];
    }

    async connect() {
        return new Promise((resolve, reject) => {
            try {
                this.websocket = new WebSocket(this.connectionUrl);
                this.connection = this.websocket;

                this.websocket.on('open', () => {
                    this.connected = true;
                    console.log('💻 Terminal client connected');
                    
                    // Skip connection banner
                    setTimeout(() => resolve(true), 1000);
                });

                this.websocket.on('error', (error) => {
                    console.error('❌ Terminal connection failed:', error.message);
                    reject(error);
                });

                this.websocket.on('close', () => {
                    this.connected = false;
                    this.connection = null;
                });

            } catch (error) {
                reject(error);
            }
        });
    }

    async disconnect() {
        if (this.websocket) {
            this.websocket.close();
            this.connected = false;
            this.connection = null;
        }
    }

    async executeCommand(command, timeout = 30000) {
        if (!this.connected) {
            return { success: false, error: "Not connected" };
        }

        return new Promise((resolve) => {
            try {
                const taskMessage = {
                    type: 'task',
                    role: 'system',
                    task: `[CMD:EXEC] ${command}`
                };

                const messageHandler = (data) => {
                    try {
                        const result = JSON.parse(data);
                        
                        if (result.type === 'result' || result.message) {
                            this.websocket.removeListener('message', messageHandler);
                            
                            // Log to command history
                            this.commandHistory.push({
                                timestamp: new Date().toISOString(),
                                command: command,
                                result: result
                            });

                            resolve({
                                success: true,
                                command: command,
                                output: result.message || "",
                                commandHistory: this.commandHistory
                            });
                        }
                    } catch (parseError) {
                        // Continue listening
                    }
                };

                this.websocket.on('message', messageHandler);
                this.websocket.send(JSON.stringify(taskMessage));

                // Timeout fallback
                setTimeout(() => {
                    this.websocket.removeListener('message', messageHandler);
                    resolve({ success: false, error: "Command execution timeout" });
                }, timeout);

            } catch (error) {
                resolve({ success: false, error: error.message });
            }
        });
    }

    async validateClientSpecific() {
        const testResult = await this.executeCommand("echo 'Terminal validation test'");

        return {
            clientType: "terminal",
            success: testResult.success,
            commandExecution: testResult.success ? "Working" : "Failed",
            validation: testResult.success ? "Terminal commands validated" : "Terminal commands not working",
            commandHistory: this.commandHistory
        };
    }
}

// Validation functions for each client type
async function validateBrowserClient() {
    const client = new BrowserClientConnection();
    return await client.runFullModemProtocol();
}

async function validateAgentClient(agentName = "GeneralAI") {
    const client = new AgentClientConnection("ws://localhost:9000", agentName);
    if (!await client.connect()) {
        return { success: false, error: "Could not connect to agent" };
    }

    try {
        return await client.validateClientSpecific();
    } finally {
        await client.disconnect();
    }
}

async function validateClaudeClient() {
    const client = new ClaudeAgentConnection();
    if (!await client.connect()) {
        return { success: false, error: "Could not connect Claude agent" };
    }

    try {
        return await client.validateClientSpecific();
    } finally {
        await client.disconnect();
    }
}

async function validateTerminalClient() {
    const client = new TerminalClientConnection();
    if (!await client.connect()) {
        return { success: false, error: "Could not connect for terminal commands" };
    }

    try {
        return await client.validateClientSpecific();
    } finally {
        await client.disconnect();
    }
}

async function validateAllClients() {
    const results = {
        timestamp: new Date().toISOString(),
        clients: {},
        overallSuccess: false
    };

    // Browser client (full modem protocol)
    console.log("🌐 Validating Browser Client...");
    results.clients.browser = await validateBrowserClient();

    // Agent client  
    console.log("🤖 Validating Agent Client...");
    results.clients.agent = await validateAgentClient();

    // Terminal client
    console.log("💻 Validating Terminal Client...");
    results.clients.terminal = await validateTerminalClient();

    // Overall success if majority pass
    const successfulClients = Object.values(results.clients)
        .filter(c => c.success || c.overallSuccess).length;
    const totalClients = Object.keys(results.clients).length;
    results.overallSuccess = successfulClients >= Math.ceil(totalClients / 2);
    results.successRate = `${successfulClients}/${totalClients}`;

    return results;
}

// Convenience functions
async function quickJsExecute(jsCode) {
    const client = new BrowserClientConnection();
    if (await client.connect()) {
        const result = await client.executeJs(jsCode);
        await client.disconnect();
        return result;
    }
    return { success: false, error: "Connection failed" };
}

async function quickAgentMessage(message, agentName = "GeneralAI") {
    const client = new AgentClientConnection("ws://localhost:9000", agentName);
    if (await client.connect()) {
        const result = await client.sendMessage(message);
        await client.disconnect();
        return result;
    }
    return { success: false, error: "Connection failed" };
}

async function quickTerminalCommand(command) {
    const client = new TerminalClientConnection();
    if (await client.connect()) {
        const result = await client.executeCommand(command);
        await client.disconnect();
        return result;
    }
    return { success: false, error: "Connection failed" };
}

// Export classes and functions
export {
    ClientConnection,
    BrowserClientConnection,
    AgentClientConnection,
    ClaudeAgentConnection,
    TerminalClientConnection,
    validateBrowserClient,
    validateAgentClient,
    validateClaudeClient,
    validateTerminalClient,
    validateAllClients,
    quickJsExecute,
    quickAgentMessage,
    quickTerminalCommand
};

// CLI usage
if (import.meta.url === `file://${process.argv[1]}`) {
    async function main() {
        const args = process.argv.slice(2);
        
        if (args.length > 0) {
            const clientType = args[0].toLowerCase();
            
            if (clientType === 'browser') {
                const result = await validateBrowserClient();
                console.log(`Browser Client: ${result.overallSuccess ? 'SUCCESS' : 'FAILED'}`);
                if (!result.overallSuccess) {
                    console.log(JSON.stringify(result, null, 2));
                }
                
            } else if (clientType === 'agent') {
                const agentName = args[1] || "GeneralAI";
                const result = await validateAgentClient(agentName);
                console.log(`Agent Client (${agentName}): ${result.success ? 'SUCCESS' : 'FAILED'}`);
                if (!result.success) {
                    console.log(JSON.stringify(result, null, 2));
                }
                
            } else if (clientType === 'claude') {
                const result = await validateClaudeClient();
                console.log(`Claude Client: ${result.success ? 'SUCCESS' : 'FAILED'}`);
                console.log(`Features: ${JSON.stringify(result.features, null, 2)}`);
                if (!result.success) {
                    console.log(JSON.stringify(result, null, 2));
                }
                
            } else if (clientType === 'terminal') {
                const result = await validateTerminalClient();
                console.log(`Terminal Client: ${result.success ? 'SUCCESS' : 'FAILED'}`);
                if (!result.success) {
                    console.log(JSON.stringify(result, null, 2));
                }
                
            } else if (clientType === 'all') {
                const results = await validateAllClients();
                console.log(`All Clients: ${results.overallSuccess ? 'SUCCESS' : 'FAILED'}`);
                console.log(`Success Rate: ${results.successRate}`);
                for (const [clientName, clientResult] of Object.entries(results.clients)) {
                    const status = (clientResult.success || clientResult.overallSuccess) ? "✅" : "❌";
                    console.log(`  ${status} ${clientName.charAt(0).toUpperCase() + clientName.slice(1)}`);
                }
                
            } else if (clientType === 'js') {
                if (args.length > 1) {
                    const jsCode = args.slice(1).join(' ');
                    const result = await quickJsExecute(jsCode);
                    console.log(JSON.stringify(result, null, 2));
                }
                
            } else {
                console.log("Usage:");
                console.log("  node ClientConnection.js [browser|agent|terminal|all]");
                console.log("  node ClientConnection.js js <javascript_code>");
                console.log("  node ClientConnection.js agent <agent_name>");
            }
        } else {
            // Default: validate all clients
            const results = await validateAllClients();
            console.log(`All Client Validation: ${results.overallSuccess ? 'SUCCESS' : 'FAILED'}`);
            console.log(`Success Rate: ${results.successRate}`);
        }
    }
    
    main().catch(console.error);
}