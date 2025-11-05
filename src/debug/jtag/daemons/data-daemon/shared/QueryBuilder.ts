/**
 * Query Builder - SQL-like Interface for Generic ORM
 *
 * Provides SQL-like query building with joins, relations, and complex filtering
 * Works across all storage backends (file, memory, SQL, NoSQL)
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { StorageQuery } from './DataStorageAdapter';

/**
 * Join Types - SQL Equivalent Relations
 */
export type JoinType = 'inner' | 'left' | 'right' | 'full' | 'cross';

/**
 * Comparison Operators for Filtering
 */
export type ComparisonOperator =
  | 'eq'      // =
  | 'ne'      // !=
  | 'gt'      // >
  | 'gte'     // >=
  | 'lt'      // <
  | 'lte'     // <=
  | 'in'      // IN (...)
  | 'nin'     // NOT IN (...)
  | 'like'    // LIKE %pattern%
  | 'regex'   // Regular expression match
  | 'exists'  // Field exists
  | 'null'    // IS NULL
  | 'between'; // BETWEEN x AND y

/**
 * Join Definition - Foreign Key Relations
 */
export interface JoinDefinition {
  readonly type: JoinType;
  readonly collection: string;          // Target collection/table
  readonly alias?: string;             // Optional alias for target
  readonly on: {
    readonly local: string;            // Local field name
    readonly foreign: string;          // Foreign field name
  };
  readonly select?: readonly string[]; // Fields to include from joined collection
}

/**
 * Advanced Filter with Operators
 */
export interface FilterCondition {
  readonly field: string;
  readonly operator: ComparisonOperator;
  readonly value: any;
  readonly collection?: string; // For joined collections
}

/**
 * Logical Grouping (AND/OR)
 */
export interface FilterGroup {
  readonly operator: 'and' | 'or';
  readonly conditions: readonly (FilterCondition | FilterGroup)[];
}

/**
 * Aggregation Functions
 */
export type AggregationType = 'count' | 'sum' | 'avg' | 'min' | 'max' | 'distinct';

export interface AggregationDefinition {
  readonly type: AggregationType;
  readonly field?: string;
  readonly alias?: string;
  readonly collection?: string; // For joined collections
}

/**
 * Enhanced Storage Query with Relations
 */
export interface RelationalQuery extends Omit<StorageQuery, 'filters'> {
  // Base collection
  readonly collection: string;
  readonly alias?: string;

  // Joins and Relations
  readonly joins?: readonly JoinDefinition[];

  // Advanced Filtering
  readonly where?: FilterGroup | FilterCondition;

  // Field Selection
  readonly select?: readonly string[]; // Fields to return
  readonly exclude?: readonly string[]; // Fields to exclude

  // Grouping and Aggregation
  readonly groupBy?: readonly string[];
  readonly having?: FilterGroup | FilterCondition;
  readonly aggregations?: readonly AggregationDefinition[];

  // Sorting and Pagination
  readonly orderBy?: readonly { field: string; direction: 'asc' | 'desc'; collection?: string }[];
  readonly limit?: number;
  readonly offset?: number;

  // Performance Hints
  readonly useIndex?: readonly string[];
  readonly forceIndex?: string;
  readonly explain?: boolean; // Return query execution plan
}

/**
 * Query Result with Metadata
 */
export interface QueryResult<T = any> {
  readonly success: boolean;
  readonly data?: T[];
  readonly error?: string;
  readonly metadata?: {
    readonly totalCount?: number;
    readonly queryTime?: number;
    readonly joinCount?: number;
    readonly cacheHit?: boolean;
    readonly executionPlan?: string;
  };
}

/**
 * Fluent Query Builder - SQL-like Interface
 *
 * Example Usage:
 * ```typescript
 * const query = QueryBuilder
 *   .from('users')
 *   .leftJoin('user_sessions', 'userId', 'userId')
 *   .leftJoin('room_participations', 'userId', 'userId')
 *   .where('users.isOnline', 'eq', true)
 *   .where('room_participations.roomId', 'eq', 'room-123')
 *   .orderBy('users.lastActiveAt', 'desc')
 *   .limit(10)
 *   .build();
 * ```
 */

/**
 * Type aliases for convenience
 */
export type WhereClause = FilterGroup | FilterCondition;
export type OrderByDefinition = { field: string; direction: 'asc' | 'desc'; collection?: string };

/**
 * Utility type to make readonly properties mutable
 */
type Mutable<T> = {
  -readonly [P in keyof T]: T[P] extends readonly (infer U)[]
    ? U[]
    : T[P];
};

/**
 * Smart value types for adaptive function signatures
 */
export type QueryValue = string | number | boolean | null | Date | QueryValue[];
export type OperatorValue<T extends ComparisonOperator> =
  T extends 'between' ? [QueryValue, QueryValue] :
  T extends 'in' | 'nin' ? QueryValue[] :
  QueryValue;

/**
 * Fluent condition builder for complex where clauses
 */
export interface ConditionBuilder {
  eq(value: QueryValue): FilterCondition;
  ne(value: QueryValue): FilterCondition;
  gt(value: QueryValue): FilterCondition;
  gte(value: QueryValue): FilterCondition;
  lt(value: QueryValue): FilterCondition;
  lte(value: QueryValue): FilterCondition;
  in(values: QueryValue[]): FilterCondition;
  nin(values: QueryValue[]): FilterCondition;
  like(pattern: string): FilterCondition;
  between(min: QueryValue, max: QueryValue): FilterCondition;
  exists(): FilterCondition;
  null(): FilterCondition;
}

/**
 * Collection-aware field builder
 */
export interface FieldBuilder {
  from(collection: string): ConditionBuilder;
  eq(value: QueryValue): FilterCondition;
  ne(value: QueryValue): FilterCondition;
  gt(value: QueryValue): FilterCondition;
  gte(value: QueryValue): FilterCondition;
  lt(value: QueryValue): FilterCondition;
  lte(value: QueryValue): FilterCondition;
  in(values: QueryValue[]): FilterCondition;
  nin(values: QueryValue[]): FilterCondition;
  like(pattern: string): FilterCondition;
  between(min: QueryValue, max: QueryValue): FilterCondition;
  exists(): FilterCondition;
  null(): FilterCondition;
}

/**
 * Mutable Query State for Builder Pattern
 */
interface MutableQuery {
  collection?: string;
  alias?: string;
  joins?: JoinDefinition[];
  where?: WhereClause;
  select?: string[];
  exclude?: string[];
  orderBy?: OrderByDefinition[];
  limit?: number;
  offset?: number;
  aggregations?: AggregationDefinition[];
  groupBy?: string[];
  explain?: boolean;
  // Legacy compatibility
  filters?: Record<string, unknown>;
  sort?: Array<{ field: string; direction: 'asc' | 'desc' }>;
}

export class QueryBuilder {
  private query: MutableQuery = {};

  /**
   * Start building a query from a collection
   */
  static from(collection: string, alias?: string): QueryBuilder {
    const builder = new QueryBuilder();
    builder.query.collection = collection;
    if (alias) {
      builder.query.alias = alias;
    }
    return builder;
  }

  /**
   * Smart field selector - returns fluent condition builder
   * Usage: QB.from('users').field('age').gt(18)
   */
  field(name: string): FieldBuilder {
    return new FieldBuilderImpl(this, name);
  }

  /**
   * Extension point for custom query types
   * Usage: QB.from('users').extend(customPlugin)
   */
  extend<T extends QueryBuilder>(plugin: (builder: this) => T): T {
    return plugin(this);
  }

  /**
   * Smart pagination with sensible defaults
   */
  paginate(page: number = 1, size: number = 10): QueryBuilder {
    return this.limit(size).offset((page - 1) * size);
  }

  /**
   * Quick search across multiple fields
   */
  search(term: string, fields: string[]): QueryBuilder {
    if (!term.trim()) return this;

    // Create OR group for searching across fields
    const conditions = fields.map(field => ({
      field,
      operator: 'like' as const,
      value: `%${term}%`
    }));

    if (conditions.length === 1) {
      return this.addCondition(conditions[0]);
    }

    const searchGroup: FilterGroup = {
      operator: 'or',
      conditions
    };

    if (!this.query.where) {
      this.query.where = searchGroup;
    } else {
      this.query.where = this.combineWhereClause(this.query.where, searchGroup, 'and');
    }

    return this;
  }

  /**
   * Smart sorting with multiple fields
   * Usage: .sort('name', 'asc', 'age', 'desc')
   * Or: .sort([{field: 'name', dir: 'asc'}])
   */
  sort(...args: Array<string | 'asc' | 'desc' | OrderByDefinition[]>): QueryBuilder {
    if (args.length === 1 && Array.isArray(args[0])) {
      // Array form: sort([{field: 'name', direction: 'asc'}])
      this.query.orderBy = args[0];
      return this;
    }

    // Variadic form: sort('name', 'asc', 'age', 'desc')
    if (!this.query.orderBy) this.query.orderBy = [];

    for (let i = 0; i < args.length; i += 2) {
      const field = args[i] as string;
      const direction = (args[i + 1] as 'asc' | 'desc') || 'asc';
      this.query.orderBy.push({ field, direction });
    }

    return this;
  }

  /**
   * Add a join to another collection
   */
  join(type: JoinType, collection: string, localField: string, foreignField: string, alias?: string): QueryBuilder {
    if (!this.query.joins) {
      this.query.joins = [];
    }

    this.query.joins.push({
      type,
      collection,
      alias,
      on: { local: localField, foreign: foreignField }
    });

    return this;
  }

  /**
   * Add an inner join
   */
  innerJoin(collection: string, localField: string, foreignField: string, alias?: string): QueryBuilder {
    return this.join('inner', collection, localField, foreignField, alias);
  }

  /**
   * Add a left join
   */
  leftJoin(collection: string, localField: string, foreignField: string, alias?: string): QueryBuilder {
    return this.join('left', collection, localField, foreignField, alias);
  }

  /**
   * Add a where condition - Multiple adaptive signatures for ease of use
   */
  // Simple equality: where('name', 'John')
  where(field: string, value: unknown): QueryBuilder;
  // With operator: where('age', 'gt', 18)
  where(field: string, operator: ComparisonOperator, value: unknown): QueryBuilder;
  // With collection: where('users.name', 'eq', 'John')
  where(field: string, operator: ComparisonOperator, value: unknown, collection: string): QueryBuilder;
  // Object form: where({ name: 'John', age: { gt: 18 } })
  where(conditions: Record<string, unknown | { [op in ComparisonOperator]?: unknown }>): QueryBuilder;

  where(
    fieldOrConditions: string | Record<string, unknown>,
    operatorOrValue?: ComparisonOperator | unknown,
    value?: unknown,
    collection?: string
  ): QueryBuilder {
    // Handle object form: where({ name: 'John', age: { gt: 18 } })
    if (typeof fieldOrConditions === 'object' && fieldOrConditions !== null) {
      Object.entries(fieldOrConditions).forEach(([field, val]) => {
        if (typeof val === 'object' && val !== null && !Array.isArray(val)) {
          // Handle { age: { gt: 18 } }
          Object.entries(val as Record<string, unknown>).forEach(([op, opVal]) => {
            this.addCondition({ field, operator: op as ComparisonOperator, value: opVal });
          });
        } else {
          // Handle { name: 'John' }
          this.addCondition({ field, operator: 'eq', value: val });
        }
      });
      return this;
    }

    // Handle string field forms
    const field = fieldOrConditions as string;
    let operator: ComparisonOperator;
    let finalValue: unknown;
    let finalCollection: string | undefined;

    if (arguments.length === 2) {
      // where('name', 'John') -> assume equality
      operator = 'eq';
      finalValue = operatorOrValue;
    } else if (arguments.length >= 3) {
      // where('age', 'gt', 18) or where('users.name', 'eq', 'John')
      operator = operatorOrValue as ComparisonOperator;
      finalValue = value;
      finalCollection = collection;
    } else {
      throw new Error('Invalid where() arguments');
    }

    return this.addCondition({ field, operator, value: finalValue, collection: finalCollection });
  }

  /**
   * Internal helper to add a condition safely
   */
  private addCondition(condition: FilterCondition): QueryBuilder {
    if (!this.query.where) {
      this.query.where = condition;
    } else {
      this.query.where = this.combineConditions(this.query.where, condition, 'and');
    }
    return this;
  }

  /**
   * Type-safe condition combining for FilterCondition
   */
  private combineConditions(
    existing: WhereClause,
    newCondition: FilterCondition,
    operator: 'and' | 'or'
  ): FilterGroup {
    if ('field' in existing) {
      // Convert single condition to group
      return {
        operator,
        conditions: [existing, newCondition]
      };
    } else {
      // Extend existing group
      if (existing.operator === operator) {
        return {
          operator,
          conditions: [...existing.conditions, newCondition]
        };
      } else {
        // Wrap different operator in new group
        return {
          operator,
          conditions: [existing, newCondition]
        };
      }
    }
  }

  /**
   * Type-safe clause combining for any WhereClause (including FilterGroup)
   */
  private combineWhereClause(
    existing: WhereClause,
    newClause: WhereClause,
    operator: 'and' | 'or'
  ): FilterGroup {
    if ('field' in existing && 'field' in newClause) {
      // Both are FilterCondition
      return {
        operator,
        conditions: [existing, newClause]
      };
    } else if ('field' in existing) {
      // existing is FilterCondition, new is FilterGroup
      return {
        operator,
        conditions: [existing, newClause as FilterGroup]
      };
    } else if ('field' in newClause) {
      // existing is FilterGroup, new is FilterCondition
      if (existing.operator === operator) {
        return {
          operator,
          conditions: [...existing.conditions, newClause]
        };
      } else {
        return {
          operator,
          conditions: [existing, newClause]
        };
      }
    } else {
      // Both are FilterGroup
      if (existing.operator === operator) {
        return {
          operator,
          conditions: [...existing.conditions, newClause as FilterGroup]
        };
      } else {
        return {
          operator,
          conditions: [existing, newClause as FilterGroup]
        };
      }
    }
  }

  /**
   * Add an OR condition
   */
  or(field: string, operator: ComparisonOperator, value: any, collection?: string): QueryBuilder {
    const condition: FilterCondition = { field, operator, value, collection };

    if (!this.query.where) {
      this.query.where = condition;
    } else {
      // Convert to OR group
      if ('field' in this.query.where) {
        this.query.where = {
          operator: 'or',
          conditions: [this.query.where, condition]
        };
      } else if (this.query.where.operator === 'or') {
        const filterGroup = this.query.where as FilterGroup;
        this.query.where = {
          operator: 'or',
          conditions: [
            ...filterGroup.conditions,
            condition
          ]
        };
      } else {
        // Wrap existing AND group in OR
        this.query.where = {
          operator: 'or',
          conditions: [this.query.where, condition]
        };
      }
    }

    return this;
  }

  /**
   * Set the collection to query (instance method for fluent chaining)
   */
  from(collection: string, alias?: string): QueryBuilder {
    this.query.collection = collection;
    if (alias) {
      this.query.alias = alias;
    }
    return this;
  }

  /**
   * Select specific fields
   */
  select(...fields: string[]): QueryBuilder {
    this.query.select = fields;
    return this;
  }

  /**
   * Exclude specific fields
   */
  exclude(...fields: string[]): QueryBuilder {
    this.query.exclude = fields;
    return this;
  }

  /**
   * Add ordering
   */
  orderBy(field: string, direction: 'asc' | 'desc' = 'asc', collection?: string): QueryBuilder {
    if (!this.query.orderBy) {
      this.query.orderBy = [];
    }

    (this.query.orderBy as any[]).push({ field, direction, collection });
    return this;
  }

  /**
   * Set limit
   */
  limit(count: number): QueryBuilder {
    this.query.limit = count;
    return this;
  }

  /**
   * Set offset
   */
  offset(count: number): QueryBuilder {
    this.query.offset = count;
    return this;
  }

  /**
   * Add aggregation
   */
  aggregate(type: AggregationType, field?: string, alias?: string, collection?: string): QueryBuilder {
    if (!this.query.aggregations) {
      this.query.aggregations = [];
    }

    (this.query.aggregations as AggregationDefinition[]).push({
      type, field, alias, collection
    });

    return this;
  }

  /**
   * Count records
   */
  count(field?: string, alias: string = 'count'): QueryBuilder {
    return this.aggregate('count', field, alias);
  }

  /**
   * Group by fields
   */
  groupBy(...fields: string[]): QueryBuilder {
    this.query.groupBy = fields;
    return this;
  }

  /**
   * Enable query explanation
   */
  explain(): QueryBuilder {
    this.query.explain = true;
    return this;
  }

  /**
   * Build the final query
   */
  build(): RelationalQuery {
    if (!this.query.collection) {
      throw new Error('Query must specify a collection');
    }

    return this.query as RelationalQuery;
  }

  /**
   * Convert to legacy StorageQuery format for backward compatibility
   */
  toLegacy(): StorageQuery {
    // Create a mutable version of StorageQuery for building
    const legacyProps: Mutable<StorageQuery> = {
      collection: this.query.collection!
    };

    if (this.query.limit) {
      legacyProps.limit = this.query.limit;
    }
    if (this.query.offset) {
      legacyProps.offset = this.query.offset;
    }

    // Convert advanced where to simple filter (best effort)
    if (this.query.where && 'field' in this.query.where) {
      const condition = this.query.where as FilterCondition;
      if (condition.operator === 'eq') {
        legacyProps.filter = { [condition.field]: condition.value };
      }
    }

    // Convert orderBy
    if (this.query.orderBy) {
      legacyProps.sort = this.query.orderBy.map(o => ({ field: o.field, direction: o.direction }));
    }

    return legacyProps;
  }
}

/**
 * Implementation of fluent field builder
 */
class FieldBuilderImpl implements FieldBuilder {
  constructor(
    private builder: QueryBuilder,
    private fieldName: string,
    private collection?: string
  ) {}

  from(collection: string): ConditionBuilder {
    return new FieldBuilderImpl(this.builder, this.fieldName, collection);
  }

  private createCondition(operator: ComparisonOperator, value: QueryValue): FilterCondition {
    return {
      field: this.fieldName,
      operator,
      value,
      collection: this.collection
    };
  }

  eq(value: QueryValue): FilterCondition {
    return this.createCondition('eq', value);
  }

  ne(value: QueryValue): FilterCondition {
    return this.createCondition('ne', value);
  }

  gt(value: QueryValue): FilterCondition {
    return this.createCondition('gt', value);
  }

  gte(value: QueryValue): FilterCondition {
    return this.createCondition('gte', value);
  }

  lt(value: QueryValue): FilterCondition {
    return this.createCondition('lt', value);
  }

  lte(value: QueryValue): FilterCondition {
    return this.createCondition('lte', value);
  }

  in(values: QueryValue[]): FilterCondition {
    return this.createCondition('in', values);
  }

  nin(values: QueryValue[]): FilterCondition {
    return this.createCondition('nin', values);
  }

  like(pattern: string): FilterCondition {
    return this.createCondition('like', pattern);
  }

  between(min: QueryValue, max: QueryValue): FilterCondition {
    return this.createCondition('between', [min, max]);
  }

  exists(): FilterCondition {
    return this.createCondition('exists', null);
  }

  null(): FilterCondition {
    return this.createCondition('null', null);
  }
}

/**
 * Extension plugins for common query patterns
 */
export const QueryExtensions = {
  /**
   * Discord-like user queries
   */
  forUsers: <T extends QueryBuilder>(builder: T): T & {
    online(): T;
    byType(type: 'human' | 'agent' | 'persona'): T;
    active(since?: Date): T;
    withCapability(capability: string): T;
  } => {
    return Object.assign(builder, {
      online(): T {
        return builder.where('isOnline', true) as T;
      },
      byType(type: 'human' | 'agent' | 'persona'): T {
        return builder.where('userType', type) as T;
      },
      active(since: Date = new Date(Date.now() - 24 * 60 * 60 * 1000)): T {
        return builder.where('lastActiveAt', 'gte', since.toISOString()) as T;
      },
      withCapability(capability: string): T {
        return builder.where('capabilities', 'like', `%${capability}%`) as T;
      }
    });
  },

  /**
   * Discord-like room queries
   */
  forRooms: <T extends QueryBuilder>(builder: T): T & {
    public(): T;
    private(): T;
    hasMembers(count?: number): T;
    activeToday(): T;
  } => {
    return Object.assign(builder, {
      public(): T {
        return builder.where('type', 'public') as T;
      },
      private(): T {
        return builder.where('type', 'private') as T;
      },
      hasMembers(count: number = 1): T {
        return builder.where('memberCount', 'gte', count) as T;
      },
      activeToday(): T {
        const today = new Date().toISOString().split('T')[0];
        return builder.where('lastActivity', 'gte', today) as T;
      }
    });
  },

  /**
   * Time-based queries
   */
  temporal: <T extends QueryBuilder>(builder: T): T & {
    recent(hours?: number): T;
    today(): T;
    thisWeek(): T;
    createdAfter(date: Date): T;
    updatedSince(date: Date): T;
  } => {
    return Object.assign(builder, {
      recent(hours: number = 1): T {
        const since = new Date(Date.now() - hours * 60 * 60 * 1000);
        return builder.where('createdAt', 'gte', since.toISOString()) as T;
      },
      today(): T {
        const today = new Date().toISOString().split('T')[0];
        return builder.where('createdAt', 'gte', today) as T;
      },
      thisWeek(): T {
        const weekStart = new Date();
        weekStart.setDate(weekStart.getDate() - weekStart.getDay());
        return builder.where('createdAt', 'gte', weekStart.toISOString()) as T;
      },
      createdAfter(date: Date): T {
        return builder.where('createdAt', 'gte', date.toISOString()) as T;
      },
      updatedSince(date: Date): T {
        return builder.where('updatedAt', 'gte', date.toISOString()) as T;
      }
    });
  }
};

/**
 * Utility functions for query operations
 */
export class QueryUtils {

  /**
   * Check if two values match based on operator
   */
  static matchesCondition(value: any, condition: FilterCondition): boolean {
    const { operator, value: conditionValue } = condition;

    switch (operator) {
      case 'eq': return value === conditionValue;
      case 'ne': return value !== conditionValue;
      case 'gt': return value > conditionValue;
      case 'gte': return value >= conditionValue;
      case 'lt': return value < conditionValue;
      case 'lte': return value <= conditionValue;
      case 'in': return Array.isArray(conditionValue) && conditionValue.includes(value);
      case 'nin': return Array.isArray(conditionValue) && !conditionValue.includes(value);
      case 'like': return typeof value === 'string' && value.includes(conditionValue);
      case 'regex': return new RegExp(conditionValue).test(String(value));
      case 'exists': return value !== undefined && value !== null;
      case 'null': return value === null || value === undefined;
      case 'between':
        return Array.isArray(conditionValue) &&
               conditionValue.length === 2 &&
               value >= conditionValue[0] &&
               value <= conditionValue[1];
      default:
        return false;
    }
  }

  /**
   * Evaluate a filter group or condition
   */
  static evaluateFilter(record: any, filter: FilterGroup | FilterCondition): boolean {
    if ('field' in filter) {
      // Single condition
      const value = this.getNestedValue(record, filter.field);
      return this.matchesCondition(value, filter);
    } else {
      // Group of conditions
      const { operator, conditions } = filter;

      if (operator === 'and') {
        return conditions.every(condition => this.evaluateFilter(record, condition));
      } else if (operator === 'or') {
        return conditions.some(condition => this.evaluateFilter(record, condition));
      }

      return false;
    }
  }

  /**
   * Get nested value from object using dot notation
   */
  private static getNestedValue(obj: any, path: string): any {
    return path.split('.').reduce((current, key) => current?.[key], obj);
  }
}