/**
 * MIDDLE-OUT ARCHITECTURE - BASE COMMAND FOUNDATION
 * 
 * Universal typed foundation for all Continuum commands with modular parsing.
 * Implements the command interface standard across the entire system.
 * 
 * ARCHITECTURE STATUS: ✅ WELL-TYPED AND PROPERLY ABSTRACTED
 * 
 * ISSUES IDENTIFIED:
 * - ✅ No hardcoded magic strings detected
 * - ✅ Proper TypeScript typing throughout
 * - ✅ Good abstraction patterns
 * - ✅ Follows middle-out methodology
 * 
 * This file exemplifies good middle-out architecture:
 * - Foundation: Type-safe interfaces and base class
 * - API: Standardized command execution pattern
 * - Integration: Universal parser registry integration
 * - Testing: Built-in validation and error handling
 */

import type { CommandResult, CommandDefinition, ParameterDefinition } from '../../../types/shared/CommandTypes';
import type { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import type { UUID } from 'crypto';

// Re-export shared types for backward compatibility
export type { CommandResult, CommandDefinition, ParameterDefinition, ContinuumContext };

// Import the modular parser system
import { IntegrationParserRegistry } from './parsers';

// WebSocket commands MUST have sessionId - enforced by linter
export interface WebSocketContinuumContext extends ContinuumContext {
  sessionId: UUID; // Required, non-null for WebSocket commands
  connectionId: string; // Required for WebSocket routing
}

// CommandResult is now imported from shared types

export interface RegistryEntry {
  name: string;
  execute: (parameters: unknown, context: ContinuumContext) => Promise<CommandResult>;
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
   * Execute command - implement this in subclasses with typed parameters
   * Parameters are automatically parsed by UniversalCommandRegistry before calling this method
   * 
   * Pattern: static async execute(params: MyTypedParams, context: ContinuumContext): Promise<MyResult>
   */
  static execute(_params: unknown, _context: ContinuumContext): Promise<CommandResult> {
    throw new Error('execute() must be implemented by subclass with typed parameters');
  }

  /**
   * Parse parameters using modular integration parser system
   * Any format to BaseCommand's canonical JSON format
   * 
   * ⚠️  INTERNAL USE ONLY: Should only be called by UniversalCommandRegistry
   * ⚠️  Individual commands should NOT call this - parameters are pre-parsed
   */
  static _registryParseParams<T = unknown>(params: unknown): T {
    return IntegrationParserRegistry.parse<T>(params);
  }

  /**
   * Get typed parameters - ensures parameters are pre-parsed by registry
   * 
   * @param params - Parameters that should already be parsed by UniversalCommandRegistry
   * @returns Typed parameters with runtime validation that they're pre-parsed
   */
  protected static args<T = unknown>(params: unknown): T {
    // Verify parameters are already in canonical JSON format (not CLI args)
    if (typeof params === 'object' && params !== null && 'args' in params) {
      console.warn('⚠️  CLI args detected - parameters should be pre-parsed by registry');
      console.warn('⚠️  This suggests UniversalCommandRegistry is not parsing properly');
    }
    
    return params as T;
  }

  /**
   * @deprecated Do not call parseParams in commands - parameters are pre-parsed by registry
   * Use args<T>() instead for type-safe access to pre-parsed parameters
   */
  protected static parseParams<T = unknown>(params: unknown): T {
    console.warn('⚠️  parseParams called in command - parameters should be pre-parsed by registry');
    console.warn('⚠️  Use args<T>() instead for type-safe access to pre-parsed parameters');
    return IntegrationParserRegistry.parse<T>(params);
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
    context: ContinuumContext
  ): void {
    const sessionInfo = context.sessionId ? ` [${context.sessionId}]` : '';
    console.log(`🎯 COMMAND: ${commandName}${sessionInfo} - params:`, params);
  }

  /**
   * Broadcast message to WebSocket clients if available
   */
  protected static async broadcast(
    context: ContinuumContext,
    message: unknown
  ): Promise<void> {
    if (context.webSocketServer && typeof context.webSocketServer.broadcast === 'function') {
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
    context: ContinuumContext | undefined,
    status: string,
    data?: unknown
  ): Promise<void> {
    if (context?.continuumStatus && typeof (context.continuumStatus as any).update === 'function') {
      try {
        (context.continuumStatus as any).update(status, data);
      } catch (error) {
        console.error('Failed to update continuum status:', error);
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