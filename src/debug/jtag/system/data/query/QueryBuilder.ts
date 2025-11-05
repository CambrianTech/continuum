/**
 * Dynamic Query Builder Classes - Storage Strategy Aware
 *
 * Modular classes for building SQL queries that work with both:
 * - Field extraction tables (optimized)
 * - JSON_EXTRACT fallbacks (universal compatibility)
 */

import type { EntityFieldMapping } from '../config/EntityFieldConfig';
import type { StorageQuery } from '../../../daemons/data-daemon/shared/DataStorageAdapter';

/**
 * Logical operators for combining filters
 */
export type LogicalOperator = 'AND' | 'OR';

/**
 * Comparison operators for field filtering
 */
export type ComparisonOperator = '=' | '!=' | '<' | '>' | '<=' | '>=' | 'LIKE' | 'IN' | 'IS NULL' | 'IS NOT NULL';

/**
 * Sort direction for ordering
 */
export type SortDirection = 'asc' | 'desc' | 'ASC' | 'DESC';

/**
 * Field Filter - Represents a single field condition
 */
export class FieldFilter {
  constructor(
    public readonly field: string,
    public readonly operator: ComparisonOperator,
    public readonly value?: any,
    public readonly logicalOperator: LogicalOperator = 'AND'
  ) {}

  /**
   * Build SQL condition for this filter
   */
  buildCondition(mapping: EntityFieldMapping | undefined, tablePrefix: { data: string; extract: string }): { condition: string; params: any[] } {
    const params: any[] = [];

    // Check if field is extracted (optimized path)
    const isExtracted = mapping?.extractedFields.some(f => f.fieldName === this.field);

    if (isExtracted && mapping) {
      // Use extracted field column (indexed, fast)
      const extractedField = mapping.extractedFields.find(f => f.fieldName === this.field)!;
      const columnRef = `${tablePrefix.extract}.${this.field}`;

      return this.buildExtractedCondition(columnRef, extractedField, params);
    } else {
      // Use JSON_EXTRACT fallback (works everywhere)
      const safePath = this.sanitizeJsonPath(this.field);
      const columnRef = `JSON_EXTRACT(${tablePrefix.data}.data, '$.${safePath}')`;

      return this.buildJsonCondition(columnRef, params);
    }
  }

  /**
   * Build condition for extracted field (typed, fast)
   */
  private buildExtractedCondition(columnRef: string, extractedField: any, params: any[]): { condition: string; params: any[] } {
    switch (this.operator) {
      case '=':
      case '!=':
      case '<':
      case '>':
      case '<=':
      case '>=':
        params.push(this.convertValue(this.value, extractedField));
        return { condition: `${columnRef} ${this.operator} ?`, params };

      case 'LIKE':
        params.push(this.convertValue(this.value, extractedField));
        return { condition: `${columnRef} LIKE ?`, params };

      case 'IN':
        if (!Array.isArray(this.value)) {
          throw new Error(`IN operator requires array value for field ${this.field}`);
        }
        const placeholders = this.value.map(() => '?').join(', ');
        const convertedValues = this.value.map(v => this.convertValue(v, extractedField));
        params.push(...convertedValues);
        return { condition: `${columnRef} IN (${placeholders})`, params };

      case 'IS NULL':
        return { condition: `${columnRef} IS NULL`, params };

      case 'IS NOT NULL':
        return { condition: `${columnRef} IS NOT NULL`, params };

      default:
        throw new Error(`Unsupported operator: ${this.operator}`);
    }
  }

  /**
   * Build condition for JSON field (universal fallback)
   */
  private buildJsonCondition(columnRef: string, params: any[]): { condition: string; params: any[] } {
    switch (this.operator) {
      case '=':
      case '!=':
      case '<':
      case '>':
      case '<=':
      case '>=':
        params.push(this.value);
        return { condition: `${columnRef} ${this.operator} ?`, params };

      case 'LIKE':
        params.push(this.value);
        return { condition: `${columnRef} LIKE ?`, params };

      case 'IN':
        if (!Array.isArray(this.value)) {
          throw new Error(`IN operator requires array value for field ${this.field}`);
        }
        const placeholders = this.value.map(() => '?').join(', ');
        params.push(...this.value);
        return { condition: `${columnRef} IN (${placeholders})`, params };

      case 'IS NULL':
        return { condition: `${columnRef} IS NULL`, params };

      case 'IS NOT NULL':
        return { condition: `${columnRef} IS NOT NULL`, params };

      default:
        throw new Error(`Unsupported operator: ${this.operator}`);
    }
  }

  /**
   * Convert domain values to storage format using field converters
   */
  private convertValue(value: any, extractedField: any): any {
    if (extractedField?.converter?.toStorage) {
      return extractedField.converter.toStorage(value);
    }
    return value;
  }

  /**
   * Sanitize JSON path to prevent injection
   */
  private sanitizeJsonPath(fieldPath: string): string {
    return fieldPath.replace(/[^a-zA-Z0-9._\[\]]/g, '');
  }
}

/**
 * Sort Field - Represents ordering specification
 */
export class SortField {
  constructor(
    public readonly field: string,
    public readonly direction: SortDirection = 'asc'
  ) {}

  /**
   * Build ORDER BY clause for this sort field
   */
  buildOrderBy(mapping: EntityFieldMapping | undefined, tablePrefix: { data: string; extract: string }): string {
    const isExtracted = mapping?.extractedFields.some(f => f.fieldName === this.field);

    if (isExtracted) {
      // Use extracted field column (indexed, fast sorting)
      return `${tablePrefix.extract}.${this.field} ${this.direction.toUpperCase()}`;
    } else {
      // Use JSON_EXTRACT fallback
      const safePath = this.sanitizeJsonPath(this.field);
      return `JSON_EXTRACT(${tablePrefix.data}.data, '$.${safePath}') ${this.direction.toUpperCase()}`;
    }
  }

  private sanitizeJsonPath(fieldPath: string): string {
    return fieldPath.replace(/[^a-zA-Z0-9._\[\]]/g, '');
  }
}

/**
 * Filter Group - Logical grouping of filters with AND/OR
 */
export class FilterGroup {
  public filters: FieldFilter[] = [];
  public subGroups: FilterGroup[] = [];

  constructor(
    public readonly logicalOperator: LogicalOperator = 'AND'
  ) {}

  /**
   * Add a field filter to this group
   */
  addFilter(field: string, operator: ComparisonOperator, value?: any): FilterGroup {
    this.filters.push(new FieldFilter(field, operator, value, this.logicalOperator));
    return this;
  }

  /**
   * Add a sub-group for complex nested logic
   */
  addGroup(group: FilterGroup): FilterGroup {
    this.subGroups.push(group);
    return this;
  }

  /**
   * Build WHERE clause conditions for this group
   */
  buildConditions(mapping: EntityFieldMapping | undefined, tablePrefix: { data: string; extract: string }): { conditions: string[]; params: any[] } {
    const conditions: string[] = [];
    const allParams: any[] = [];

    // Build conditions for direct filters
    for (const filter of this.filters) {
      const { condition, params } = filter.buildCondition(mapping, tablePrefix);
      conditions.push(condition);
      allParams.push(...params);
    }

    // Build conditions for sub-groups
    for (const subGroup of this.subGroups) {
      const { conditions: subConditions, params: subParams } = subGroup.buildConditions(mapping, tablePrefix);
      if (subConditions.length > 0) {
        const groupCondition = `(${subConditions.join(` ${subGroup.logicalOperator} `)})`;
        conditions.push(groupCondition);
        allParams.push(...subParams);
      }
    }

    return { conditions, params: allParams };
  }
}

/**
 * Dynamic Query Builder - Main orchestrator class
 */
export class DynamicQueryBuilder {
  private collection: string;
  private filterGroup: FilterGroup;
  private sortFields: SortField[] = [];
  private limitCount?: number;
  private offsetCount?: number;

  constructor(collection: string) {
    this.collection = collection;
    this.filterGroup = new FilterGroup('AND');
  }

  /**
   * Add a simple field filter
   */
  where(field: string, operator: ComparisonOperator, value?: any): DynamicQueryBuilder {
    this.filterGroup.addFilter(field, operator, value);
    return this;
  }

  /**
   * Add a complex filter group
   */
  whereGroup(callback: (group: FilterGroup) => void, logicalOperator: LogicalOperator = 'AND'): DynamicQueryBuilder {
    const group = new FilterGroup(logicalOperator);
    callback(group);
    this.filterGroup.addGroup(group);
    return this;
  }

  /**
   * Add sorting
   */
  orderBy(field: string, direction: SortDirection = 'asc'): DynamicQueryBuilder {
    this.sortFields.push(new SortField(field, direction));
    return this;
  }

  /**
   * Add pagination
   */
  limit(count: number): DynamicQueryBuilder {
    this.limitCount = count;
    return this;
  }

  offset(count: number): DynamicQueryBuilder {
    this.offsetCount = count;
    return this;
  }

  /**
   * Build final SQL query with parameters
   */
  build(mapping: EntityFieldMapping | undefined): { sql: string; params: any[] } {
    const hasExtraction = mapping !== undefined;
    const tablePrefix = hasExtraction
      ? { data: 'data', extract: 'ext' }
      : { data: 'data', extract: 'data' };

    // Base SELECT with appropriate JOIN
    let sql = hasExtraction
      ? `SELECT data.* FROM _data data LEFT JOIN _extract_${this.collection} ext ON data.id = ext.id`
      : `SELECT * FROM _data data`;

    sql += ` WHERE data.collection = ?`;
    const params: any[] = [this.collection];

    // Add filter conditions
    const { conditions, params: filterParams } = this.filterGroup.buildConditions(mapping, tablePrefix);
    if (conditions.length > 0) {
      sql += ` AND (${conditions.join(` ${this.filterGroup.logicalOperator} `)})`;
      params.push(...filterParams);
    }

    // Add sorting
    if (this.sortFields.length > 0) {
      const orderClauses = this.sortFields.map(sort => sort.buildOrderBy(mapping, tablePrefix));
      sql += ` ORDER BY ${orderClauses.join(', ')}`;
    }

    // Add pagination
    if (this.limitCount) {
      sql += ` LIMIT ?`;
      params.push(this.limitCount);
    }

    if (this.offsetCount) {
      sql += ` OFFSET ?`;
      params.push(this.offsetCount);
    }

    return { sql, params };
  }

  /**
   * Convert from StorageQuery to DynamicQueryBuilder
   */
  static fromStorageQuery(query: StorageQuery): DynamicQueryBuilder {
    const builder = new DynamicQueryBuilder(query.collection);

    // Add filters
    if (query.filter) {
      for (const [field, value] of Object.entries(query.filter)) {
        if (Array.isArray(value)) {
          builder.where(field, 'IN', value);
        } else if (value === null) {
          builder.where(field, 'IS NULL');
        } else {
          builder.where(field, '=', value);
        }
      }
    }

    // Add sorting
    if (query.sort) {
      for (const sort of query.sort) {
        builder.orderBy(sort.field, sort.direction);
      }
    }

    // Add pagination
    if (query.limit) {
      builder.limit(query.limit);
    }
    if (query.offset) {
      builder.offset(query.offset);
    }

    return builder;
  }
}