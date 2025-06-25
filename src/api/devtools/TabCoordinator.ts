/**
 * Tab Coordinator - Centralized Tab and Session Management
 * ========================================================
 * 
 * The TabCoordinator is the central nervous system for managing all browser
 * tabs and sessions. It maintains a registry of tabs as first-class objects,
 * tracks their state, and provides coordination between different purposes
 * (git verification, AI workspaces, etc.) to prevent duplicate tabs.
 * 
 * ARCHITECTURE:
 * - TabCoordinator: Central registry and coordination logic
 * - Tab: First-class tab object with methods and state tracking
 * - SessionRegistry: Maps purposes/personas to tabs and sessions
 * - TabFactory: Creates and configures tabs for specific purposes
 */

import {
    IDevToolsSession,
    ITabInformation,
    SessionPurpose,
    SessionState,
    LogLevel,
    EventType,
    IResult,
    DevToolsError,
    SessionError,
    IEventEmitter
} from './types';

// ========================================
// TAB STATE AND EVENTS
// ========================================

export enum TabState {
    INITIALIZING = 'initializing',
    CONNECTING = 'connecting', 
    CONNECTED = 'connected',
    LOADING = 'loading',
    READY = 'ready',
    BUSY = 'busy',
    ERROR = 'error',
    CLOSING = 'closing',
    CLOSED = 'closed'
}

export enum TabEvent {
    STATE_CHANGED = 'tab:state_changed',
    URL_CHANGED = 'tab:url_changed',
    TITLE_CHANGED = 'tab:title_changed',
    CONSOLE_MESSAGE = 'tab:console_message',
    ERROR_OCCURRED = 'tab:error_occurred',
    READY = 'tab:ready'
}

export interface ITabMetadata {
    readonly id: string;
    readonly purpose: SessionPurpose;
    readonly aiPersona: string;
    readonly sessionId: string;
    readonly port: number;
    readonly created: Date;
    readonly lastActivity: Date;
    readonly isShared: boolean;
    readonly windowId?: string;
}

export interface ITabStatus {
    readonly state: TabState;
    readonly url?: string;
    readonly title?: string;
    readonly loadingProgress: number;
    readonly consoleMessages: number;
    readonly errorCount: number;
    readonly lastPing: Date;
    readonly isResponsive: boolean;
}

// ========================================
// TAB CLASS - FIRST-CLASS TAB OBJECT
// ========================================

export class Tab implements IEventEmitter {
    private metadata: ITabMetadata;
    private status: ITabStatus;
    private websocket: any = null;
    private eventListeners: Map<string, Set<Function>> = new Map();
    private messageId: number = 1;
    private pendingCommands: Map<number, any> = new Map();
    private healthCheckInterval: NodeJS.Timeout | null = null;

    constructor(tabInfo: ITabInformation, metadata: Partial<ITabMetadata>) {
        this.metadata = {
            id: tabInfo.id,
            purpose: metadata.purpose || SessionPurpose.DEVELOPMENT,
            aiPersona: metadata.aiPersona || 'system',
            sessionId: metadata.sessionId || '',
            port: metadata.port || 9222,
            created: new Date(),
            lastActivity: new Date(),
            isShared: metadata.isShared || false,
            windowId: metadata.windowId
        };

        this.status = {
            state: TabState.INITIALIZING,
            url: tabInfo.url,
            title: tabInfo.title,
            loadingProgress: 0,
            consoleMessages: 0,
            errorCount: 0,
            lastPing: new Date(),
            isResponsive: true
        };

        this.initializeTab(tabInfo);
    }

    // ========================================
    // EVENT SYSTEM
    // ========================================

    public on(event: string, listener: Function): void {
        if (!this.eventListeners.has(event)) {
            this.eventListeners.set(event, new Set());
        }
        this.eventListeners.get(event)!.add(listener);
    }

    public off(event: string, listener: Function): void {
        const listeners = this.eventListeners.get(event);
        if (listeners) {
            listeners.delete(listener);
        }
    }

    public emit(event: string, ...args: any[]): void {
        const listeners = this.eventListeners.get(event);
        if (listeners) {
            listeners.forEach(listener => {
                try {
                    listener(...args);
                } catch (error) {
                    console.error(`❌ Tab ${this.metadata.id} event listener error:`, error);
                }
            });
        }
    }

    // ========================================
    // TAB LIFECYCLE MANAGEMENT
    // ========================================

    private async initializeTab(tabInfo: ITabInformation): Promise<void> {
        try {
            this.setState(TabState.CONNECTING);
            
            // Connect to tab WebSocket
            await this.connectWebSocket(tabInfo.webSocketDebuggerUrl);
            
            // Setup domains and event handlers
            await this.setupTabDomains();
            
            this.setState(TabState.CONNECTED);
            this.startHealthMonitoring();
            
            console.log(`✅ Tab initialized: ${this.metadata.id} (${this.metadata.purpose}/${this.metadata.aiPersona})`);
        } catch (error) {
            this.setState(TabState.ERROR);
            this.status = { ...this.status, errorCount: this.status.errorCount + 1 };
            console.error(`❌ Tab initialization failed: ${this.metadata.id}`, error);
        }
    }

    private async connectWebSocket(webSocketUrl: string): Promise<void> {
        const { WebSocket } = require('ws');
        this.websocket = new WebSocket(webSocketUrl);

        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error('WebSocket connection timeout'));
            }, 10000);

            this.websocket.on('open', () => {
                clearTimeout(timeout);
                this.setupWebSocketHandlers();
                resolve();
            });

            this.websocket.on('error', (error: Error) => {
                clearTimeout(timeout);
                reject(error);
            });
        });
    }

    private setupWebSocketHandlers(): void {
        this.websocket.on('message', (data: any) => {
            try {
                const message = JSON.parse(data.toString());
                this.handleWebSocketMessage(message);
            } catch (error) {
                console.error(`❌ Tab ${this.metadata.id} message parse error:`, error);
            }
        });

        this.websocket.on('close', () => {
            this.setState(TabState.CLOSED);
            this.stopHealthMonitoring();
            console.log(`🔌 Tab WebSocket closed: ${this.metadata.id}`);
        });

        this.websocket.on('error', (error: Error) => {
            this.status = { ...this.status, errorCount: this.status.errorCount + 1 };
            this.emit(TabEvent.ERROR_OCCURRED, error);
            console.error(`❌ Tab WebSocket error: ${this.metadata.id}`, error);
        });
    }

    private handleWebSocketMessage(message: any): void {
        this.updateLastActivity();

        if (message.id && this.pendingCommands.has(message.id)) {
            // Handle command response
            const { resolve, reject } = this.pendingCommands.get(message.id)!;
            this.pendingCommands.delete(message.id);

            if (message.error) {
                reject(new Error(message.error.message));
            } else {
                resolve(message.result);
            }
        } else if (message.method) {
            // Handle event notification
            this.handleTabEvent(message);
        }
    }

    private handleTabEvent(message: any): void {
        switch (message.method) {
            case 'Page.frameNavigated':
                if (!message.params.frame.parentId) {
                    // Main frame navigation
                    this.status = { 
                        ...this.status, 
                        url: message.params.frame.url 
                    };
                    this.emit(TabEvent.URL_CHANGED, message.params.frame.url);
                }
                break;

            case 'Page.loadEventFired':
                this.setState(TabState.READY);
                this.emit(TabEvent.READY);
                break;

            case 'Runtime.consoleAPICalled':
                this.status = { 
                    ...this.status, 
                    consoleMessages: this.status.consoleMessages + 1 
                };
                this.emit(TabEvent.CONSOLE_MESSAGE, message.params);
                break;

            case 'Runtime.exceptionThrown':
                this.status = { 
                    ...this.status, 
                    errorCount: this.status.errorCount + 1 
                };
                this.emit(TabEvent.ERROR_OCCURRED, message.params);
                break;
        }
    }

    private async setupTabDomains(): Promise<void> {
        const domains = ['Page', 'Runtime', 'Console', 'DOM'];
        
        for (const domain of domains) {
            await this.sendCommand(`${domain}.enable`);
        }
    }

    // ========================================
    // TAB CONTROL METHODS
    // ========================================

    public async executeJavaScript(expression: string): Promise<any> {
        this.setState(TabState.BUSY);
        
        try {
            const result = await this.sendCommand('Runtime.evaluate', {
                expression: expression,
                returnByValue: true,
                awaitPromise: true
            });

            this.setState(TabState.READY);

            if (result.exceptionDetails) {
                throw new Error(`JavaScript execution error: ${result.exceptionDetails.text}`);
            }

            return result.result.value;
        } catch (error) {
            this.setState(TabState.ERROR);
            throw error;
        }
    }

    public async navigate(url: string): Promise<void> {
        this.setState(TabState.LOADING);
        
        try {
            await this.sendCommand('Page.navigate', { url });
            // State will be updated to READY via loadEventFired
        } catch (error) {
            this.setState(TabState.ERROR);
            throw error;
        }
    }

    public async takeScreenshot(options: any = {}): Promise<string> {
        try {
            const result = await this.sendCommand('Page.captureScreenshot', {
                format: 'png',
                quality: 90,
                ...options
            });
            return result.data;
        } catch (error) {
            throw new Error(`Screenshot failed: ${error.message}`);
        }
    }

    public async clickElement(selector: string): Promise<boolean> {
        return await this.executeJavaScript(`
            const element = document.querySelector('${selector}');
            if (element) {
                element.click();
                return true;
            }
            return false;
        `);
    }

    private async sendCommand(method: string, params: any = {}): Promise<any> {
        if (!this.websocket) {
            throw new Error(`Tab ${this.metadata.id} not connected`);
        }

        const messageId = this.messageId++;
        const command = {
            id: messageId,
            method: method,
            params: params
        };

        return new Promise((resolve, reject) => {
            this.pendingCommands.set(messageId, { resolve, reject });
            
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

    // ========================================
    // TAB STATE MANAGEMENT
    // ========================================

    private setState(newState: TabState): void {
        const oldState = this.status.state;
        this.status = { ...this.status, state: newState };
        
        if (oldState !== newState) {
            this.emit(TabEvent.STATE_CHANGED, { oldState, newState });
            console.log(`🔄 Tab ${this.metadata.id} state: ${oldState} → ${newState}`);
        }
    }

    private updateLastActivity(): void {
        this.metadata = {
            ...this.metadata,
            lastActivity: new Date()
        };
        this.status = { 
            ...this.status, 
            lastPing: new Date(),
            isResponsive: true 
        };
    }

    private startHealthMonitoring(): void {
        this.healthCheckInterval = setInterval(async () => {
            try {
                await this.sendCommand('Runtime.evaluate', {
                    expression: 'Date.now()',
                    returnByValue: true
                });
            } catch (error) {
                this.status = { 
                    ...this.status, 
                    isResponsive: false 
                };
                console.warn(`⚠️ Tab ${this.metadata.id} health check failed`);
            }
        }, 30000); // Every 30 seconds
    }

    private stopHealthMonitoring(): void {
        if (this.healthCheckInterval) {
            clearInterval(this.healthCheckInterval);
            this.healthCheckInterval = null;
        }
    }

    // ========================================
    // PUBLIC ACCESSORS
    // ========================================

    public getId(): string {
        return this.metadata.id;
    }

    public getMetadata(): ITabMetadata {
        return { ...this.metadata };
    }

    public getStatus(): ITabStatus {
        return { ...this.status };
    }

    public getPurpose(): SessionPurpose {
        return this.metadata.purpose;
    }

    public getAIPersona(): string {
        return this.metadata.aiPersona;
    }

    public getSessionId(): string {
        return this.metadata.sessionId;
    }

    public isReady(): boolean {
        return this.status.state === TabState.READY;
    }

    public isResponsive(): boolean {
        return this.status.isResponsive;
    }

    public async close(): Promise<void> {
        this.setState(TabState.CLOSING);
        this.stopHealthMonitoring();
        
        if (this.websocket) {
            this.websocket.close();
            this.websocket = null;
        }
        
        this.pendingCommands.clear();
        this.eventListeners.clear();
        this.setState(TabState.CLOSED);
    }
}

// ========================================
// TAB COORDINATOR - CENTRAL REGISTRY
// ========================================

export class TabCoordinator implements IEventEmitter {
    private static instance: TabCoordinator | null = null;
    private tabs: Map<string, Tab> = new Map(); // tabId -> Tab
    private sessionRegistry: Map<string, Set<string>> = new Map(); // sessionKey -> tabIds
    private purposeRegistry: Map<SessionPurpose, Set<string>> = new Map(); // purpose -> tabIds
    private personaRegistry: Map<string, Set<string>> = new Map(); // aiPersona -> tabIds
    private eventListeners: Map<string, Set<Function>> = new Map();

    private constructor() {
        console.log('🎯 TabCoordinator initialized');
    }

    public static getInstance(): TabCoordinator {
        if (!TabCoordinator.instance) {
            TabCoordinator.instance = new TabCoordinator();
        }
        return TabCoordinator.instance;
    }

    // ========================================
    // EVENT SYSTEM
    // ========================================

    public on(event: string, listener: Function): void {
        if (!this.eventListeners.has(event)) {
            this.eventListeners.set(event, new Set());
        }
        this.eventListeners.get(event)!.add(listener);
    }

    public off(event: string, listener: Function): void {
        const listeners = this.eventListeners.get(event);
        if (listeners) {
            listeners.delete(listener);
        }
    }

    public emit(event: string, ...args: any[]): void {
        const listeners = this.eventListeners.get(event);
        if (listeners) {
            listeners.forEach(listener => {
                try {
                    listener(...args);
                } catch (error) {
                    console.error(`❌ TabCoordinator event listener error:`, error);
                }
            });
        }
    }

    // ========================================
    // TAB COORDINATION LOGIC
    // ========================================

    /**
     * Find existing tab for purpose/persona combination
     * This is the key method that prevents duplicate tabs
     */
    public findExistingTab(purpose: SessionPurpose, aiPersona: string): Tab | null {
        const sessionKey = this.generateSessionKey(purpose, aiPersona);
        const tabIds = this.sessionRegistry.get(sessionKey);
        
        if (tabIds && tabIds.size > 0) {
            // Return the first ready tab for this purpose/persona
            for (const tabId of Array.from(tabIds)) {
                const tab = this.tabs.get(tabId);
                if (tab && tab.isResponsive() && tab.getStatus().state !== TabState.CLOSED) {
                    console.log(`🔄 Reusing existing tab: ${tabId} for ${purpose}/${aiPersona}`);
                    return tab;
                }
            }
        }
        
        return null;
    }

    /**
     * Register a new tab with the coordinator
     */
    public async registerTab(tabInfo: ITabInformation, metadata: Partial<ITabMetadata>): Promise<Tab> {
        // Check if we already have a suitable tab
        const existingTab = this.findExistingTab(
            metadata.purpose || SessionPurpose.DEVELOPMENT,
            metadata.aiPersona || 'system'
        );
        
        if (existingTab) {
            console.log(`✅ Using existing tab instead of creating new one`);
            return existingTab;
        }

        // Create new tab
        const tab = new Tab(tabInfo, metadata);
        
        // Register tab in all registries
        this.tabs.set(tab.getId(), tab);
        
        const sessionKey = this.generateSessionKey(tab.getPurpose(), tab.getAIPersona());
        if (!this.sessionRegistry.has(sessionKey)) {
            this.sessionRegistry.set(sessionKey, new Set());
        }
        this.sessionRegistry.get(sessionKey)!.add(tab.getId());
        
        if (!this.purposeRegistry.has(tab.getPurpose())) {
            this.purposeRegistry.set(tab.getPurpose(), new Set());
        }
        this.purposeRegistry.get(tab.getPurpose())!.add(tab.getId());
        
        if (!this.personaRegistry.has(tab.getAIPersona())) {
            this.personaRegistry.set(tab.getAIPersona(), new Set());
        }
        this.personaRegistry.get(tab.getAIPersona())!.add(tab.getId());

        // Set up tab event forwarding
        this.setupTabEventForwarding(tab);

        console.log(`📝 Registered new tab: ${tab.getId()} (${tab.getPurpose()}/${tab.getAIPersona()})`);
        this.emit(EventType.TAB_CREATED, tab);
        
        return tab;
    }

    private setupTabEventForwarding(tab: Tab): void {
        tab.on(TabEvent.STATE_CHANGED, (stateChange: any) => {
            console.log(`🔄 Tab ${tab.getId()} state changed: ${stateChange.oldState} → ${stateChange.newState}`);
        });

        tab.on(TabEvent.READY, () => {
            console.log(`✅ Tab ${tab.getId()} is ready`);
        });

        tab.on(TabEvent.ERROR_OCCURRED, (error: any) => {
            console.log(`❌ Tab ${tab.getId()} error: ${error}`);
        });
    }

    /**
     * Get tab by ID
     */
    public getTab(tabId: string): Tab | undefined {
        return this.tabs.get(tabId);
    }

    /**
     * Get all tabs for a specific purpose
     */
    public getTabsForPurpose(purpose: SessionPurpose): Tab[] {
        const tabIds = this.purposeRegistry.get(purpose) || new Set();
        return Array.from(tabIds)
            .map(id => this.tabs.get(id))
            .filter((tab): tab is Tab => tab !== undefined);
    }

    /**
     * Get all tabs for a specific AI persona
     */
    public getTabsForPersona(aiPersona: string): Tab[] {
        const tabIds = this.personaRegistry.get(aiPersona) || new Set();
        return Array.from(tabIds)
            .map(id => this.tabs.get(id))
            .filter((tab): tab is Tab => tab !== undefined);
    }

    /**
     * Get coordinator summary
     */
    public getSummary(): any {
        const activeTabs = Array.from(this.tabs.values())
            .filter(tab => tab.getStatus().state !== TabState.CLOSED);

        return {
            totalTabs: this.tabs.size,
            activeTabs: activeTabs.length,
            byPurpose: Object.fromEntries(
                Array.from(this.purposeRegistry.entries())
                    .map(([purpose, tabIds]) => [
                        purpose, 
                        Array.from(tabIds).filter(id => {
                            const tab = this.tabs.get(id);
                            return tab && tab.getStatus().state !== TabState.CLOSED;
                        }).length
                    ])
            ),
            byPersona: Object.fromEntries(
                Array.from(this.personaRegistry.entries())
                    .map(([persona, tabIds]) => [
                        persona,
                        Array.from(tabIds).filter(id => {
                            const tab = this.tabs.get(id);
                            return tab && tab.getStatus().state !== TabState.CLOSED;
                        }).length
                    ])
            ),
            tabs: activeTabs.map(tab => ({
                id: tab.getId(),
                purpose: tab.getPurpose(),
                persona: tab.getAIPersona(),
                state: tab.getStatus().state,
                url: tab.getStatus().url,
                isResponsive: tab.isResponsive()
            }))
        };
    }

    /**
     * Close tab and cleanup registries
     */
    public async closeTab(tabId: string): Promise<void> {
        const tab = this.tabs.get(tabId);
        if (!tab) return;

        // Close the tab
        await tab.close();

        // Remove from registries
        this.tabs.delete(tabId);
        
        const sessionKey = this.generateSessionKey(tab.getPurpose(), tab.getAIPersona());
        const sessionTabs = this.sessionRegistry.get(sessionKey);
        if (sessionTabs) {
            sessionTabs.delete(tabId);
            if (sessionTabs.size === 0) {
                this.sessionRegistry.delete(sessionKey);
            }
        }

        const purposeTabs = this.purposeRegistry.get(tab.getPurpose());
        if (purposeTabs) {
            purposeTabs.delete(tabId);
            if (purposeTabs.size === 0) {
                this.purposeRegistry.delete(tab.getPurpose());
            }
        }

        const personaTabs = this.personaRegistry.get(tab.getAIPersona());
        if (personaTabs) {
            personaTabs.delete(tabId);
            if (personaTabs.size === 0) {
                this.personaRegistry.delete(tab.getAIPersona());
            }
        }

        console.log(`🗑️ Closed and removed tab: ${tabId}`);
        this.emit(EventType.TAB_CLOSED, tabId);
    }

    /**
     * Emergency shutdown - close all tabs
     */
    public async emergencyShutdown(): Promise<void> {
        console.log('🚨 TabCoordinator emergency shutdown...');
        
        const closeTasks = Array.from(this.tabs.values()).map(tab => tab.close());
        await Promise.all(closeTasks);
        
        this.tabs.clear();
        this.sessionRegistry.clear();
        this.purposeRegistry.clear();
        this.personaRegistry.clear();
        this.eventListeners.clear();
        
        console.log('✅ TabCoordinator emergency shutdown complete');
    }

    private generateSessionKey(purpose: SessionPurpose, aiPersona: string): string {
        return `${purpose}_${aiPersona}`;
    }
}

/**
 * Factory function to get tab coordinator
 */
export function getTabCoordinator(): TabCoordinator {
    return TabCoordinator.getInstance();
}

export default TabCoordinator;