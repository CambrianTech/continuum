/**
 * SQLite Query Builder - Single Source of Truth for SQL Generation
 *
 * Used by both actual query execution and explain functionality
 * to ensure queries are identical and true-to-life
 */

import type { StorageQuery } from '../shared/DataStorageAdapter';
import { SqlNamingConverter } from './SqliteStorageAdapter';

export interface SqliteQueryResult {
  sql: string;
  params: unknown[];
  description: string;
}

export class SqliteQueryBuilder {
  /**
   * Build a SELECT query from StorageQuery
   */
  static buildSelect(query: StorageQuery, tableName?: string): SqliteQueryResult {
    const table = tableName || SqlNamingConverter.toTableName(query.collection);
    const params: unknown[] = [];
    const operations: string[] = [];

    // Base SELECT with JSON data extraction
    let sql = `
      SELECT
        id,
        data,
        created_at,
        updated_at,
        version
      FROM \`${table}\`
    `;

    operations.push(`SELECT from table "${table}"`);

    // WHERE clause from filters
    const whereClauses: string[] = [];

    // Universal filters (new system)
    if (query.filter) {
      operations.push('FILTER with universal operators:');
      for (const [field, filter] of Object.entries(query.filter)) {
        const columnName = `JSON_EXTRACT(data, '$.${field}')`;

        if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
          // Handle operators like { $gt: value, $in: [...] }
          for (const [operator, value] of Object.entries(filter)) {
            const { clause, addedParams, description } = this.buildOperatorClause(columnName, operator, value, field);
            if (clause) {
              whereClauses.push(clause);
              params.push(...addedParams);
              operations.push(`  - ${description}`);
            }
          }
        } else {
          // Direct value implies equality
          whereClauses.push(`${columnName} = ?`);
          params.push(filter);
          operations.push(`  - field "${field}" equals ${JSON.stringify(filter)}`);
        }
      }
    }

    // Legacy filters (backward compatibility)
    if (query.filters) {
      operations.push('FILTER with legacy filters:');
      for (const [field, value] of Object.entries(query.filters)) {
        const columnName = `JSON_EXTRACT(data, '$.${field}')`;
        whereClauses.push(`${columnName} = ?`);
        params.push(value);
        operations.push(`  - field "${field}" equals ${JSON.stringify(value)}`);
      }
    }

    if (whereClauses.length > 0) {
      sql += ' WHERE ' + whereClauses.join(' AND ');
    }

    // ORDER BY clause
    if (query.sort && query.sort.length > 0) {
      const orderClauses = query.sort.map(sortField => {
        const columnName = `JSON_EXTRACT(data, '$.${sortField.field}')`;
        // For timestamp fields, treat as datetime for proper sorting
        if (sortField.field === 'timestamp' || sortField.field.includes('Time') || sortField.field.includes('Date')) {
          return `datetime(${columnName}) ${sortField.direction.toUpperCase()}`;
        }
        return `${columnName} ${sortField.direction.toUpperCase()}`;
      });
      sql += ' ORDER BY ' + orderClauses.join(', ');

      const sortDesc = query.sort.map(s => `"${s.field}" ${s.direction.toUpperCase()}`).join(', ');
      operations.push(`ORDER BY ${sortDesc}`);
    }

    // LIMIT clause
    if (query.limit) {
      sql += ' LIMIT ?';
      params.push(query.limit);
      operations.push(`LIMIT to ${query.limit} records`);
    }

    // OFFSET clause
    if (query.offset) {
      sql += ' OFFSET ?';
      params.push(query.offset);
      operations.push(`SKIP first ${query.offset} records`);
    }

    return {
      sql: sql.trim(),
      params,
      description: operations.join('\n')
    };
  }

  /**
   * Build operator clause for WHERE conditions
   */
  private static buildOperatorClause(
    columnName: string,
    operator: string,
    value: unknown,
    field: string
  ): { clause: string; addedParams: unknown[]; description: string } {
    const addedParams: unknown[] = [];

    switch (operator) {
      case '$eq':
        addedParams.push(value);
        return {
          clause: `${columnName} = ?`,
          addedParams,
          description: `field "${field}" equals ${JSON.stringify(value)}`
        };

      case '$ne':
        addedParams.push(value);
        return {
          clause: `${columnName} != ?`,
          addedParams,
          description: `field "${field}" does not equal ${JSON.stringify(value)}`
        };

      case '$gt':
        addedParams.push(value);
        return {
          clause: `${columnName} > ?`,
          addedParams,
          description: `field "${field}" is greater than ${JSON.stringify(value)}`
        };

      case '$gte':
        addedParams.push(value);
        return {
          clause: `${columnName} >= ?`,
          addedParams,
          description: `field "${field}" is greater than or equal to ${JSON.stringify(value)}`
        };

      case '$lt':
        addedParams.push(value);
        return {
          clause: `${columnName} < ?`,
          addedParams,
          description: `field "${field}" is less than ${JSON.stringify(value)}`
        };

      case '$lte':
        addedParams.push(value);
        return {
          clause: `${columnName} <= ?`,
          addedParams,
          description: `field "${field}" is less than or equal to ${JSON.stringify(value)}`
        };

      case '$in':
        if (Array.isArray(value) && value.length > 0) {
          const placeholders = value.map(() => '?').join(', ');
          addedParams.push(...value);
          return {
            clause: `${columnName} IN (${placeholders})`,
            addedParams,
            description: `field "${field}" is in ${JSON.stringify(value)}`
          };
        }
        break;

      case '$nin':
        if (Array.isArray(value) && value.length > 0) {
          const placeholders = value.map(() => '?').join(', ');
          addedParams.push(...value);
          return {
            clause: `${columnName} NOT IN (${placeholders})`,
            addedParams,
            description: `field "${field}" is not in ${JSON.stringify(value)}`
          };
        }
        break;

      case '$exists':
        if (value === true) {
          return {
            clause: `${columnName} IS NOT NULL`,
            addedParams,
            description: `field "${field}" exists`
          };
        } else {
          return {
            clause: `${columnName} IS NULL`,
            addedParams,
            description: `field "${field}" does not exist`
          };
        }

      case '$regex':
        addedParams.push(value);
        return {
          clause: `${columnName} REGEXP ?`,
          addedParams,
          description: `field "${field}" matches pattern ${JSON.stringify(value)}`
        };

      case '$contains':
        addedParams.push(`%${value}%`);
        return {
          clause: `${columnName} LIKE ?`,
          addedParams,
          description: `field "${field}" contains ${JSON.stringify(value)}`
        };
    }

    // Fallback for unknown operators
    return {
      clause: '',
      addedParams: [],
      description: `field "${field}" ${operator} ${JSON.stringify(value)} (unknown operator)`
    };
  }

  /**
   * Convert camelCase to snake_case for table names (deprecated - use SqlNamingConverter)
   */
  private static toSnakeCase(str: string): string {
    return str.replace(/([A-Z])/g, '_$1').toLowerCase();
  }
}