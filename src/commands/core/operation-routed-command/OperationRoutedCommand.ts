/**
 * OperationRoutedCommand - Base class for commands with operation-based routing
 * 
 * Handles the common pattern of:
 * 1. Extract operation from params (operation, _[0], or default)
 * 2. Route to specific handler method based on operation
 * 3. Standardized error handling and result formatting
 */

import { BaseCommand, CommandResult, CommandContext } from '../base-command/BaseCommand.js';

export interface OperationHandler {
  (data: any, context?: CommandContext): Promise<CommandResult>;
}

export interface OperationMap {
  [operation: string]: OperationHandler;
}

export abstract class OperationRoutedCommand extends BaseCommand {
  /**
   * Subclasses must define their operation handlers
   */
  protected static getOperationMap(): OperationMap {
    throw new Error('getOperationMap() must be implemented by subclass');
  }

  /**
   * Get the default operation when none specified
   */
  protected static getDefaultOperation(): string {
    return 'list';
  }

  /**
   * Extract operation from parameters using common patterns
   */
  protected static extractOperation(params: any): string {
    const parsedParams = this.parseParams(params) as any;
    
    // Try multiple common patterns for operation specification
    return parsedParams.operation || 
           parsedParams.action ||
           parsedParams._?.[0] || 
           this.getDefaultOperation();
  }

  /**
   * Standard execute implementation with operation routing
   */
  static async execute(params: any, context?: CommandContext): Promise<CommandResult> {
    try {
      const operation = this.extractOperation(params);
      const operationMap = this.getOperationMap();
      const handler = operationMap[operation];
      
      if (!handler) {
        const availableOps = Object.keys(operationMap).join(', ');
        return this.createErrorResult(
          `Unknown operation: ${operation}. Available operations: ${availableOps}`
        );
      }

      // Pass the full parsed params to the handler
      const parsedParams = this.parseParams(params) as any;
      return await handler.call(this, parsedParams, context);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Operation failed: ${errorMessage}`);
    }
  }

  /**
   * Helper method for subclasses to get supported operations
   */
  protected static getSupportedOperations(): string[] {
    return Object.keys(this.getOperationMap());
  }

  /**
   * Utility method to create operation examples for getDefinition()
   */
  protected static createOperationExamples(commandName: string, operations: string[]): Array<{description: string, command: string}> {
    return operations.map(op => ({
      description: `${op.charAt(0).toUpperCase() + op.slice(1)} operation`,
      command: `${commandName} --operation=${op}`
    }));
  }
}