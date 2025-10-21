/**
 * Integration Parser Interface
 * Universal format converter - any format to BaseCommand's canonical JSON format
 */

export interface IntegrationParser {
  /**
   * Can this parser handle the input format?
   */
  canHandle(params: unknown): boolean;
  
  /**
   * Parse input format to BaseCommand's canonical JSON format
   */
  parse<T>(params: unknown): T;
  
  /**
   * Optional priority for ordering (higher = checked first)
   */
  priority?: number;
  
  /**
   * Optional name for debugging
   */
  name?: string;
}

/**
 * Registry for integration parsers
 */
export class IntegrationParserRegistry {
  private static parsers: IntegrationParser[] = [];
  
  static register(parser: IntegrationParser): void {
    this.parsers.push(parser);
    // Sort by priority (higher first)
    this.parsers.sort((a, b) => (b.priority || 0) - (a.priority || 0));
  }
  
  static getAll(): IntegrationParser[] {
    return [...this.parsers];
  }
  
  static parse<T>(params: unknown): T {
    for (const parser of this.parsers) {
      if (parser.canHandle(params)) {
        return parser.parse<T>(params);
      }
    }
    
    // Fallback: assume already in canonical format
    return params as T;
  }
}