/**
 * Exec Command - Server Implementation
 * 
 * Strongly typed command execution interface for server context.
 */

import { BaseCommand, CommandResult } from '../../core/base-command/BaseCommand';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  CommandExecution,
  CommandExecutionFactory,
  isCommandExecution
} from '../../../types/shared/CommandTypes';
// Simplified exec command - no external dependencies for now
// import { getGlobalCommandRegistry } from '../../../services/UniversalCommandRegistry';
// import { ExecParameters, ExecResult, ExecMetadata, validateExecParameters } from './shared/ExecTypes';

// Temporary simple types
interface ExecParameters {
  command?: string;
  args?: string[];
  execution?: CommandExecution;
}

function validateExecParameters(params: any): params is ExecParameters {
  // Must be non-null object
  if (typeof params !== 'object' || params === null) {
    throw new Error('ExecParameters must be a non-null object');
  }
  
  // Must have either command, execution object, or args
  if (!params.command && !params.execution && !params.args) {
    throw new Error('ExecParameters must have either "command", "execution", or "args" property');
  }
  
  // Validate execution object if present
  if (params.execution && !isCommandExecution(params.execution)) {
    throw new Error('ExecParameters.execution must be a valid CommandExecution object');
  }
  
  // Validate command type if present
  if (params.command && typeof params.command !== 'string') {
    throw new Error('ExecParameters.command must be a string');
  }
  
  // Validate args type if present
  if (params.args && !Array.isArray(params.args)) {
    throw new Error('ExecParameters.args must be an array');
  }
  
  return true;
}

export class ExecCommand extends BaseCommand {

  static definition = {
    name: 'exec',
    category: 'core' as const,
    description: 'Execute commands using strongly typed interface',
    parameters: {
      command: {
        type: 'string' as const,
        description: 'Command name to execute',
        required: false
      },
      args: {
        type: 'array' as const,
        description: 'Arguments for the command',
        required: false
      },
      execution: {
        type: 'object' as const,
        description: 'Complete CommandExecution object',
        required: false
      }
    },
    examples: [
      {
        description: 'Execute js command',
        command: JSON.stringify({
          command: 'js',
          args: ['--script=console.log("hello from exec")']
        })
      },
      {
        description: 'Execute help command',
        command: JSON.stringify({
          command: 'help'
        })
      }
    ]
  } as const;

  static async execute(parameters: unknown, context: ContinuumContext): Promise<CommandResult> {
    // Generic strongly typed pattern - eliminates 'any' parameters
    try {
      // Step 1: Basic type checking
      if (typeof parameters !== 'object' || parameters === null) {
        throw new Error('Parameters must be a non-null object');
      }
      
      // Step 2: Parse CLI arguments if present (for backward compatibility)
      const parsedParams = ExecCommand.parseCliArguments(parameters);
      
      // Step 3: Validate with custom type guard
      validateExecParameters(parsedParams);
      const typedParams = parsedParams as ExecParameters;
      
      // Step 4: Execute with strongly typed parameters
      return await ExecCommand.executeTyped(typedParams, context);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Parse CLI arguments to extract typed parameters (reusable pattern)
   */
  private static parseCliArguments(params: any): any {
    // If no args array, return as-is
    if (!params.args || !Array.isArray(params.args)) {
      return params;
    }

    const result: any = { ...params };
    const remainingArgs: string[] = [];
    
    // Parse CLI-style args
    for (const arg of params.args) {
      if (typeof arg === 'string' && arg.startsWith('--')) {
        const [key, value] = arg.split('=', 2);
        const cleanKey = key.replace('--', '');
        if (cleanKey === 'args' && value) {
          // Handle --args as array
          result[cleanKey] = value.split(',');
        } else {
          result[cleanKey] = value || true; // Support flags without values
        }
      } else {
        // Keep non-CLI args (positional arguments)
        remainingArgs.push(arg);
      }
    }
    
    // Replace args with only the non-CLI args
    result.args = remainingArgs;
    return result;
  }

  /**
   * Execute with strongly typed parameters
   */
  private static async executeTyped(params: ExecParameters, _context: ContinuumContext): Promise<CommandResult> {
    const startTime = Date.now();
    console.log('ExecCommand: Starting strongly typed command execution');
    
    let execution: CommandExecution;
    
    // Handle direct CommandExecution object
    if (params.execution && isCommandExecution(params.execution)) {
      execution = params.execution;
      console.log(`ExecCommand: Using provided CommandExecution: ${execution.command}`);
    }
    // Handle command/args format
    else if (params.command) {
      execution = CommandExecutionFactory.create(
        params.command,
        params.args || [],
        {
          source: 'api',
          transport: 'http'
        }
      );
      console.log(`ExecCommand: Created CommandExecution: ${execution.command}`);
    }
    else {
      throw new Error('Must provide either command name or execution object');
    }
    
    // Simplified exec - just return the execution info for now
    // This demonstrates the strongly typed interface without breaking anything
    
    const executionTime = Date.now() - startTime;
    
    console.log(`ExecCommand: Would execute: ${execution.command} with args:`, execution.args);
    
    // Return demonstration of the strongly typed interface
    return {
      success: true,
      data: {
        message: 'Exec command received strongly typed execution',
        execution: execution,
        parsedCommand: execution.command,
        parsedArgs: execution.args,
        metadata: {
          executionTime,
          processor: 'exec-command',
          timestamp: new Date().toISOString()
        }
      },
      timestamp: new Date().toISOString(),
      executionTime
    };
  }
}