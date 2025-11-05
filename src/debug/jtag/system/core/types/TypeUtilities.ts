/**
 * TypeUtilities - Cross-cutting type checking and validation utilities
 * 
 * PURPOSE: Centralized type checking logic to avoid duplication
 * PATTERN: Single responsibility for runtime type validation
 * USAGE: Import and use across all modules that need type checking
 */

/**
 * Generic object property checker - cross-cutting utility
 */
export class TypeUtilities {
  
  /**
   * Check if value is object with required properties
   */
  static hasProperties(value: unknown, properties: string[]): value is Record<string, unknown> {
    return (
      value !== null &&
      typeof value === 'object' &&
      properties.every(prop => prop in value)
    );
  }

  /**
   * Check if value is object with at least one property
   */
  static hasProperty(value: unknown, property: string): value is Record<string, unknown> {
    return value !== null && typeof value === 'object' && property in value;
  }

  /**
   * Safe cast to type T - use when confident about type structure
   */
  static cast<T>(value: unknown): T {
    return value as T;
  }

  /**
   * Check if value is non-null object
   */
  static isObject(value: unknown): value is Record<string, unknown> {
    return value !== null && typeof value === 'object';
  }
}