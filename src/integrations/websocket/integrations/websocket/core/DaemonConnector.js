/**
 * Daemon Connector - Simple connection to existing TypeScript command system
 */
import { EventEmitter } from 'events';
export class DaemonConnector extends EventEmitter {
    constructor(config = {}) {
        super();
        this.commandProcessor = null;
        this.config = {
            autoConnect: config.autoConnect ?? true,
            enableFallback: config.enableFallback ?? false,
            retryAttempts: config.retryAttempts ?? 3,
            retryInterval: config.retryInterval ?? 5000
        };
        this.connection = {
            connected: false,
            connectionAttempts: 0
        };
    }
    async connect() {
        console.log('🔌 Connecting to TypeScript command system...');
        try {
            // Import the existing SelfTestCommand directly (proven working)
            const { SelfTestCommand } = await import('../../commands/development/selftest/SelfTestCommand');
            // Create a simple command processor that uses existing TypeScript commands
            this.commandProcessor = {
                initialized: true,
                executeCommand: async (command, params, context) => {
                    console.log(`🚀 Routing ${command} to TypeScript command system`);
                    switch (command) {
                        case 'selftest':
                            const result = await SelfTestCommand.execute(params, context);
                            return {
                                ...result,
                                processor: 'typescript-command-system'
                            };
                        default:
                            return {
                                success: false,
                                error: `Command ${command} not found in TypeScript system`,
                                processor: 'typescript-command-system'
                            };
                    }
                },
                getCommands: () => ['selftest'], // Expandable as we add more TypeScript commands
                getDefinition: (command) => {
                    if (command === 'selftest') {
                        return SelfTestCommand.getDefinition();
                    }
                    return null;
                }
            };
            this.connection = {
                connected: true,
                commandProcessor: this.commandProcessor,
                lastConnectAttempt: new Date(),
                connectionAttempts: this.connection.connectionAttempts + 1
            };
            console.log('✅ Connected to TypeScript command system');
            this.emit('connected');
            return true;
        }
        catch (error) {
            console.error('❌ Failed to connect to TypeScript command system:', error);
            this.connection.connectionAttempts++;
            this.emit('error', error);
            return false;
        }
    }
    async disconnect() {
        if (this.connection.connected) {
            this.connection.connected = false;
            this.commandProcessor = null;
            console.log('🔌 Disconnected from TypeScript command system');
            this.emit('disconnected');
        }
    }
    isConnected() {
        return this.connection.connected;
    }
    async executeCommand(command, params, context) {
        if (!this.connection.connected || !this.commandProcessor) {
            return {
                success: false,
                error: 'Not connected to TypeScript command system',
                processor: 'daemon-connector-disconnected'
            };
        }
        try {
            return await this.commandProcessor.executeCommand(command, params, context);
        }
        catch (error) {
            return {
                success: false,
                error: error.message,
                processor: 'daemon-connector-error'
            };
        }
    }
    getAvailableCommands() {
        if (!this.connection.connected || !this.commandProcessor) {
            return [];
        }
        return this.commandProcessor.getCommands();
    }
    getCommandDefinition(command) {
        if (!this.connection.connected || !this.commandProcessor) {
            return null;
        }
        return this.commandProcessor.getDefinition(command);
    }
}
