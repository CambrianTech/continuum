/**
 * DirectCommand - Base class for commands with direct execution
 * 
 * Handles the common pattern of:
 * 1. Parse parameters with type safety
 * 2. Execute single operation directly
 * 3. Standardized error handling and result formatting
 */

import { BaseCommand, CommandResult, CommandContext } from '../base-command/BaseCommand';

export abstract class DirectCommand extends BaseCommand {
  /**
   * Subclasses implement the core execution logic
   */
  protected static async executeOperation(_params: any, _context?: CommandContext): Promise<CommandResult> {
    throw new Error('executeOperation() must be implemented by subclass');
  }

  /**
   * Standard execute implementation with error handling
   */
  static async execute(params: any, context?: CommandContext): Promise<CommandResult> {
    try {
      const parsedParams = this.parseParams(params);
      return await this.executeOperation(parsedParams, context);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Command execution failed: ${errorMessage}`);
    }
  }

  /**
   * Helper method to validate required parameters
   */
  protected static validateRequiredParams(params: any, requiredFields: string[]): void {
    const missing = requiredFields.filter(field => {
      const value = params[field];
      return value === undefined || value === null || value === '';
    });

    if (missing.length > 0) {
      throw new Error(`Missing required parameters: ${missing.join(', ')}`);
    }
  }

  /**
   * Helper method to apply default values to parameters
   */
  protected static applyDefaults<T>(params: any, defaults: Partial<T>): T {
    return { ...defaults, ...params } as T;
  }
}