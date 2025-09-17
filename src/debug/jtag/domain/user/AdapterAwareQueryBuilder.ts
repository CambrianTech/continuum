/**
 * Adapter-Aware Query Builder - Handles different backend query formats
 *
 * Automatically adapts queries based on the underlying DataDaemon adapter:
 * - SQL adapters: Use SQL syntax (WHERE, JOIN, etc.)
 * - NoSQL adapters: Use document queries ($gte, $in, etc.)
 * - JSON adapters: Use JavaScript filter functions
 * - Memory adapters: Use in-memory filtering
 *
 * The repository calls this, and it generates the right query format.
 */

import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { StorageQuery } from '../../daemons/data-daemon/shared/DataStorageAdapter';

/**
 * Query conditions that work across all adapter types
 */
export interface UniversalQueryConditions {
  // Basic equality
  equals?: Record<string, any>;

  // Comparison operators
  greaterThan?: Record<string, any>;
  greaterThanOrEqual?: Record<string, any>;
  lessThan?: Record<string, any>;
  lessThanOrEqual?: Record<string, any>;

  // Array operators
  in?: Record<string, any[]>;
  notIn?: Record<string, any[]>;

  // String operators
  contains?: Record<string, string>;
  startsWith?: Record<string, string>;
  endsWith?: Record<string, string>;

  // Logical operators
  and?: UniversalQueryConditions[];
  or?: UniversalQueryConditions[];
  not?: UniversalQueryConditions;
}

/**
 * Universal sorting specification
 */
export interface UniversalSort {
  field: string;
  direction: 'asc' | 'desc';
}

/**
 * Universal query options
 */
export interface UniversalQueryOptions {
  conditions?: UniversalQueryConditions;
  sort?: UniversalSort[];
  limit?: number;
  offset?: number;

  // Relationship loading hints (adapter-specific interpretation)
  include?: string[]; // e.g., ['sessions', 'permissions']

  // Adapter-specific hints
  hints?: {
    sql?: {
      useIndex?: string;
      forceJoin?: boolean;
    };
    nosql?: {
      allowDiskUse?: boolean;
      explain?: boolean;
    };
    json?: {
      useCache?: boolean;
    };
  };
}

/**
 * Detects adapter type from DataDaemon configuration or adapter name
 */
export type AdapterType = 'sql' | 'nosql' | 'json' | 'memory' | 'network' | 'unknown';

export class AdapterAwareQueryBuilder {
  /**
   * Build query for specific collection with universal conditions
   */
  static buildQuery(
    collection: string,
    options: UniversalQueryOptions,
    adapterType: AdapterType = 'unknown'
  ): StorageQuery {
    const query: StorageQuery = {
      collection,
      filters: {},
      limit: options.limit,
      offset: options.offset
    };

    // Convert universal conditions to adapter-specific format
    const filters = options.conditions
      ? this.convertConditions(options.conditions, adapterType)
      : query.filters;

    // Convert universal sort to adapter-specific format
    const sort = (options.sort && options.sort.length > 0)
      ? this.convertSort(options.sort, adapterType)
      : query.sort;

    // Create new query object with converted properties
    const convertedQuery: StorageQuery = {
      ...query,
      filters,
      sort
    };

    // Note: Adapter-specific hints would need to be handled differently
    // since StorageQuery doesn't have an 'options' property

    return convertedQuery;
  }

  /**
   * Convert universal conditions to adapter-specific filter format
   */
  private static convertConditions(
    conditions: UniversalQueryConditions,
    adapterType: AdapterType
  ): Record<string, any> {
    switch (adapterType) {
      case 'sql':
        return this.convertToSQLConditions(conditions);
      case 'nosql':
        return this.convertToNoSQLConditions(conditions);
      case 'json':
        return this.convertToJSONConditions(conditions);
      case 'memory':
        return this.convertToMemoryConditions(conditions);
      default:
        // Generic format that most adapters can interpret
        return this.convertToGenericConditions(conditions);
    }
  }

  /**
   * SQL adapter conditions (using SQL-like operators)
   */
  private static convertToSQLConditions(conditions: UniversalQueryConditions): Record<string, any> {
    const filters: Record<string, any> = {};

    if (conditions.equals) {
      Object.assign(filters, conditions.equals);
    }

    if (conditions.greaterThan) {
      for (const [field, value] of Object.entries(conditions.greaterThan)) {
        filters[`${field} >`] = value;
      }
    }

    if (conditions.greaterThanOrEqual) {
      for (const [field, value] of Object.entries(conditions.greaterThanOrEqual)) {
        filters[`${field} >=`] = value;
      }
    }

    if (conditions.in) {
      for (const [field, values] of Object.entries(conditions.in)) {
        filters[`${field} IN`] = values;
      }
    }

    if (conditions.contains) {
      for (const [field, value] of Object.entries(conditions.contains)) {
        filters[`${field} LIKE`] = `%${value}%`;
      }
    }

    if (conditions.and) {
      filters['$AND'] = conditions.and.map(cond => this.convertToSQLConditions(cond));
    }

    if (conditions.or) {
      filters['$OR'] = conditions.or.map(cond => this.convertToSQLConditions(cond));
    }

    return filters;
  }

  /**
   * NoSQL adapter conditions (MongoDB-style)
   */
  private static convertToNoSQLConditions(conditions: UniversalQueryConditions): Record<string, any> {
    const filters: Record<string, any> = {};

    if (conditions.equals) {
      Object.assign(filters, conditions.equals);
    }

    if (conditions.greaterThan) {
      for (const [field, value] of Object.entries(conditions.greaterThan)) {
        filters[field] = { ...filters[field], $gt: value };
      }
    }

    if (conditions.greaterThanOrEqual) {
      for (const [field, value] of Object.entries(conditions.greaterThanOrEqual)) {
        filters[field] = { ...filters[field], $gte: value };
      }
    }

    if (conditions.lessThan) {
      for (const [field, value] of Object.entries(conditions.lessThan)) {
        filters[field] = { ...filters[field], $lt: value };
      }
    }

    if (conditions.lessThanOrEqual) {
      for (const [field, value] of Object.entries(conditions.lessThanOrEqual)) {
        filters[field] = { ...filters[field], $lte: value };
      }
    }

    if (conditions.in) {
      for (const [field, values] of Object.entries(conditions.in)) {
        filters[field] = { $in: values };
      }
    }

    if (conditions.notIn) {
      for (const [field, values] of Object.entries(conditions.notIn)) {
        filters[field] = { $nin: values };
      }
    }

    if (conditions.contains) {
      for (const [field, value] of Object.entries(conditions.contains)) {
        filters[field] = { $regex: value, $options: 'i' };
      }
    }

    if (conditions.and) {
      filters['$and'] = conditions.and.map(cond => this.convertToNoSQLConditions(cond));
    }

    if (conditions.or) {
      filters['$or'] = conditions.or.map(cond => this.convertToNoSQLConditions(cond));
    }

    return filters;
  }

  /**
   * JSON adapter conditions (JavaScript function format)
   */
  private static convertToJSONConditions(conditions: UniversalQueryConditions): Record<string, any> {
    // JSON adapter might store filter functions as strings
    const filters: Record<string, any> = {};

    if (conditions.equals) {
      Object.assign(filters, conditions.equals);
    }

    // For complex conditions, store as evaluatable expressions
    if (conditions.greaterThanOrEqual) {
      for (const [field, value] of Object.entries(conditions.greaterThanOrEqual)) {
        filters[`${field}__gte`] = value;
      }
    }

    if (conditions.in) {
      for (const [field, values] of Object.entries(conditions.in)) {
        filters[`${field}__in`] = values;
      }
    }

    if (conditions.contains) {
      for (const [field, value] of Object.entries(conditions.contains)) {
        filters[`${field}__contains`] = value;
      }
    }

    return filters;
  }

  /**
   * Memory adapter conditions (direct JavaScript objects)
   */
  private static convertToMemoryConditions(conditions: UniversalQueryConditions): Record<string, any> {
    // Memory adapter can use JavaScript functions directly
    return this.convertToGenericConditions(conditions);
  }

  /**
   * Generic conditions that work with most adapters
   */
  private static convertToGenericConditions(conditions: UniversalQueryConditions): Record<string, any> {
    const filters: Record<string, any> = {};

    if (conditions.equals) {
      Object.assign(filters, conditions.equals);
    }

    // Use MongoDB-style operators as fallback since they're widely supported
    if (conditions.greaterThanOrEqual) {
      for (const [field, value] of Object.entries(conditions.greaterThanOrEqual)) {
        filters[field] = { $gte: value };
      }
    }

    if (conditions.in) {
      for (const [field, values] of Object.entries(conditions.in)) {
        filters[field] = { $in: values };
      }
    }

    if (conditions.or) {
      filters['$or'] = conditions.or.map(cond => this.convertToGenericConditions(cond));
    }

    return filters;
  }

  /**
   * Convert universal sort to adapter-specific format
   */
  private static convertSort(sort: UniversalSort[], adapterType: AdapterType): any {
    switch (adapterType) {
      case 'sql':
        // SQL: ORDER BY field ASC/DESC
        return sort.map(s => `${s.field} ${s.direction.toUpperCase()}`).join(', ');
      case 'nosql':
        // MongoDB: { field: 1/-1 }
        const mongoSort: Record<string, number> = {};
        sort.forEach(s => {
          mongoSort[s.field] = s.direction === 'asc' ? 1 : -1;
        });
        return mongoSort;
      default:
        // Generic array format
        return sort.map(s => ({ field: s.field, direction: s.direction }));
    }
  }

  /**
   * Convert adapter-specific hints
   */
  private static convertHints(hints: NonNullable<UniversalQueryOptions['hints']>, adapterType: AdapterType): any {
    switch (adapterType) {
      case 'sql':
        return hints.sql;
      case 'nosql':
        return hints.nosql;
      case 'json':
        return hints.json;
      default:
        return {};
    }
  }

  /**
   * Helper methods for common user queries
   */
  static queryActiveUsers(sinceDate: string): UniversalQueryOptions {
    return {
      conditions: {
        and: [
          { equals: { isOnline: true } },
          { greaterThanOrEqual: { lastActiveAt: sinceDate } }
        ]
      },
      sort: [{ field: 'lastActiveAt', direction: 'desc' }]
    };
  }

  static queryUsersByType(citizenType: 'human' | 'ai', aiType?: 'agent' | 'persona'): UniversalQueryOptions {
    const conditions: UniversalQueryConditions = {
      equals: { citizenType }
    };

    if (aiType) {
      conditions.and = [
        conditions,
        { equals: { aiType } }
      ];
    }

    return { conditions };
  }

  static queryUsersWithPermission(permission: string, resource?: string): UniversalQueryOptions {
    // This would require a JOIN/lookup operation - implementation depends on adapter
    const conditions: UniversalQueryConditions = {};

    // For SQL: JOIN with user_permissions table
    // For NoSQL: $lookup or separate query + filter
    // For JSON: Manual relationship resolution

    return {
      conditions,
      hints: {
        sql: { forceJoin: true },
        nosql: { allowDiskUse: true }
      }
    };
  }
}