/**
 * BrowserManager - Intelligent Browser Connection Management
 * Detects existing connections, manages browser launching, and coordinates DevTools
 */
import { EventEmitter } from 'events';
import { spawn } from 'child_process';
export class BrowserManager extends EventEmitter {
    constructor(serverPort = 9000) {
        super();
        this.connectedClients = new Map();
        this.browserProcess = null;
        this.debugPort = 9222;
        this.lastConnectionCheck = 0;
        this.connectionCheckInterval = null;
        this.serverPort = serverPort;
        this.startConnectionMonitoring();
    }
    /**
     * Get current browser state
     */
    getBrowserState() {
        const now = Date.now();
        const activeClients = Array.from(this.connectedClients.values())
            .filter(client => (now - client.lastSeen) < 60000); // Active within last minute
        return {
            hasActiveConnections: activeClients.length > 0,
            connectedClients: activeClients.map(client => ({
                clientId: client.clientId,
                type: this.detectClientType(client),
                url: client.url || `http://localhost:${this.serverPort}`,
                lastSeen: client.lastSeen,
                capabilities: client.capabilities || []
            })),
            debugMode: this.browserProcess !== null,
            devToolsPort: this.browserProcess ? this.debugPort : undefined,
            browserProcess: this.browserProcess ? {
                pid: this.browserProcess.pid,
                command: 'browser-process',
                startTime: this.browserProcess.spawnargs ? Date.now() : 0
            } : undefined
        };
    }
    /**
     * Intelligently ensure browser connection exists
     */
    async ensureBrowserConnection(options = { mode: 'default' }) {
        console.log('🔍 Checking browser connection status...');
        const currentState = this.getBrowserState();
        // Check if we already have what we need
        if (this.hasRequiredConnection(currentState, options)) {
            console.log('✅ Required browser connection already exists');
            return currentState;
        }
        // Determine what action to take
        const action = this.determineBrowserAction(currentState, options);
        console.log(`🎯 Browser action needed: ${action}`);
        switch (action) {
            case 'none':
                return currentState;
            case 'open-tab':
                await this.openNewTab(options.url);
                break;
            case 'launch-browser':
                await this.launchBrowser(options);
                break;
            case 'launch-devtools':
                await this.launchBrowserWithDevTools(options);
                break;
            case 'activate-existing':
                await this.activateExistingBrowser();
                break;
        }
        // Wait for connection to establish
        await this.waitForConnection(5000);
        return this.getBrowserState();
    }
    /**
     * Register a client connection
     */
    registerClient(clientData) {
        const clientId = clientData.clientId || `client_${Date.now()}`;
        this.connectedClients.set(clientId, {
            ...clientData,
            lastSeen: Date.now(),
            registeredAt: Date.now()
        });
        console.log(`📱 Client registered: ${clientId} (type: ${this.detectClientType(clientData)})`);
        this.emit('client:connected', clientId, clientData);
    }
    /**
     * Update client last seen time
     */
    updateClientActivity(clientId) {
        const client = this.connectedClients.get(clientId);
        if (client) {
            client.lastSeen = Date.now();
        }
    }
    /**
     * Remove disconnected client
     */
    removeClient(clientId) {
        const client = this.connectedClients.get(clientId);
        if (client) {
            this.connectedClients.delete(clientId);
            console.log(`📴 Client disconnected: ${clientId}`);
            this.emit('client:disconnected', clientId, client);
        }
    }
    /**
     * Check if we have the required connection type
     */
    hasRequiredConnection(state, options) {
        if (!state.hasActiveConnections) {
            return false;
        }
        // For devtools mode, need debug port active
        if (options.mode === 'devtools') {
            return state.debugMode && state.devToolsPort !== undefined;
        }
        // For default mode, just need any browser connection
        if (options.mode === 'default') {
            return state.connectedClients.some(client => client.type === 'browser-tab' || client.type === 'thin-client');
        }
        return true;
    }
    /**
     * Determine what browser action is needed
     */
    determineBrowserAction(state, options) {
        // If we need DevTools but don't have it
        if (options.mode === 'devtools' && !state.debugMode) {
            return 'launch-devtools';
        }
        // If we have DevTools but need default browser
        if (options.mode === 'default' && state.debugMode && !state.hasActiveConnections) {
            return 'open-tab';
        }
        // If no browser at all
        if (!state.hasActiveConnections && !state.browserProcess) {
            return options.mode === 'devtools' ? 'launch-devtools' : 'launch-browser';
        }
        // If browser exists but no connections
        if (state.browserProcess && !state.hasActiveConnections) {
            return 'activate-existing';
        }
        return 'none';
    }
    /**
     * Launch browser with DevTools debugging
     */
    async launchBrowserWithDevTools(options) {
        console.log('🛠️ Launching browser with DevTools debugging...');
        const debugPort = options.debugPort || this.debugPort;
        const url = options.url || `http://localhost:${this.serverPort}`;
        // Try different browsers in order of preference
        const browsers = [
            {
                name: 'Opera GX',
                command: '/Applications/Opera GX.app/Contents/MacOS/Opera',
                args: [
                    `--remote-debugging-port=${debugPort}`,
                    '--disable-web-security',
                    '--disable-extensions',
                    '--user-data-dir=/tmp/continuum-debug',
                    url
                ]
            },
            {
                name: 'Google Chrome',
                command: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
                args: [
                    `--remote-debugging-port=${debugPort}`,
                    '--disable-web-security',
                    '--user-data-dir=/tmp/continuum-debug',
                    url
                ]
            },
            {
                name: 'Chromium',
                command: 'chromium',
                args: [
                    `--remote-debugging-port=${debugPort}`,
                    '--disable-web-security',
                    url
                ]
            }
        ];
        for (const browser of browsers) {
            try {
                await this.tryLaunchBrowser(browser.command, browser.args);
                console.log(`✅ Launched ${browser.name} with DevTools on port ${debugPort}`);
                this.debugPort = debugPort;
                return;
            }
            catch (error) {
                console.log(`⚠️ Failed to launch ${browser.name}, trying next...`);
            }
        }
        throw new Error('Failed to launch any browser with DevTools');
    }
    /**
     * Launch default browser
     */
    async launchBrowser(options) {
        console.log('🌐 Launching default browser...');
        const url = options.url || `http://localhost:${this.serverPort}`;
        // Use system default browser
        const command = process.platform === 'darwin' ? 'open' :
            process.platform === 'win32' ? 'start' : 'xdg-open';
        try {
            const process = spawn(command, [url], {
                detached: true,
                stdio: 'ignore'
            });
            process.unref();
            console.log(`✅ Opened ${url} in default browser`);
        }
        catch (error) {
            console.error('❌ Failed to launch default browser:', error);
            throw error;
        }
    }
    /**
     * Open new tab in existing browser
     */
    async openNewTab(url) {
        const targetUrl = url || `http://localhost:${this.serverPort}`;
        if (this.browserProcess && this.debugPort) {
            // Use DevTools API to open new tab
            try {
                const response = await fetch(`http://localhost:${this.debugPort}/json/new?${targetUrl}`);
                if (response.ok) {
                    console.log(`✅ Opened new tab: ${targetUrl}`);
                    return;
                }
            }
            catch (error) {
                console.warn('⚠️ DevTools API failed, falling back to system open');
            }
        }
        // Fallback to system open
        await this.launchBrowser({ mode: 'default', url: targetUrl });
    }
    /**
     * Activate existing browser window
     */
    async activateExistingBrowser() {
        if (process.platform === 'darwin') {
            // On macOS, try to activate browser windows
            try {
                spawn('osascript', ['-e', 'tell application "System Events" to set frontmost of first process whose name contains "chrom" to true'], {
                    stdio: 'ignore'
                });
            }
            catch (error) {
                console.warn('⚠️ Failed to activate browser window');
            }
        }
        // Also try opening the URL again
        await this.openNewTab();
    }
    /**
     * Try to launch a specific browser
     */
    async tryLaunchBrowser(command, args) {
        return new Promise((resolve, reject) => {
            try {
                this.browserProcess = spawn(command, args, {
                    detached: true,
                    stdio: 'ignore'
                });
                this.browserProcess.on('error', reject);
                // Give it a moment to start
                setTimeout(() => {
                    if (this.browserProcess && this.browserProcess.pid) {
                        this.browserProcess.unref();
                        resolve();
                    }
                    else {
                        reject(new Error('Browser process failed to start'));
                    }
                }, 1000);
            }
            catch (error) {
                reject(error);
            }
        });
    }
    /**
     * Wait for a client connection to establish
     */
    async waitForConnection(timeout = 5000) {
        const startTime = Date.now();
        return new Promise((resolve, reject) => {
            const checkConnection = () => {
                if (this.getBrowserState().hasActiveConnections) {
                    resolve();
                    return;
                }
                if (Date.now() - startTime > timeout) {
                    reject(new Error('Timeout waiting for browser connection'));
                    return;
                }
                setTimeout(checkConnection, 500);
            };
            checkConnection();
        });
    }
    /**
     * Detect client type based on properties
     */
    detectClientType(clientData) {
        if (clientData.userAgent?.includes('HeadlessChrome')) {
            return 'devtools';
        }
        if (clientData.capabilities?.includes('devtools-protocol')) {
            return 'devtools';
        }
        if (clientData.clientType === 'thin-client' || clientData.capabilities?.includes('browser-execution')) {
            return 'thin-client';
        }
        return 'browser-tab';
    }
    /**
     * Start monitoring connection health
     */
    startConnectionMonitoring() {
        this.connectionCheckInterval = setInterval(() => {
            this.cleanupStaleConnections();
        }, 30000); // Check every 30 seconds
    }
    /**
     * Remove stale connections
     */
    cleanupStaleConnections() {
        const now = Date.now();
        const staleThreshold = 120000; // 2 minutes
        for (const [clientId, client] of this.connectedClients.entries()) {
            if (now - client.lastSeen > staleThreshold) {
                console.log(`🧹 Removing stale connection: ${clientId}`);
                this.removeClient(clientId);
            }
        }
    }
    /**
     * Cleanup resources
     */
    async cleanup() {
        if (this.connectionCheckInterval) {
            clearInterval(this.connectionCheckInterval);
        }
        if (this.browserProcess) {
            try {
                this.browserProcess.kill();
            }
            catch (error) {
                console.warn('⚠️ Failed to kill browser process:', error);
            }
        }
        this.connectedClients.clear();
        console.log('🧹 BrowserManager cleanup completed');
    }
}
