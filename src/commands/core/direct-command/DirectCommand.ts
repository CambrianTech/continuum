// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Converting to typed parameter execution pattern
/**
 * DirectCommand - Base class for commands with direct execution
 * 
 * Handles the common pattern of:
 * 1. Receive typed parameters (auto-parsed by registry)
 * 2. Execute single operation directly
 * 3. Standardized error handling and result formatting
 */

import { BaseCommand, CommandResult, ContinuumContext } from '../base-command/BaseCommand';

export abstract class DirectCommand extends BaseCommand {
  /**
   * Subclasses implement the core execution logic
   */
  protected static async executeOperation(_params: any, _context?: ContinuumContext): Promise<CommandResult> {
    throw new Error('executeOperation() must be implemented by subclass');
  }

  /**
   * Standard execute implementation with error handling
   */
  static async execute(params: any, context?: ContinuumContext): Promise<CommandResult> {
    try {
      // Parameters are automatically parsed by UniversalCommandRegistry
      return await this.executeOperation(params, context);
      
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