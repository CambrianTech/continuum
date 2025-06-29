/**
 * Continuum API - Browser Client
 * Dynamic command discovery with console.debug tracking
 */
class ContinuumAPI {
    constructor() {
        this.ws = null;
        this.connected = false;
        this.messageHandlers = new Map();
        this.eventHandlers = new Map();
        this.commandRegistry = new Map();
        this.discoveredCommands = [];
        this.version = undefined;
        this.versionData = undefined;
    }
    async connect() {
        // Get version from server first
        try {
            const response = await fetch('/api/version');
            const versionData = await response.json();
            const timestamp = new Date().toLocaleTimeString();
            console.log(`🚀 Continuum Client v${versionData.version} (Build: ${versionData.build || 'unknown'}) - ${timestamp}`);
            console.log(`📦 Server: ${versionData.server || 'unknown'} | Environment: ${versionData.environment || 'development'}`);
            this.version = versionData.version;
            this.versionData = versionData;
            // Notify widgets of version update
            window.dispatchEvent(new CustomEvent('continuum:version-update', {
                detail: { version: versionData.version, data: versionData }
            }));
        }
        catch (error) {
            console.log('⚠️ Could not fetch version info:', error.message);
            console.log(`🕐 Client load time: ${new Date().toLocaleTimeString()}`);
        }
        return new Promise((resolve, reject) => {
            try {
                this.ws = new WebSocket('ws://localhost:9000');
                this.ws.onopen = () => {
                    this.connected = true;
                    console.log('✅ Continuum TypeScript API connected');
                    
                    // Initialize command discovery after connection
                    this.initializeCommandDiscovery();
                    
                    window.dispatchEvent(new CustomEvent('continuum-connected'));
                    resolve();
                };
                this.ws.onmessage = (event) => {
                    try {
                        const data = JSON.parse(event.data);
                        console.log('📨 Command response:', data);
                        // Emit events via clean API
                        if (data.timestamp) {
                            this.emit('message', data);
                        }
                        // Handle specific command responses by ID
                        const messageId = this.findMessageId(data);
                        if (messageId && this.messageHandlers.has(messageId)) {
                            const handler = this.messageHandlers.get(messageId);
                            this.messageHandlers.delete(messageId);
                            handler(data);
                        }
                    }
                    catch (error) {
                        console.error('❌ Message parsing error:', error);
                    }
                };
                this.ws.onerror = (error) => {
                    console.error('❌ WebSocket error:', error);
                    reject(error);
                };
                this.ws.onclose = () => {
                    this.connected = false;
                    console.log('🔌 Continuum disconnected - attempting reconnect');
                    setTimeout(() => this.connect(), 3000);
                };
            }
            catch (error) {
                console.error('❌ Connection failed:', error);
                reject(error);
            }
        });
    }
    /**
     * Execute any command with full type safety
     */
    async execute(command, params = {}) {
        if (!this.ws || !this.connected) {
            throw new Error('Continuum not connected');
        }
        const message = {
            type: 'execute_command',
            command,
            params,
            timestamp: new Date().toISOString(),
            id: `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`
        };
        return new Promise((resolve, reject) => {
            // Store callback for response
            this.messageHandlers.set(message.id, (response) => {
                if (response.success) {
                    resolve(response.data);
                }
                else {
                    reject(new Error(response.error || 'Command failed'));
                }
            });
            try {
                this.ws.send(JSON.stringify(message));
                console.log('🎯 Command executed:', command, params);
            }
            catch (error) {
                this.messageHandlers.delete(message.id);
                reject(error);
            }
        });
    }
    // Typed command methods
    async info(params = {}) {
        return this.execute('info', params);
    }
    async chat(message, room = 'general') {
        return this.execute('chat', { message, room });
    }
    async screenshot(params = {}) {
        return this.execute('screenshot', params);
    }
    async loadChatMessages(room = 'general') {
        return this.execute('load_chat_messages', { room });
    }
    
    async list(params = {}) {
        return this.execute('list', params);
    }
    // Event handling with type safety
    on(eventType, handler) {
        if (!this.eventHandlers.has(eventType)) {
            this.eventHandlers.set(eventType, []);
        }
        this.eventHandlers.get(eventType).push(handler);
    }
    off(eventType, handler) {
        if (this.eventHandlers.has(eventType)) {
            const handlers = this.eventHandlers.get(eventType);
            const index = handlers.indexOf(handler);
            if (index > -1) {
                handlers.splice(index, 1);
            }
        }
    }
    emit(eventType, data) {
        if (this.eventHandlers.has(eventType)) {
            const handlers = this.eventHandlers.get(eventType);
            handlers.forEach(handler => {
                try {
                    handler(data);
                }
                catch (error) {
                    console.error('❌ Event handler error:', error);
                }
            });
        }
    }
    isConnected() {
        return this.connected;
    }
    findMessageId(response) {
        // Try to extract message ID from response
        // This depends on how the server formats responses
        return response.id || null;
    }

    /**
     * Initialize command discovery system
     */
    async initializeCommandDiscovery() {
        try {
            console.debug('🔍 CONTINUUM: Starting command discovery...');
            
            // Register core client-side commands first
            this.registerCoreCommands();
            
            // Discover available commands from server
            const commandList = await this.list();
            
            console.debug('📋 CONTINUUM: Command discovery complete:', {
                totalCommands: commandList.totalCommands,
                bootstrapCommands: commandList.bootstrapCommands?.length || 0,
                discoveredCommands: commandList.discoveredCommands?.length || 0,
                systemReady: commandList.systemReady
            });
            
            // Register dynamic methods for discovered commands
            this.registerDynamicCommands(commandList.commands);
            
            // Emit command discovery complete event
            this.emit('commands-discovered', commandList);
            
        } catch (error) {
            console.debug('⚠️ CONTINUUM: Command discovery failed:', error);
        }
    }

    /**
     * Register core commands that are always available
     */
    registerCoreCommands() {
        const coreCommands = ['info', 'chat', 'screenshot', 'list', 'loadChatMessages'];
        
        coreCommands.forEach(command => {
            this.registerCommand(command, 'core');
        });
        
        console.debug('🏗️ CONTINUUM: Core commands registered:', coreCommands);
    }

    /**
     * Register dynamic commands discovered from server
     */
    registerDynamicCommands(commands) {
        const newCommands = [];
        
        commands.forEach(command => {
            if (!this.commandRegistry.has(command)) {
                this.registerCommand(command, 'discovered');
                newCommands.push(command);
            }
        });
        
        if (newCommands.length > 0) {
            console.debug('🔧 CONTINUUM: Dynamic commands registered:', newCommands);
        }
    }

    /**
     * Register a command and add it to the client API
     */
    registerCommand(commandName, category) {
        // Add to command registry
        this.commandRegistry.set(commandName, {
            name: commandName,
            category,
            registeredAt: new Date().toISOString()
        });
        
        // Add to discovered commands list
        if (!this.discoveredCommands.includes(commandName)) {
            this.discoveredCommands.push(commandName);
        }
        
        // Dynamically add method to continuum object if it doesn't exist
        if (!this[commandName] && !this.isReservedMethod(commandName)) {
            this[commandName] = async (params = {}) => {
                return this.execute(commandName, params);
            };
        }
        
        console.debug(`🎯 CONTINUUM: Registered continuum.${commandName}() method (${category})`);
    }

    /**
     * Check if method name conflicts with existing API methods
     */
    isReservedMethod(methodName) {
        const reserved = [
            'connect', 'execute', 'on', 'off', 'emit', 'isConnected',
            'constructor', 'toString', 'valueOf'
        ];
        return reserved.includes(methodName);
    }

    /**
     * Get current command registry state
     */
    getCommandRegistry() {
        return Array.from(this.commandRegistry.values());
    }

    /**
     * Get discovered commands list
     */
    getDiscoveredCommands() {
        return [...this.discoveredCommands];
    }
}
// Create and expose global continuum object
window.continuum = new ContinuumAPI();
// Auto-connect when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        window.continuum.connect();
    });
}
else {
    window.continuum.connect();
}
console.log('🚀 Continuum API initialized with dynamic command discovery');
