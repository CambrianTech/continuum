/**
 * DevTools Protocol Client - Raw Protocol Communication
 * ====================================================
 * 
 * Low-level client for DevTools Protocol communication.
 * Handles raw protocol messages, connection management, and
 * provides foundation for higher-level automation features.
 * 
 * FEATURES:
 * - WebSocket connection management
 * - Command/response correlation
 * - Event subscription and handling
 * - Connection health monitoring
 * - Automatic reconnection
 */

class DevToolsProtocolClient {
    constructor(port, sessionId) {
        this.port = port;
        this.sessionId = sessionId;
        this.websocket = null;
        this.connected = false;
        this.messageId = 1;
        this.pendingCommands = new Map();
        this.eventSubscriptions = new Map();
        this.connectionAttempts = 0;
        this.maxReconnectAttempts = 3;
        
        // Health monitoring
        this.lastPingTime = null;
        this.pingInterval = null;
    }

    /**
     * Connect to DevTools Protocol endpoint
     */
    async connect() {
        try {
            // Get available tabs
            const response = await fetch(`http://localhost:${this.port}/json`);
            const tabs = await response.json();
            
            if (tabs.length === 0) {
                throw new Error('No tabs available for connection');
            }

            // Connect to first available tab (or specific tab if needed)
            const targetTab = tabs[0];
            const { WebSocket } = require('ws');
            this.websocket = new WebSocket(targetTab.webSocketDebuggerUrl);

            this.setupWebSocketHandlers();

            // Wait for connection
            await new Promise((resolve, reject) => {
                const timeout = setTimeout(() => {
                    reject(new Error('Connection timeout'));
                }, 10000);

                this.websocket.on('open', () => {
                    clearTimeout(timeout);
                    this.connected = true;
                    this.connectionAttempts = 0;
                    console.log(`🔌 DevTools Protocol client connected: ${this.sessionId}`);
                    this.startHealthMonitoring();
                    resolve();
                });

                this.websocket.on('error', (error) => {
                    clearTimeout(timeout);
                    reject(error);
                });
            });

        } catch (error) {
            throw new Error(`Failed to connect DevTools Protocol client: ${error.message}`);
        }
    }

    /**
     * Setup WebSocket event handlers
     */
    setupWebSocketHandlers() {
        this.websocket.on('message', (data) => {
            try {
                const message = JSON.parse(data.toString());
                this.handleMessage(message);
            } catch (error) {
                console.error(`❌ Failed to parse DevTools message:`, error);
            }
        });

        this.websocket.on('close', () => {
            this.connected = false;
            this.stopHealthMonitoring();
            console.log(`🔌 DevTools Protocol client disconnected: ${this.sessionId}`);
            
            // Attempt reconnection if not intentional
            if (this.connectionAttempts < this.maxReconnectAttempts) {
                this.attemptReconnection();
            }
        });

        this.websocket.on('error', (error) => {
            console.error(`❌ DevTools Protocol client error: ${this.sessionId}`, error);
        });
    }

    /**
     * Handle incoming WebSocket messages
     */
    handleMessage(message) {
        if (message.id !== undefined) {
            // Response to command
            this.handleCommandResponse(message);
        } else if (message.method) {
            // Event notification
            this.handleEvent(message);
        }
    }

    /**
     * Handle command responses
     */
    handleCommandResponse(message) {
        const pendingCommand = this.pendingCommands.get(message.id);
        if (pendingCommand) {
            this.pendingCommands.delete(message.id);
            const { resolve, reject } = pendingCommand;

            if (message.error) {
                reject(new Error(`DevTools Protocol error: ${message.error.message}`));
            } else {
                resolve(message.result || {});
            }
        }
    }

    /**
     * Handle event notifications
     */
    handleEvent(message) {
        const subscribers = this.eventSubscriptions.get(message.method);
        if (subscribers) {
            subscribers.forEach(callback => {
                try {
                    callback(message.params || {});
                } catch (error) {
                    console.error(`❌ Event handler error for ${message.method}:`, error);
                }
            });
        }
    }

    /**
     * Send DevTools Protocol command
     */
    async sendCommand(method, params = {}, targetTabId = null) {
        if (!this.connected) {
            throw new Error('DevTools Protocol client not connected');
        }

        const messageId = this.messageId++;
        const command = {
            id: messageId,
            method: method,
            params: params
        };

        // Add session target if specified
        if (targetTabId) {
            command.sessionId = targetTabId;
        }

        return new Promise((resolve, reject) => {
            // Store pending command
            this.pendingCommands.set(messageId, { resolve, reject });

            // Send command
            this.websocket.send(JSON.stringify(command));

            // Timeout after 15 seconds
            setTimeout(() => {
                if (this.pendingCommands.has(messageId)) {
                    this.pendingCommands.delete(messageId);
                    reject(new Error(`Command ${method} timed out`));
                }
            }, 15000);
        });
    }

    /**
     * Subscribe to DevTools Protocol events
     */
    subscribe(eventMethod, callback) {
        if (!this.eventSubscriptions.has(eventMethod)) {
            this.eventSubscriptions.set(eventMethod, new Set());
        }
        this.eventSubscriptions.get(eventMethod).add(callback);
    }

    /**
     * Unsubscribe from DevTools Protocol events
     */
    unsubscribe(eventMethod, callback) {
        const subscribers = this.eventSubscriptions.get(eventMethod);
        if (subscribers) {
            subscribers.delete(callback);
            if (subscribers.size === 0) {
                this.eventSubscriptions.delete(eventMethod);
            }
        }
    }

    /**
     * Execute JavaScript in browser
     */
    async executeJavaScript(expression, tabId = null) {
        const result = await this.sendCommand('Runtime.evaluate', {
            expression: expression,
            returnByValue: true,
            awaitPromise: true
        }, tabId);

        if (result.exceptionDetails) {
            throw new Error(`JavaScript execution failed: ${result.exceptionDetails.text}`);
        }

        return result.result;
    }

    /**
     * Create new tab
     */
    async createTab(url = 'about:blank') {
        // Use DevTools API to create new tab
        const response = await fetch(`http://localhost:${this.port}/json/new?${encodeURIComponent(url)}`, {
            method: 'PUT'
        });

        if (!response.ok) {
            throw new Error(`Failed to create tab: ${response.status}`);
        }

        return await response.json();
    }

    /**
     * Navigate to URL
     */
    async navigate(url, tabId = null) {
        return await this.sendCommand('Page.navigate', { url }, tabId);
    }

    /**
     * Take screenshot
     */
    async takeScreenshot(options = {}, tabId = null) {
        const screenshotParams = {
            format: options.format || 'png',
            quality: options.quality || 90,
            ...options
        };

        const result = await this.sendCommand('Page.captureScreenshot', screenshotParams, tabId);
        return result.data; // Base64 encoded image
    }

    /**
     * Enable DevTools domain
     */
    async enableDomain(domain, tabId = null) {
        return await this.sendCommand(`${domain}.enable`, {}, tabId);
    }

    /**
     * Disable DevTools domain
     */
    async disableDomain(domain, tabId = null) {
        return await this.sendCommand(`${domain}.disable`, {}, tabId);
    }

    /**
     * Get browser version info
     */
    async getBrowserVersion() {
        const response = await fetch(`http://localhost:${this.port}/json/version`);
        return await response.json();
    }

    /**
     * List all tabs
     */
    async listTabs() {
        const response = await fetch(`http://localhost:${this.port}/json`);
        return await response.json();
    }

    /**
     * Start health monitoring
     */
    startHealthMonitoring() {
        this.pingInterval = setInterval(async () => {
            try {
                this.lastPingTime = Date.now();
                await this.sendCommand('Runtime.evaluate', {
                    expression: 'Date.now()',
                    returnByValue: true
                });
            } catch (error) {
                console.warn(`⚠️ Health check failed for ${this.sessionId}:`, error.message);
            }
        }, 30000); // Ping every 30 seconds
    }

    /**
     * Stop health monitoring
     */
    stopHealthMonitoring() {
        if (this.pingInterval) {
            clearInterval(this.pingInterval);
            this.pingInterval = null;
        }
    }

    /**
     * Attempt reconnection
     */
    async attemptReconnection() {
        this.connectionAttempts++;
        console.log(`🔄 Attempting reconnection ${this.connectionAttempts}/${this.maxReconnectAttempts} for ${this.sessionId}`);

        try {
            await new Promise(resolve => setTimeout(resolve, 2000)); // Wait 2 seconds
            await this.connect();
        } catch (error) {
            console.error(`❌ Reconnection attempt ${this.connectionAttempts} failed:`, error.message);
        }
    }

    /**
     * Check if client is connected
     */
    isConnected() {
        return this.connected && this.websocket && this.websocket.readyState === 1;
    }

    /**
     * Get connection health information
     */
    getHealthInfo() {
        return {
            connected: this.isConnected(),
            sessionId: this.sessionId,
            port: this.port,
            lastPingTime: this.lastPingTime,
            connectionAttempts: this.connectionAttempts,
            pendingCommands: this.pendingCommands.size,
            eventSubscriptions: this.eventSubscriptions.size
        };
    }

    /**
     * Disconnect client
     */
    async disconnect() {
        this.stopHealthMonitoring();
        
        // Clear pending commands
        for (const [id, { reject }] of this.pendingCommands.entries()) {
            reject(new Error('Client disconnected'));
        }
        this.pendingCommands.clear();

        // Clear event subscriptions
        this.eventSubscriptions.clear();

        // Close WebSocket
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
        }

        this.connected = false;
        console.log(`🔌 DevTools Protocol client disconnected: ${this.sessionId}`);
    }
}

module.exports = DevToolsProtocolClient;