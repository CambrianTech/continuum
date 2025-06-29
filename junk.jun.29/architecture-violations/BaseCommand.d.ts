/**
 * Base Command - TypeScript Implementation
 * Clean, typed foundation for all Continuum commands
 */
export interface CommandDefinition {
    name: string;
    category: string;
    icon: string;
    description: string;
    params: string;
    examples: string[];
    usage: string;
}
export interface CommandContext {
    continuum?: any;
    webSocketServer?: any;
    continuonStatus?: any;
    sessionId?: string;
    userId?: string;
    [key: string]: any;
}
export interface CommandResult<T = any> {
    success: boolean;
    message: string;
    data?: T;
    error?: string;
    timestamp?: string;
}
/**
 * Abstract base class for all Continuum commands
 * Provides type safety, consistent interfaces, and common functionality
 */
export declare abstract class BaseCommand {
    /**
     * Get command definition - must be implemented by subclasses
     */
    static abstract getDefinition(): CommandDefinition;
    /**
     * Execute command - must be implemented by subclasses
     */
    static abstract execute(params: any, context?: CommandContext): Promise<CommandResult>;
    /**
     * Parse parameters with type safety
     */
    protected static parseParams<T = any>(params: any): T;
    /**
     * Create standardized success result
     */
    protected static createSuccessResult<T = any>(message: string, data?: T): CommandResult<T>;
    /**
     * Create standardized error result
     */
    protected static createErrorResult(message: string, error?: string): CommandResult;
    /**
     * Validate required parameters
     */
    protected static validateRequired(params: any, requiredFields: string[]): {
        valid: boolean;
        missing: string[];
    };
    /**
     * Log command execution with consistent format
     */
    protected static logExecution(commandName: string, params: any, context?: CommandContext): void;
    /**
     * Broadcast message to WebSocket clients if available
     */
    protected static broadcast(context: CommandContext | undefined, message: any): Promise<void>;
    /**
     * Update continuon status if available
     */
    protected static updateStatus(context: CommandContext | undefined, status: string, data?: any): Promise<void>;
    /**
     * Create command registry entry
     */
    static createRegistryEntry(): {
        name: string;
        execute: typeof BaseCommand.execute;
        definition: CommandDefinition;
    };
}
//# sourceMappingURL=BaseCommand.d.ts.map