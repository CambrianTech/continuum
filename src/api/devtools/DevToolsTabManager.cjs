/**
 * DevTools Tab Manager - Individual Tab Control
 * =============================================
 * 
 * Manages individual browser tabs within a DevTools session.
 * Provides focused control over specific tabs including:
 * 
 * - Tab lifecycle management (create, navigate, close)
 * - Tab-specific JavaScript execution  
 * - Tab state monitoring and events
 * - Tab-specific console log capture
 * - Tab screenshot and UI interaction
 */

const { WebSocket } = require('ws');

class DevToolsTabManager {
    constructor(port, tabId) {
        this.port = port;
        this.tabId = tabId;
        this.websocket = null;
        this.connected = false;
        this.messageId = 1;
        this.pendingMessages = new Map();
        this.eventHandlers = new Map();
        
        // Tab state
        this.url = null;
        this.title = null;
        this.loadingState = 'unknown';
        
        this.setupEventHandlers();
    }

    setupEventHandlers() {
        // Default event handlers
        this.eventHandlers.set('Page.loadEventFired', () => {
            this.loadingState = 'loaded';
        });
        
        this.eventHandlers.set('Page.domContentEventFired', () => {
            this.loadingState = 'domReady';
        });
        
        this.eventHandlers.set('Page.frameNavigated', (params) => {
            if (params.frame.parentId === undefined) {
                // Main frame navigation
                this.url = params.frame.url;
            }
        });
    }

    /**
     * Connect to tab via WebSocket
     */
    async connect() {
        try {
            // Get tab info first
            const response = await fetch(`http://localhost:${this.port}/json`);
            const tabs = await response.json();
            const tabInfo = tabs.find(tab => tab.id === this.tabId);
            
            if (!tabInfo) {
                throw new Error(`Tab ${this.tabId} not found`);
            }

            // Connect to tab WebSocket
            this.websocket = new WebSocket(tabInfo.webSocketDebuggerUrl);
            
            this.websocket.on('open', () => {
                this.connected = true;
                console.log(`🔌 Tab ${this.tabId} connected`);
                
                // Enable domains
                this.enableDomains();
            });

            this.websocket.on('message', (data) => {
                this.handleMessage(JSON.parse(data.toString()));
            });

            this.websocket.on('close', () => {
                this.connected = false;
                console.log(`🔌 Tab ${this.tabId} disconnected`);
            });

            this.websocket.on('error', (error) => {
                console.error(`❌ Tab ${this.tabId} WebSocket error:`, error);
            });

            // Wait for connection
            await new Promise((resolve, reject) => {
                const timeout = setTimeout(() => reject(new Error('Connection timeout')), 5000);
                this.websocket.on('open', () => {
                    clearTimeout(timeout);
                    resolve();
                });
                this.websocket.on('error', (error) => {
                    clearTimeout(timeout);
                    reject(error);
                });
            });

        } catch (error) {
            throw new Error(`Failed to connect to tab ${this.tabId}: ${error.message}`);
        }
    }

    /**
     * Enable DevTools domains for tab
     */
    async enableDomains() {
        const domains = ['Page', 'Runtime', 'Console', 'DOM'];
        
        for (const domain of domains) {
            await this.sendCommand(`${domain}.enable`);
        }
    }

    /**
     * Handle incoming WebSocket messages
     */
    handleMessage(message) {
        if (message.id && this.pendingMessages.has(message.id)) {
            // Response to command
            const { resolve, reject } = this.pendingMessages.get(message.id);
            this.pendingMessages.delete(message.id);
            
            if (message.error) {
                reject(new Error(message.error.message));
            } else {
                resolve(message.result);
            }
        } else if (message.method) {
            // Event notification
            const handler = this.eventHandlers.get(message.method);
            if (handler) {
                handler(message.params);
            }
        }
    }

    /**
     * Send DevTools Protocol command to tab
     */
    async sendCommand(method, params = {}) {
        if (!this.connected) {
            throw new Error(`Tab ${this.tabId} not connected`);
        }

        const messageId = this.messageId++;
        const message = {
            id: messageId,
            method: method,
            params: params
        };

        return new Promise((resolve, reject) => {
            this.pendingMessages.set(messageId, { resolve, reject });
            
            this.websocket.send(JSON.stringify(message));
            
            // Timeout after 10 seconds
            setTimeout(() => {
                if (this.pendingMessages.has(messageId)) {
                    this.pendingMessages.delete(messageId);
                    reject(new Error(`Command ${method} timed out`));
                }
            }, 10000);
        });
    }

    /**
     * Execute JavaScript in tab
     */
    async executeJavaScript(expression) {
        const result = await this.sendCommand('Runtime.evaluate', {
            expression: expression,
            returnByValue: true
        });

        if (result.exceptionDetails) {
            throw new Error(`JavaScript execution error: ${result.exceptionDetails.text}`);
        }

        return result.result.value;
    }

    /**
     * Navigate tab to URL
     */
    async navigate(url) {
        this.loadingState = 'loading';
        await this.sendCommand('Page.navigate', { url });
        
        // Wait for page to load
        return new Promise((resolve) => {
            const checkLoaded = () => {
                if (this.loadingState === 'loaded') {
                    resolve();
                } else {
                    setTimeout(checkLoaded, 100);
                }
            };
            checkLoaded();
        });
    }

    /**
     * Take screenshot of tab
     */
    async takeScreenshot(options = {}) {
        const screenshotParams = {
            format: options.format || 'png',
            quality: options.quality || 90,
            ...options
        };

        const result = await this.sendCommand('Page.captureScreenshot', screenshotParams);
        return result.data; // Base64 encoded image
    }

    /**
     * Get tab title
     */
    async getTitle() {
        const result = await this.executeJavaScript('document.title');
        this.title = result;
        return result;
    }

    /**
     * Get tab URL
     */
    async getURL() {
        const result = await this.executeJavaScript('window.location.href');
        this.url = result;
        return result;
    }

    /**
     * Click element in tab
     */
    async clickElement(selector) {
        return await this.executeJavaScript(`
            const element = document.querySelector('${selector}');
            if (element) {
                element.click();
                return true;
            }
            return false;
        `);
    }

    /**
     * Type text in element
     */
    async typeInElement(selector, text) {
        return await this.executeJavaScript(`
            const element = document.querySelector('${selector}');
            if (element) {
                element.value = '${text}';
                element.dispatchEvent(new Event('input', { bubbles: true }));
                return true;
            }
            return false;
        `);
    }

    /**
     * Wait for element to appear
     */
    async waitForElement(selector, timeout = 5000) {
        const startTime = Date.now();
        
        while (Date.now() - startTime < timeout) {
            const exists = await this.executeJavaScript(`
                document.querySelector('${selector}') !== null
            `);
            
            if (exists) {
                return true;
            }
            
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        
        throw new Error(`Element ${selector} not found within ${timeout}ms`);
    }

    /**
     * Add event listener for specific DevTools events
     */
    addEventListener(eventType, handler) {
        this.eventHandlers.set(eventType, handler);
    }

    /**
     * Remove event listener
     */
    removeEventListener(eventType) {
        this.eventHandlers.delete(eventType);
    }

    /**
     * Get tab state information
     */
    getState() {
        return {
            tabId: this.tabId,
            connected: this.connected,
            url: this.url,
            title: this.title,
            loadingState: this.loadingState
        };
    }

    /**
     * Close tab connection
     */
    async close() {
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
        }
        this.connected = false;
        this.pendingMessages.clear();
    }
}

module.exports = DevToolsTabManager;