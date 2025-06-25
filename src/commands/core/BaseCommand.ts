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
export abstract class BaseCommand {
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
  protected static parseParams<T = any>(params: any): T {
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
  protected static createSuccessResult<T = any>(
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
    params: any, 
    requiredFields: string[]
  ): { valid: boolean; missing: string[] } {
    const missing: string[] = [];
    
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
  protected static logExecution(
    commandName: string, 
    params: any, 
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
    message: any
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
    data?: any
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