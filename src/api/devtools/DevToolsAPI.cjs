/**
 * DevTools API - Unified Interface for Browser Automation
 * ======================================================
 * 
 * Comprehensive API module for all DevTools Protocol operations, session management,
 * and browser automation. Provides a clean, modular interface for:
 * 
 * - Session coordination and management
 * - Browser tab creation and control  
 * - Screenshot capture and UI automation
 * - Console log monitoring and forwarding
 * - DevTools Protocol command execution
 * 
 * ARCHITECTURE:
 * - SessionCoordinator: Multi-session browser management
 * - TabManager: Individual tab lifecycle and control
 * - ProtocolClient: Raw DevTools Protocol communication
 * - ScreenshotCapture: Advanced screenshot and UI capture
 * - ConsoleForwarder: Real-time console log streaming
 */

const { DevToolsSessionCoordinator, getDevToolsCoordinator } = require('../../core/DevToolsSessionCoordinator.cjs');
const DevToolsTabManager = require('./DevToolsTabManager.cjs');
const DevToolsProtocolClient = require('./DevToolsProtocolClient.cjs');
const DevToolsScreenshotCapture = require('./DevToolsScreenshotCapture.cjs');
const DevToolsConsoleForwarder = require('./DevToolsConsoleForwarder.cjs');

class DevToolsAPI {
    constructor() {
        this.coordinator = null;
        this.activeTabs = new Map(); // tabId -> TabManager instance
        this.protocolClients = new Map(); // sessionId -> ProtocolClient
        this.consoleForwarders = new Map(); // sessionId -> ConsoleForwarder
        this.screenshotCapture = null;
        
        this.init();
    }

    async init() {
        // Initialize singleton coordinator
        this.coordinator = getDevToolsCoordinator();
        
        // Initialize screenshot capture system
        this.screenshotCapture = new DevToolsScreenshotCapture(this.coordinator);
        
        console.log('🚀 DevTools API initialized');
    }

    // ========================================
    // SESSION MANAGEMENT API
    // ========================================

    /**
     * Request a DevTools session for specific purpose and AI persona
     * @param {string} purpose - Session purpose (git_verification, workspace, etc.)
     * @param {string} aiPersona - AI persona name (system, DataViz, etc.)
     * @param {Object} options - Session options (sharedWindow, windowTitle, etc.)
     * @returns {Object} Session information
     */
    async requestSession(purpose, aiPersona = 'system', options = {}) {
        const session = await this.coordinator.requestSession(purpose, aiPersona, {
            sharedWindow: true,
            windowTitle: 'Continuum DevTools',
            ...options
        });

        // Initialize protocol client for session
        if (!this.protocolClients.has(session.sessionId)) {
            const protocolClient = new DevToolsProtocolClient(session.port, session.sessionId);
            await protocolClient.connect();
            this.protocolClients.set(session.sessionId, protocolClient);
        }

        // Initialize console forwarder for session
        if (!this.consoleForwarders.has(session.sessionId)) {
            const consoleForwarder = new DevToolsConsoleForwarder(session.port, session.sessionId);
            await consoleForwarder.start();
            this.consoleForwarders.set(session.sessionId, consoleForwarder);
        }

        return session;
    }

    /**
     * Close specific session and cleanup resources
     */
    async closeSession(sessionKey) {
        const session = this.coordinator.activeSessions.get(sessionKey);
        if (!session) return;

        // Cleanup protocol client
        const protocolClient = this.protocolClients.get(session.sessionId);
        if (protocolClient) {
            await protocolClient.disconnect();
            this.protocolClients.delete(session.sessionId);
        }

        // Cleanup console forwarder
        const consoleForwarder = this.consoleForwarders.get(session.sessionId);
        if (consoleForwarder) {
            await consoleForwarder.stop();
            this.consoleForwarders.delete(session.sessionId);
        }

        // Close session
        await this.coordinator.closeSession(sessionKey);
    }

    /**
     * Get session status summary
     */
    getSessionSummary() {
        return this.coordinator.getSessionSummary();
    }

    // ========================================
    // TAB MANAGEMENT API
    // ========================================

    /**
     * Create new tab in existing session
     */
    async createTab(sessionId, url = 'http://localhost:9000') {
        const session = Array.from(this.coordinator.activeSessions.values())
            .find(s => s.sessionId === sessionId);
        
        if (!session) {
            throw new Error(`Session ${sessionId} not found`);
        }

        const protocolClient = this.protocolClients.get(sessionId);
        if (!protocolClient) {
            throw new Error(`Protocol client for session ${sessionId} not found`);
        }

        const tabInfo = await protocolClient.createTab(url);
        
        // Create tab manager
        const tabManager = new DevToolsTabManager(session.port, tabInfo.id);
        await tabManager.connect();
        this.activeTabs.set(tabInfo.id, tabManager);

        return { tabInfo, tabManager };
    }

    /**
     * Get tab manager for specific tab
     */
    getTabManager(tabId) {
        return this.activeTabs.get(tabId);
    }

    /**
     * Close specific tab
     */
    async closeTab(tabId) {
        const tabManager = this.activeTabs.get(tabId);
        if (tabManager) {
            await tabManager.close();
            this.activeTabs.delete(tabId);
        }
    }

    // ========================================
    // BROWSER AUTOMATION API
    // ========================================

    /**
     * Execute JavaScript in specific tab
     */
    async executeJavaScript(sessionId, script, tabId = null) {
        const protocolClient = this.protocolClients.get(sessionId);
        if (!protocolClient) {
            throw new Error(`Protocol client for session ${sessionId} not found`);
        }

        return await protocolClient.executeJavaScript(script, tabId);
    }

    /**
     * Take screenshot of specific session/tab
     */
    async takeScreenshot(sessionId, filename = null, options = {}) {
        const session = Array.from(this.coordinator.activeSessions.values())
            .find(s => s.sessionId === sessionId);
        
        if (!session) {
            throw new Error(`Session ${sessionId} not found`);
        }

        return await this.screenshotCapture.captureScreenshot(session, filename, options);
    }

    /**
     * Navigate tab to specific URL
     */
    async navigateTab(sessionId, url, tabId = null) {
        const protocolClient = this.protocolClients.get(sessionId);
        if (!protocolClient) {
            throw new Error(`Protocol client for session ${sessionId} not found`);
        }

        return await protocolClient.navigate(url, tabId);
    }

    // ========================================
    // CONSOLE MONITORING API
    // ========================================

    /**
     * Get console forwarder for session
     */
    getConsoleForwarder(sessionId) {
        return this.consoleForwarders.get(sessionId);
    }

    /**
     * Subscribe to console messages for session
     */
    onConsoleMessage(sessionId, callback) {
        const forwarder = this.consoleForwarders.get(sessionId);
        if (forwarder) {
            forwarder.onMessage(callback);
        }
    }

    /**
     * Get recent console messages for session
     */
    getRecentConsoleMessages(sessionId, count = 10) {
        const forwarder = this.consoleForwarders.get(sessionId);
        if (forwarder) {
            return forwarder.getRecentMessages(count);
        }
        return [];
    }

    // ========================================
    // PROTOCOL CLIENT API
    // ========================================

    /**
     * Get protocol client for direct DevTools Protocol access
     */
    getProtocolClient(sessionId) {
        return this.protocolClients.get(sessionId);
    }

    /**
     * Send raw DevTools Protocol command
     */
    async sendProtocolCommand(sessionId, method, params = {}, tabId = null) {
        const protocolClient = this.protocolClients.get(sessionId);
        if (!protocolClient) {
            throw new Error(`Protocol client for session ${sessionId} not found`);
        }

        return await protocolClient.sendCommand(method, params, tabId);
    }

    // ========================================
    // UTILITY AND CLEANUP API
    // ========================================

    /**
     * Emergency shutdown - close all sessions and cleanup
     */
    async emergencyShutdown() {
        console.log('🚨 DevTools API emergency shutdown...');

        // Stop all console forwarders
        for (const [sessionId, forwarder] of this.consoleForwarders.entries()) {
            await forwarder.stop();
        }
        this.consoleForwarders.clear();

        // Disconnect all protocol clients
        for (const [sessionId, client] of this.protocolClients.entries()) {
            await client.disconnect();
        }
        this.protocolClients.clear();

        // Close all tabs
        for (const [tabId, tabManager] of this.activeTabs.entries()) {
            await tabManager.close();
        }
        this.activeTabs.clear();

        // Emergency shutdown coordinator
        await this.coordinator.emergencyShutdown();

        console.log('✅ DevTools API emergency shutdown complete');
    }

    /**
     * Health check - verify all components are working
     */
    async healthCheck() {
        const health = {
            coordinator: false,
            activeSessions: 0,
            activeClients: 0,
            activeForwarders: 0,
            activeTabs: 0,
            errors: []
        };

        try {
            // Check coordinator
            health.coordinator = this.coordinator ? true : false;
            health.activeSessions = this.coordinator ? this.coordinator.activeSessions.size : 0;

            // Check protocol clients
            health.activeClients = this.protocolClients.size;
            for (const [sessionId, client] of this.protocolClients.entries()) {
                if (!client.isConnected()) {
                    health.errors.push(`Protocol client ${sessionId} disconnected`);
                }
            }

            // Check console forwarders
            health.activeForwarders = this.consoleForwarders.size;

            // Check tabs
            health.activeTabs = this.activeTabs.size;

        } catch (error) {
            health.errors.push(`Health check error: ${error.message}`);
        }

        return health;
    }
}

// Singleton instance
let devToolsAPIInstance = null;

/**
 * Get singleton DevTools API instance
 */
function getDevToolsAPI() {
    if (!devToolsAPIInstance) {
        devToolsAPIInstance = new DevToolsAPI();
    }
    return devToolsAPIInstance;
}

module.exports = {
    DevToolsAPI,
    getDevToolsAPI
};