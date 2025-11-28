/**
 * SQL Naming Convention Converter
 *
 * Handles conversion between camelCase (TypeScript) and snake_case (SQL).
 * Extracted from SqliteStorageAdapter for reusability across SQL adapters.
 */

export class SqlNamingConverter {
  /**
   * Convert camelCase to snake_case for SQL columns
   */
  static toSnakeCase(camelCase: string): string {
    return camelCase.replace(/([A-Z])/g, '_$1').toLowerCase().replace(/^_/, '');
  }

  /**
   * Convert snake_case back to camelCase for object properties
   */
  static toCamelCase(snakeCase: string): string {
    return snakeCase.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
  }

  /**
   * Convert collection name to table name (snake_case)
   *
   * ARCHITECTURE-RULES.md compliance:
   * - Collection name IS the table name (no pluralization)
   * - Entities define .collection property with correct name
   * - No English grammar rules - use what's given
   */
  static toTableName(collectionName: string): string {
    return SqlNamingConverter.toSnakeCase(collectionName);
  }
}
