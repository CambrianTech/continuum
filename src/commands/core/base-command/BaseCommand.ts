/**
 * Base Command - TypeScript Implementation
 * Clean, typed foundation for all Continuum commands
 */

export interface ParameterDefinition {
  type: 'string' | 'number' | 'boolean' | 'array' | 'object';
  description: string;
  required?: boolean;
  default?: unknown;
}

export interface CommandDefinition {
  name: string;
  category: string;
  icon?: string;
  description: string;
  parameters: Record<string, ParameterDefinition>;
  examples: Array<{
    description: string;
    command: string;
  }>;
  usage?: string;
}

// Common types for context properties
export interface WebSocketServer {
  send: (message: unknown) => void;
  broadcast: (message: unknown) => void;
  clients: Set<unknown>;
}

export interface ContinuumInstance {
  version: string;
  config: Record<string, unknown>;
  daemons: Map<string, unknown>;
}

export interface CommandContext {
  continuum?: ContinuumInstance;
  webSocketServer?: WebSocketServer;
  continuonStatus?: Record<string, unknown>;
  sessionId?: string;
  userId?: string;
  [key: string]: unknown;
}

export interface CommandResult<T = unknown> {
  success: boolean;
  message?: string | undefined;
  data?: T | undefined;
  error?: string | undefined;
  timestamp?: string | undefined;
}

/**
 * Abstract base class for all Continuum commands
 * Provides type safety, consistent interfaces, and common functionality
 */
export abstract class BaseCommand {
  /**
   * Get command definition - must be implemented by subclasses
   */
  static getDefinition(): CommandDefinition {
    throw new Error('getDefinition() must be implemented by subclass');
  }

  /**
   * Execute command - must be implemented by subclasses
   */
  static execute(_params: unknown, _context?: CommandContext): Promise<CommandResult> {
    throw new Error('execute() must be implemented by subclass');
  }

  /**
   * Parse parameters with type safety
   */
  protected static parseParams<T = unknown>(params: unknown): T {
    if (typeof params === 'string') {
      try {
        return JSON.parse(params) as T;
      } catch (error) {
        console.warn(`Failed to parse JSON params: ${params}`, error);
        return params as T;
      }
    }
    return params as T;
  }

  /**
   * Create standardized success result
   */
  protected static createSuccessResult<T = unknown>(
    message: string, 
    data?: T
  ): CommandResult<T> {
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
  protected static createErrorResult(
    message: string, 
    error?: string
  ): CommandResult {
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
  protected static validateRequired(
    params: unknown, 
    requiredFields: string[]
  ): { valid: boolean; missing: string[] } {
    const missing: string[] = [];
    
    for (const field of requiredFields) {
      if ((params as Record<string, unknown>)[field] === undefined || (params as Record<string, unknown>)[field] === null) {
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
  protected static logExecution(
    commandName: string, 
    params: unknown, 
    context?: CommandContext
  ): void {
    const sessionInfo = context?.sessionId ? ` [${context.sessionId}]` : '';
    console.log(`🎯 COMMAND: ${commandName}${sessionInfo} - params:`, params);
  }

  /**
   * Broadcast message to WebSocket clients if available
   */
  protected static async broadcast(
    context: CommandContext | undefined,
    message: unknown
  ): Promise<void> {
    if (context?.webSocketServer && typeof context.webSocketServer.broadcast === 'function') {
      try {
        const messageStr = typeof message === 'string' ? message : JSON.stringify(message);
        context.webSocketServer.broadcast(messageStr);
      } catch (error) {
        console.error('Failed to broadcast message:', error);
      }
    }
  }

  /**
   * Update continuon status if available
   */
  protected static async updateStatus(
    context: CommandContext | undefined,
    status: string,
    data?: unknown
  ): Promise<void> {
    if (context?.continuonStatus && typeof context.continuonStatus.update === 'function') {
      try {
        context.continuonStatus.update(status, data);
      } catch (error) {
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