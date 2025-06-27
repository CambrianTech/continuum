/**
 * Dynamic Message Router - Routes WebSocket messages to daemon capabilities dynamically
 * No hardcoded handlers - discovers and routes based on daemon capabilities
 */
import { EventEmitter } from 'events';
export class DynamicMessageRouter extends EventEmitter {
    constructor() {
        super();
        this.registeredDaemons = new Map();
        this.routerVersion = '1.0.0';
        // Load version asynchronously, don't block constructor
        this.loadCurrentVersionSync().catch(error => {
            console.warn(`⚠️ Failed to load version in constructor: ${error.message}`);
        });
    }
    async loadCurrentVersionSync() {
        try {
            // Use ES module imports
            const { readFileSync } = await import('fs');
            const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
            this.routerVersion = packageData.version;
            console.log(`📦 DynamicMessageRouter loaded version: ${this.routerVersion}`);
        }
        catch (error) {
            console.warn(`⚠️ DynamicMessageRouter failed to load version: ${error.message}`);
            this.routerVersion = '0.2.UNKNOWN';
        }
    }
    /**
     * Register a daemon and discover its capabilities automatically
     */
    async registerDaemon(name, daemon) {
        console.log(`🔌 Registering daemon: ${name}`);
        try {
            // Get daemon capabilities
            const capResponse = await daemon.handleMessage({ type: 'get_capabilities', data: {} });
            const capabilities = capResponse.success ? (capResponse.data.capabilities || []) : [];
            // Discover supported message types by introspection
            const messageTypes = this.discoverMessageTypes(daemon);
            this.registeredDaemons.set(name, {
                name,
                daemon,
                capabilities,
                messageTypes
            });
            console.log(`✅ Registered ${name} with capabilities: ${capabilities.join(', ')}`);
            console.log(`📝 Message types: ${messageTypes.join(', ')}`);
        }
        catch (error) {
            console.warn(`⚠️ Failed to register daemon ${name}:`, error.message);
        }
    }
    /**
     * Route message to appropriate daemon based on message type
     */
    async routeMessage(message, clientId, daemonConnector) {
        const messageType = message.type;
        // Find daemon that can handle this message type
        const targetDaemon = this.findHandlerDaemon(messageType);
        if (!targetDaemon) {
            console.log(`🔄 No daemon found for message type: ${messageType}`);
            return {
                type: 'error',
                data: {
                    error: `No daemon registered for message type: ${messageType}`,
                    availableTypes: this.getAllMessageTypes(),
                    routerVersion: this.routerVersion,
                    component: 'DynamicMessageRouter'
                },
                timestamp: new Date().toISOString(),
                clientId
            };
        }
        try {
            console.log(`📨 Routing ${messageType} to daemon: ${targetDaemon.name}`);
            const result = await targetDaemon.daemon.handleMessage({
                type: messageType,
                data: message.data
            });
            return {
                type: `${messageType}_response`,
                data: result.data,
                timestamp: new Date().toISOString(),
                clientId,
                requestId: message.requestId,
                processedBy: targetDaemon.name
            };
        }
        catch (error) {
            console.error(`❌ Handler error for ${messageType} in ${targetDaemon.name}:`, error);
            return {
                type: 'error',
                data: {
                    error: error.message,
                    daemon: targetDaemon.name,
                    messageType,
                    routerVersion: this.routerVersion,
                    component: 'DynamicMessageRouter'
                },
                timestamp: new Date().toISOString(),
                clientId,
                requestId: message.requestId
            };
        }
    }
    /**
     * Find which daemon can handle a specific message type (prefer specific capabilities)
     */
    findHandlerDaemon(messageType) {
        // First, look for daemons with specific capabilities for this message
        const capabilityMap = {
            'get_capabilities': ['basic-rendering', 'legacy-ui', 'modern-ui'],
            'render_request': ['basic-rendering', 'legacy-ui', 'modern-ui'],
            'switch_engine': ['basic-rendering', 'legacy-ui', 'modern-ui'],
            'tabRegister': ['websocket-server', 'client-management'],
            'get_component_css': ['websocket-server', 'css-service']
        };
        const preferredCapabilities = capabilityMap[messageType];
        if (preferredCapabilities) {
            for (const daemon of this.registeredDaemons.values()) {
                const hasCapability = daemon.capabilities.some(cap => preferredCapabilities.includes(cap));
                if (hasCapability && daemon.messageTypes.includes(messageType)) {
                    console.log(`🎯 Found specialized daemon ${daemon.name} for ${messageType}`);
                    return daemon;
                }
            }
        }
        // Fallback: find any daemon that supports the message type
        for (const daemon of this.registeredDaemons.values()) {
            if (daemon.messageTypes.includes(messageType)) {
                console.log(`🔄 Using fallback daemon ${daemon.name} for ${messageType}`);
                return daemon;
            }
        }
        return null;
    }
    /**
     * Discover what message types a daemon supports by introspection
     */
    discoverMessageTypes(daemon) {
        const messageTypes = [];
        // FIRST: Check if daemon has getMessageTypes() method (CRITICAL FIX)
        if (typeof daemon.getMessageTypes === 'function') {
            try {
                const daemonSpecificTypes = daemon.getMessageTypes();
                if (Array.isArray(daemonSpecificTypes)) {
                    messageTypes.push(...daemonSpecificTypes);
                    console.log(`✅ Discovered daemon-specific message types: ${daemonSpecificTypes.join(', ')}`);
                }
            }
            catch (error) {
                console.warn(`⚠️ Failed to get daemon-specific message types:`, error.message);
            }
        }
        // SECOND: Add common daemon message types
        const commonTypes = [
            'ping', 'pong', // Essential for chat communication
            'get_stats', 'get_clients', 'get_capabilities',
            'render_request', 'switch_engine',
            'execute_command', 'broadcast_message', 'send_message',
            'tabRegister', 'get_component_css'
        ];
        // Add common types if daemon has handleMessage
        if (typeof daemon.handleMessage === 'function') {
            for (const type of commonTypes) {
                if (!messageTypes.includes(type)) {
                    messageTypes.push(type);
                }
            }
        }
        console.log(`📝 Total discovered message types for daemon: ${messageTypes.join(', ')}`);
        return messageTypes;
    }
    /**
     * Get all supported message types across all daemons
     */
    getAllMessageTypes() {
        const types = new Set();
        for (const daemon of this.registeredDaemons.values()) {
            daemon.messageTypes.forEach(type => types.add(type));
        }
        return Array.from(types);
    }
    /**
     * Get system status showing all registered daemons
     */
    getSystemStatus() {
        return {
            registeredDaemons: this.registeredDaemons.size,
            daemons: Array.from(this.registeredDaemons.values()).map(d => ({
                name: d.name,
                capabilities: d.capabilities,
                messageTypes: d.messageTypes
            })),
            totalMessageTypes: this.getAllMessageTypes().length
        };
    }
}
