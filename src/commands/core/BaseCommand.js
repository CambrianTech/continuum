/**
 * Base Command - TypeScript Implementation
 * Clean, typed foundation for all Continuum commands
 */
/**
 * Abstract base class for all Continuum commands
 * Provides type safety, consistent interfaces, and common functionality
 */
export class BaseCommand {
    /**
     * Parse parameters with type safety
     */
    static parseParams(params) {
        if (typeof params === 'string') {
            try {
                return JSON.parse(params);
            }
            catch (error) {
                console.warn(`Failed to parse JSON params: ${params}`, error);
                return params;
            }
        }
        return params;
    }
    /**
     * Create standardized success result
     */
    static createSuccessResult(message, data) {
        return {
            success: true,
            message,
            data,
            timestamp: new Date().toISOString()
        };
    }
    /**
     * Create standardized error result
     */
    static createErrorResult(message, error) {
        return {
            success: false,
            message,
            error,
            timestamp: new Date().toISOString()
        };
    }
    /**
     * Validate required parameters
     */
    static validateRequired(params, requiredFields) {
        const missing = [];
        for (const field of requiredFields) {
            if (params[field] === undefined || params[field] === null) {
                missing.push(field);
            }
        }
        return {
            valid: missing.length === 0,
            missing
        };
    }
    /**
     * Log command execution with consistent format
     */
    static logExecution(commandName, params, context) {
        const sessionInfo = context?.sessionId ? ` [${context.sessionId}]` : '';
        console.log(`🎯 COMMAND: ${commandName}${sessionInfo} - params:`, params);
    }
    /**
     * Broadcast message to WebSocket clients if available
     */
    static async broadcast(context, message) {
        if (context?.webSocketServer && typeof context.webSocketServer.broadcast === 'function') {
            try {
                const messageStr = typeof message === 'string' ? message : JSON.stringify(message);
                context.webSocketServer.broadcast(messageStr);
            }
            catch (error) {
                console.error('Failed to broadcast message:', error);
            }
        }
    }
    /**
     * Update continuon status if available
     */
    static async updateStatus(context, status, data) {
        if (context?.continuonStatus && typeof context.continuonStatus.update === 'function') {
            try {
                context.continuonStatus.update(status, data);
            }
            catch (error) {
                console.error('Failed to update continuon status:', error);
            }
        }
    }
    /**
     * Create command registry entry
     */
    static createRegistryEntry() {
        const definition = this.getDefinition();
        return {
            name: definition.name.toUpperCase(),
            execute: this.execute.bind(this),
            definition
        };
    }
}
//# sourceMappingURL=BaseCommand.js.map