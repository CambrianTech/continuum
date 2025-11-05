/**
 * CLI Integration Parser
 * Converts CLI args format to BaseCommand's canonical JSON format
 */

import { IntegrationParser } from './IntegrationParser';

export class CLIIntegrationParser implements IntegrationParser {
  name = 'CLI';
  priority = 100; // High priority - specific format
  
  canHandle(params: unknown): boolean {
    return !!(params && 
              typeof params === 'object' && 
              'args' in params && 
              Array.isArray((params as any).args) &&
              (params as any).args.length > 0);
  }
  
  parse<T>(params: unknown): T {
    const { args } = params as { args: string[] };
    const result: Record<string, unknown> = {};
    const positionalArgs: string[] = [];
    
    console.log('🔍 CLIIntegrationParser.parse - input params:', params);
    
    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      
      if (typeof arg === 'string' && arg.startsWith('--')) {
        if (arg.includes('=')) {
          // Handle --key=value format
          const [key, ...valueParts] = arg.substring(2).split('=');
          const rawValue = valueParts.join('=');
          result[key] = this.parseValue(rawValue);
        } else {
          // Handle --key value format (next argument is the value)
          const key = arg.substring(2);
          const nextArg = args[i + 1];
          
          if (nextArg && !nextArg.startsWith('--')) {
            // Next argument is the value
            result[key] = this.parseValue(nextArg);
            i++; // Skip the next argument since we consumed it
          } else {
            // No value, treat as boolean flag
            result[key] = true;
          }
        }
      } else {
        // Positional argument (doesn't start with --)
        positionalArgs.push(arg);
      }
    }
    
    // Map positional arguments to common parameter names
    if (positionalArgs.length > 0) {
      // For help command: first positional arg becomes "command"
      // For other commands: first positional arg becomes common parameter names
      result.command = positionalArgs[0];
      result.filename = positionalArgs[0]; // For file commands
      result.target = positionalArgs[0];   // For reload, etc.
      result.script = positionalArgs[0];   // For js-execute command
      
      // If there are multiple positional args, keep them as an array
      if (positionalArgs.length > 1) {
        result.args = positionalArgs;
      }
    }
    
    console.log('🔍 CLIIntegrationParser.parse - output result:', result);
    return result as T;
  }
  
  private parseValue(value: string): unknown {
    if (!value) return true; // --flag without value
    
    // Try JSON first (for objects/arrays)
    try {
      return JSON.parse(value);
    } catch {
      return value; // String as-is
    }
  }
}