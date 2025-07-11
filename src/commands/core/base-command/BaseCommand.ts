/**
 * Base Command - TypeScript Implementation
 * Clean, typed foundation for all Continuum commands
 */

import { CommandResult, CommandDefinition, ParameterDefinition } from '../../../types/shared/CommandTypes';

// Re-export shared types for backward compatibility
export type { CommandResult, CommandDefinition, ParameterDefinition };

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

// WebSocket commands MUST have sessionId - enforced by linter
export interface WebSocketCommandContext extends CommandContext {
  sessionId: string; // Required, non-null for WebSocket commands
  connectionId: string; // Required for WebSocket routing
}

// CommandResult is now imported from shared types

export interface RegistryEntry {
  name: string;
  execute: (context: CommandContext, parameters: Record<string, unknown>) => Promise<CommandResult>;
  definition: CommandDefinition;
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
   * Legacy signature: createSuccessResult(message, data) or createSuccessResult(data)
   */
  protected static createSuccessResult<T = unknown>(
    messageOrData?: string | T,
    data?: T
  ): CommandResult<T> {
    const result: CommandResult<T> = {
      success: true,
      timestamp: new Date().toISOString()
    };
    
    // Handle legacy signature: createSuccessResult(message, data)
    if (typeof messageOrData === 'string' && data !== undefined) {
      result.data = data;
      // Legacy message parameter is ignored - data takes precedence
    } else if (messageOrData !== undefined) {
      // New signature: createSuccessResult(data)
      result.data = messageOrData as T;
    }
    
    return result;
  }

  /**
   * Create standardized error result
   */
  protected static createErrorResult(
    error: string,
    _data?: unknown  // Legacy parameter - maintained for backward compatibility
  ): CommandResult {
    return {
      success: false,
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
  static createRegistryEntry(): RegistryEntry {
    const definition = this.getDefinition();
    return {
      name: definition.name.toUpperCase(),
      execute: this.execute.bind(this),
      definition
    };
  }
}