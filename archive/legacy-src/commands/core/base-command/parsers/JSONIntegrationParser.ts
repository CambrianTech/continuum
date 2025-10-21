/**
 * JSON Integration Parser
 * Handles various JSON-based formats and converts to BaseCommand's canonical format
 */

import { IntegrationParser } from './IntegrationParser';

export class JSONWithArgsIntegrationParser implements IntegrationParser {
  name = 'JSON-with-args';
  priority = 90; // High priority - specific hybrid format
  
  canHandle(params: unknown): boolean {
    return !!(params && 
              typeof params === 'object' && 
              'args' in params && 
              Array.isArray((params as any).args) &&
              (params as any).args.length === 0);
  }
  
  parse<T>(params: unknown): T {
    const { args, ...cleanParams } = params as any;
    return cleanParams as T;
  }
}

export class PureJSONIntegrationParser implements IntegrationParser {
  name = 'Pure-JSON';
  priority = 10; // Low priority - catch-all for objects
  
  canHandle(params: unknown): boolean {
    return !!(params && typeof params === 'object');
  }
  
  parse<T>(params: unknown): T {
    return params as T;
  }
}

export class StringJSONIntegrationParser implements IntegrationParser {
  name = 'String-JSON';
  priority = 80; // High priority - specific string format
  
  canHandle(params: unknown): boolean {
    return typeof params === 'string';
  }
  
  parse<T>(params: unknown): T {
    try {
      return JSON.parse(params as string) as T;
    } catch {
      // If not JSON, treat as single parameter
      return { data: params } as T;
    }
  }
}