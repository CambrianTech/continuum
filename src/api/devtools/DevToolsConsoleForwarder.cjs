/**
 * DevTools Console Forwarder - Real-time Console Log Streaming
 * ===========================================================
 * 
 * Captures and forwards browser console messages to server-side logging.
 * Provides real-time console monitoring for debugging and automation.
 * 
 * FEATURES:
 * - Real-time console message capture
 * - Message filtering and formatting
 * - Multiple output destinations
 * - Session-specific log isolation
 * - Error aggregation and reporting
 */

const { WebSocket } = require('ws');

class DevToolsConsoleForwarder {
    constructor(port, sessionId) {
        this.port = port;
        this.sessionId = sessionId;
        this.websocket = null;
        this.connected = false;
        this.messageBuffer = [];
        this.maxBufferSize = 1000;
        this.messageHandlers = new Set();
        this.filterRules = new Set();
        
        // Message statistics
        this.stats = {
            totalMessages: 0,
            errorMessages: 0,
            warningMessages: 0,
            infoMessages: 0,
            debugMessages: 0
        };
    }

    /**
     * Start console forwarding
     */
    async start() {
        try {
            // Get available tabs
            const response = await fetch(`http://localhost:${this.port}/json`);
            const tabs = await response.json();
            
            const targetTab = tabs.find(tab => tab.url.includes('localhost:9000'));
            if (!targetTab) {
                throw new Error('No suitable tab found for console forwarding');
            }

            // Connect to tab WebSocket
            this.websocket = new WebSocket(targetTab.webSocketDebuggerUrl);
            
            this.websocket.on('open', async () => {
                this.connected = true;
                console.log(`🔌 Console forwarder connected: ${this.sessionId}`);
                
                // Enable console domain
                await this.sendCommand('Runtime.enable');
                await this.sendCommand('Console.enable');
                
                // Subscribe to console events
                this.setupConsoleHandlers();
            });

            this.websocket.on('message', (data) => {
                this.handleMessage(JSON.parse(data.toString()));
            });

            this.websocket.on('close', () => {
                this.connected = false;
                console.log(`🔌 Console forwarder disconnected: ${this.sessionId}`);
            });

            this.websocket.on('error', (error) => {
                console.error(`❌ Console forwarder error: ${this.sessionId}`, error);
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
            throw new Error(`Failed to start console forwarder: ${error.message}`);
        }
    }

    /**
     * Send command to DevTools
     */
    async sendCommand(method, params = {}) {
        return new Promise((resolve, reject) => {
            if (!this.connected) {
                reject(new Error('Console forwarder not connected'));
                return;
            }

            const messageId = Date.now();
            const command = {
                id: messageId,
                method: method,
                params: params
            };

            const timeout = setTimeout(() => {
                reject(new Error(`Command ${method} timed out`));
            }, 5000);

            const messageHandler = (data) => {
                const message = JSON.parse(data.toString());
                if (message.id === messageId) {
                    clearTimeout(timeout);
                    this.websocket.off('message', messageHandler);
                    if (message.error) {
                        reject(new Error(message.error.message));
                    } else {
                        resolve(message.result);
                    }
                }
            };

            this.websocket.on('message', messageHandler);
            this.websocket.send(JSON.stringify(command));
        });
    }

    /**
     * Setup console event handlers
     */
    setupConsoleHandlers() {
        // Handler for console messages
        this.messageHandlers.add((message) => {
            if (message.method === 'Runtime.consoleAPICalled') {
                this.handleConsoleMessage(message.params);
            } else if (message.method === 'Runtime.exceptionThrown') {
                this.handleException(message.params);
            }
        });
    }

    /**
     * Handle incoming WebSocket messages
     */
    handleMessage(message) {
        this.messageHandlers.forEach(handler => {
            try {
                handler(message);
            } catch (error) {
                console.error(`❌ Console message handler error:`, error);
            }
        });
    }

    /**
     * Handle console API calls (console.log, console.error, etc.)
     */
    handleConsoleMessage(params) {
        const consoleMessage = {
            type: params.type,
            timestamp: new Date(params.timestamp),
            args: params.args.map(arg => this.formatArgument(arg)),
            level: this.mapConsoleLevel(params.type),
            sessionId: this.sessionId,
            source: 'console'
        };

        this.processMessage(consoleMessage);
    }

    /**
     * Handle JavaScript exceptions
     */
    handleException(params) {
        const exceptionMessage = {
            type: 'exception',
            timestamp: new Date(params.timestamp),
            args: [params.exceptionDetails.text],
            level: 'error',
            sessionId: this.sessionId,
            source: 'exception',
            stackTrace: params.exceptionDetails.stackTrace
        };

        this.processMessage(exceptionMessage);
    }

    /**
     * Format console argument for logging
     */
    formatArgument(arg) {
        switch (arg.type) {
            case 'string':
                return arg.value;
            case 'number':
                return arg.value;
            case 'boolean':
                return arg.value;
            case 'object':
                return arg.description || '[Object]';
            case 'function':
                return arg.description || '[Function]';
            case 'undefined':
                return 'undefined';
            case 'null':
                return 'null';
            default:
                return arg.description || String(arg.value);
        }
    }

    /**
     * Map console type to log level
     */
    mapConsoleLevel(type) {
        const levelMap = {
            'log': 'info',
            'info': 'info',
            'warn': 'warning',
            'warning': 'warning',
            'error': 'error',
            'debug': 'debug',
            'trace': 'debug',
            'dir': 'info',
            'dirxml': 'info',
            'table': 'info',
            'clear': 'info'
        };
        return levelMap[type] || 'info';
    }

    /**
     * Process and forward console message
     */
    processMessage(message) {
        // Update statistics
        this.stats.totalMessages++;
        this.stats[`${message.level}Messages`] = (this.stats[`${message.level}Messages`] || 0) + 1;

        // Apply filters
        if (this.shouldFilterMessage(message)) {
            return;
        }

        // Add to buffer
        this.messageBuffer.push(message);
        if (this.messageBuffer.length > this.maxBufferSize) {
            this.messageBuffer.shift(); // Remove oldest message
        }

        // Forward to registered handlers
        this.forwardMessage(message);
    }

    /**
     * Check if message should be filtered
     */
    shouldFilterMessage(message) {
        for (const rule of this.filterRules) {
            if (rule(message)) {
                return true;
            }
        }
        return false;
    }

    /**
     * Forward message to registered handlers
     */
    forwardMessage(message) {
        // Format message for console output
        const formattedMessage = this.formatForConsole(message);
        
        // Output to console with appropriate level
        switch (message.level) {
            case 'error':
                console.error(formattedMessage);
                break;
            case 'warning':
                console.warn(formattedMessage);
                break;
            case 'debug':
                console.debug(formattedMessage);
                break;
            default:
                console.log(formattedMessage);
        }

        // Call custom message handlers
        this.messageHandlers.forEach(handler => {
            if (handler.name === 'onMessage') {
                try {
                    handler(message);
                } catch (error) {
                    console.error(`❌ Custom message handler error:`, error);
                }
            }
        });
    }

    /**
     * Format message for console output
     */
    formatForConsole(message) {
        const timestamp = message.timestamp.toISOString().slice(11, 23);
        const level = message.level.toUpperCase().padEnd(7);
        const sessionInfo = `[${this.sessionId.slice(-8)}]`;
        const content = message.args.join(' ');
        
        return `🌐 [${timestamp}] ${level} ${sessionInfo}: ${content}`;
    }

    /**
     * Add custom message handler
     */
    onMessage(callback) {
        const handler = callback;
        handler.name = 'onMessage';
        this.messageHandlers.add(handler);
    }

    /**
     * Remove message handler
     */
    offMessage(callback) {
        this.messageHandlers.delete(callback);
    }

    /**
     * Add message filter rule
     */
    addFilter(filterFunction) {
        this.filterRules.add(filterFunction);
    }

    /**
     * Remove message filter rule
     */
    removeFilter(filterFunction) {
        this.filterRules.delete(filterFunction);
    }

    /**
     * Get recent messages
     */
    getRecentMessages(count = 10) {
        return this.messageBuffer.slice(-count);
    }

    /**
     * Get messages by level
     */
    getMessagesByLevel(level, count = 10) {
        return this.messageBuffer
            .filter(msg => msg.level === level)
            .slice(-count);
    }

    /**
     * Get forwarding statistics
     */
    getStats() {
        return {
            ...this.stats,
            bufferSize: this.messageBuffer.length,
            connected: this.connected,
            sessionId: this.sessionId
        };
    }

    /**
     * Clear message buffer
     */
    clearBuffer() {
        this.messageBuffer = [];
    }

    /**
     * Stop console forwarding
     */
    async stop() {
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
        }
        this.connected = false;
        this.messageHandlers.clear();
        this.filterRules.clear();
        
        console.log(`🔌 Console forwarder stopped: ${this.sessionId}`);
    }
}

module.exports = DevToolsConsoleForwarder;