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
    
    for (const arg of args) {
      if (typeof arg === 'string' && arg.startsWith('--')) {
        const [key, ...valueParts] = arg.substring(2).split('=');
        const rawValue = valueParts.join('=');
        
        // Smart value parsing
        result[key] = this.parseValue(rawValue);
      }
    }
    
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