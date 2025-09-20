/**
 * BaseORM - Professional ORM built on DataDaemon infrastructure
 *
 * Integrates with existing BaseUser hierarchy and UserRepository patterns.
 * Provides real relationships, foreign keys, and type-safe operations while
 * leveraging the proven DataDaemon adapter system.
 *
 * Architecture Integration:
 * - Works WITH existing BaseUser/HumanUser/AIUser classes
 * - Leverages existing UserRepository + DataDaemon pattern
 * - Extends existing UserRelationships with proper foreign keys
 * - Maintains backward compatibility with current domain objects
 */

import { DataDaemon, type DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';
import type { StorageResult, StorageQuery } from '../../daemons/data-daemon/shared/DataStorageAdapter';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * Base Entity interface - all ORM entities must extend this
 */
export interface BaseEntity {
    readonly id: UUID;
    readonly createdAt: Date;
    readonly updatedAt: Date;
}

// Import existing domain objects
import type { BaseUser, BaseUserData } from '../../domain/user/BaseUser';
import type {
    UserSession,
    UserPermission,
    RoomParticipation,
    ChatRoom,
    ChatMessage,
    UserRelationship,
    MessageReaction,
    TypingIndicator,
    UserPresence,
    UserProfile
} from '../../domain/user/UserRelationships';

/**
 * ORM Entity Metadata - Describes table structure and relationships
 */
export interface EntityMetadata {
    readonly tableName: string;
    readonly primaryKey: string;
    readonly columns: Record<string, {
        type: 'string' | 'number' | 'boolean' | 'date' | 'json' | 'uuid';
        nullable?: boolean;
        unique?: boolean;
        default?: any;
        length?: number;
    }>;
    readonly relationships: Record<string, {
        type: 'oneToOne' | 'oneToMany' | 'manyToOne' | 'manyToMany';
        targetEntity: string;
        foreignKey?: string;
        joinTable?: string;
        cascade?: boolean;
    }>;
    readonly indexes: Array<{
        name: string;
        columns: string[];
        unique?: boolean;
    }>;
    readonly constraints: Array<{
        type: 'foreignKey' | 'unique' | 'check';
        columns: string[];
        references?: { table: string; columns: string[] };
        condition?: string;
    }>;
}

/**
 * Type-safe Query Builder
 */
export class ORMQueryBuilder<T> {
    private conditions: Array<{ field: string; operator: string; value: any }> = [];
    private joins: Array<{ type: 'LEFT' | 'INNER'; table: string; on: string }> = [];
    private ordering: Array<{ field: string; direction: 'asc' | 'desc' }> = [];
    private limitCount?: number;
    private offsetCount?: number;

    constructor(private tableName: string) {}

    where(field: string, operator: '=' | '!=' | '>' | '<' | '>=' | '<=' | 'LIKE' | 'IN', value: any): this {
        this.conditions.push({ field, operator, value });
        return this;
    }

    leftJoin(table: string, localKey: string, foreignKey: string): this {
        this.joins.push({
            type: 'LEFT',
            table,
            on: `${this.tableName}.${localKey} = ${table}.${foreignKey}`
        });
        return this;
    }

    innerJoin(table: string, localKey: string, foreignKey: string): this {
        this.joins.push({
            type: 'INNER',
            table,
            on: `${this.tableName}.${localKey} = ${table}.${foreignKey}`
        });
        return this;
    }

    orderBy(field: string, direction: 'asc' | 'desc' = 'asc'): this {
        this.ordering.push({ field, direction });
        return this;
    }

    limit(count: number): this {
        this.limitCount = count;
        return this;
    }

    offset(count: number): this {
        this.offsetCount = count;
        return this;
    }

    /**
     * Convert to DataDaemon StorageQuery
     */
    toStorageQuery(): StorageQuery {
        const filters: Record<string, any> = {};

        // Convert conditions to DataDaemon filters
        for (const condition of this.conditions) {
            if (condition.operator === '=') {
                filters[condition.field] = condition.value;
            } else if (condition.operator === '!=') {
                filters[condition.field] = { $ne: condition.value };
            } else if (condition.operator === '>') {
                filters[condition.field] = { $gt: condition.value };
            } else if (condition.operator === '<') {
                filters[condition.field] = { $lt: condition.value };
            } else if (condition.operator === '>=') {
                filters[condition.field] = { $gte: condition.value };
            } else if (condition.operator === '<=') {
                filters[condition.field] = { $lte: condition.value };
            } else if (condition.operator === 'LIKE') {
                filters[condition.field] = { $regex: condition.value };
            } else if (condition.operator === 'IN') {
                filters[condition.field] = { $in: condition.value };
            }
        }

        return {
            collection: this.tableName,
            filters,
            sort: this.ordering.map(o => ({ field: o.field, direction: o.direction })),
            limit: this.limitCount,
            offset: this.offsetCount
        };
    }

    /**
     * Generate SQL for SQL-based DataDaemon adapters
     */
    toSQL(): { sql: string; params: any[] } {
        let sql = `SELECT * FROM ${this.tableName}`;
        const params: any[] = [];

        // Add JOINs
        for (const join of this.joins) {
            sql += ` ${join.type} JOIN ${join.table} ON ${join.on}`;
        }

        // Add WHERE clause
        if (this.conditions.length > 0) {
            const whereConditions = this.conditions.map(c => {
                params.push(c.value);
                return `${c.field} ${c.operator} ?`;
            });
            sql += ` WHERE ${whereConditions.join(' AND ')}`;
        }

        // Add ORDER BY
        if (this.ordering.length > 0) {
            sql += ` ORDER BY ${this.ordering.map(o => `${o.field} ${o.direction.toUpperCase()}`).join(', ')}`;
        }

        // Add LIMIT/OFFSET
        if (this.limitCount !== undefined) {
            sql += ` LIMIT ${this.limitCount}`;
        }
        if (this.offsetCount !== undefined) {
            sql += ` OFFSET ${this.offsetCount}`;
        }

        return { sql, params };
    }
}

/**
 * Base Entity Repository - Works with DataDaemon
 */
export abstract class EntityRepository<T extends BaseEntity> {
    protected dataDaemon: DataDaemon;
    protected metadata: EntityMetadata;

    constructor(dataDaemon: DataDaemon, metadata: EntityMetadata) {
        this.dataDaemon = dataDaemon;
        this.metadata = metadata;
    }

    /**
     * Generate UUID using cross-platform UUID generator
     */
    protected generateId(): UUID {
        return generateUUID();
    }

    /**
     * Convert clean entity to storage format
     * Handles the DataDaemon's internal structure requirements
     */
    protected toStorageFormat(entity: T): Record<string, unknown> {
        const result = { ...(entity as Record<string, unknown>) };

        // Convert dates to ISO strings for storage (only if they exist)
        if (entity.createdAt instanceof Date) {
            result.createdAt = entity.createdAt.toISOString();
        }
        if (entity.updatedAt instanceof Date) {
            result.updatedAt = entity.updatedAt.toISOString();
        }

        return result;
    }

    /**
     * Convert storage format back to clean entity
     * Handles the DataRecord wrapper and type conversions
     */
    protected fromStorageFormat(dataRecord: import('../../daemons/data-daemon/shared/DataStorageAdapter').DataRecord<unknown>): T {
        const rawData = dataRecord.data as Record<string, unknown>;

        return {
            ...rawData,
            // Convert ISO strings back to Date objects
            createdAt: new Date(rawData.createdAt as string),
            updatedAt: new Date(rawData.updatedAt as string)
        } as T;
    }

    /**
     * Create entity
     */
    async create(
        data: Omit<T, 'id' | 'createdAt' | 'updatedAt'> & Partial<Pick<T, 'id'>>,
        context: DataOperationContext
    ): Promise<StorageResult<T>> {
        const entity: T = {
            ...data,
            id: data.id ?? this.generateId(),
            createdAt: new Date(),
            updatedAt: new Date()
        } as T;

        // Convert to storage format before sending to DataDaemon
        const storageData = this.toStorageFormat(entity);

        // USE NEW CLEAN INTERFACE - DataDaemon.store() with auto-context
        const result = await DataDaemon.store(this.metadata.tableName, storageData, entity.id);

        if (!result.success || !result.data) {
            return {
                success: false,
                error: result.error || 'Failed to create entity'
            };
        }

        // Extract clean entity from storage wrapper
        const cleanEntity = this.fromStorageFormat(result.data);
        return {
            success: true,
            data: cleanEntity
        };
    }

    /**
     * Find by ID
     */
    async findById(id: UUID, context: DataOperationContext): Promise<StorageResult<T | null>> {
        // USE NEW CLEAN INTERFACE - DataDaemon.read() with auto-context
        const result = await DataDaemon.read(this.metadata.tableName, id);

        if (!result.success) {
            return {
                success: false,
                error: result.error || 'Failed to read entity'
            };
        }

        if (!result.data) {
            return { success: true, data: null };
        }

        // Extract clean entity from storage wrapper
        const cleanEntity = this.fromStorageFormat(result.data);
        return {
            success: true,
            data: cleanEntity
        };
    }

    /**
     * Update entity
     */
    async update(id: UUID, updates: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>> {
        const updateEntity = {
            ...updates,
            updatedAt: new Date()
        } as Partial<T>;

        // Convert to storage format
        const storageData = this.toStorageFormat(updateEntity as T);
        const result = await this.dataDaemon.update(this.metadata.tableName, id, storageData, context);

        if (!result.success || !result.data) {
            return {
                success: false,
                error: result.error || 'Failed to update entity'
            };
        }

        // Extract clean entity from storage wrapper
        const cleanEntity = this.fromStorageFormat(result.data);
        return {
            success: true,
            data: cleanEntity
        };
    }

    /**
     * Delete entity
     */
    async delete(id: UUID, context: DataOperationContext): Promise<StorageResult<void>> {
        const result = await this.dataDaemon.delete(this.metadata.tableName, id, context);
        if (!result.success) {
            return { success: false, error: result.error };
        }
        return { success: true, data: undefined };
    }

    /**
     * Query with builder
     */
    async query(queryBuilder: ORMQueryBuilder<T>, context: DataOperationContext): Promise<StorageResult<T[]>> {
        const storageQuery = queryBuilder.toStorageQuery();
        // USE NEW CLEAN INTERFACE - DataDaemon.query() with auto-context
        const result = await DataDaemon.query(storageQuery);

        if (!result.success || !result.data) {
            return {
                success: false,
                error: result.error || 'Failed to query entities'
            };
        }

        return {
            success: true,
            data: result.data.map(record => this.fromStorageFormat(record))
        };
    }

    /**
     * Load related entities
     */
    async loadRelation<R>(
        entityId: UUID,
        relationName: string,
        context: DataOperationContext
    ): Promise<StorageResult<R[]>> {
        const relationship = this.metadata.relationships[relationName];
        if (!relationship) {
            return {
                success: false,
                error: `Relationship '${relationName}' not found on ${this.metadata.tableName}`
            };
        }

        let query: StorageQuery;

        if (relationship.type === 'oneToMany') {
            // Foreign key is on the target table
            query = {
                collection: relationship.targetEntity,
                filters: { [relationship.foreignKey || 'id']: entityId }
            };
        } else if (relationship.type === 'manyToOne') {
            // Foreign key is on this table, load the related entity
            query = {
                collection: relationship.targetEntity,
                filters: { id: entityId }
            };
        } else if (relationship.type === 'manyToMany') {
            // Use join table
            const joinTable = relationship.joinTable;
            if (!joinTable) {
                return {
                    success: false,
                    error: `Many-to-many relationship '${relationName}' missing joinTable`
                };
            }

            // This would need more complex join logic
            // For now, return empty array
            return {
                success: true,
                data: [] as R[]
            };
        }

        const result = await this.dataDaemon.query(query!, context);

        if (!result.success || !result.data) {
            return result as StorageResult<R[]>;
        }

        return {
            success: true,
            data: result.data.map(record => record.data as R) // Relations can have different types
        };
    }

    /**
     * Create query builder
     */
    createQuery(): ORMQueryBuilder<T> {
        return new ORMQueryBuilder<T>(this.metadata.tableName);
    }
}

/**
 * ORM Manager - Coordinates all repositories and metadata
 */
export class ORM {
    private repositories: Map<string, EntityRepository<any>> = new Map();
    private metadata: Map<string, EntityMetadata> = new Map();

    constructor(private dataDaemon: DataDaemon) {}

    /**
     * Register entity metadata
     */
    registerEntity(name: string, metadata: EntityMetadata): void {
        this.metadata.set(name, metadata);
    }

    /**
     * Register repository
     */
    registerRepository<T extends BaseEntity>(name: string, repository: EntityRepository<T>): void {
        this.repositories.set(name, repository);
    }

    /**
     * Get repository
     */
    getRepository<T extends BaseEntity>(name: string): EntityRepository<T> {
        const repo = this.repositories.get(name);
        if (!repo) {
            throw new Error(`Repository '${name}' not registered`);
        }
        return repo;
    }

    /**
     * Generate database schema DDL
     */
    async generateSchema(context: DataOperationContext): Promise<StorageResult<string[]>> {
        const ddlStatements: string[] = [];

        for (const [entityName, metadata] of this.metadata) {
            // CREATE TABLE
            let createTable = `CREATE TABLE IF NOT EXISTS ${metadata.tableName} (\n`;

            const columnDefs: string[] = [];
            for (const [columnName, columnDef] of Object.entries(metadata.columns)) {
                let colDef = `  ${columnName}`;

                switch (columnDef.type) {
                    case 'uuid': colDef += ' UUID'; break;
                    case 'string': colDef += ` VARCHAR(${columnDef.length || 255})`; break;
                    case 'number': colDef += ' INTEGER'; break;
                    case 'boolean': colDef += ' BOOLEAN'; break;
                    case 'date': colDef += ' TIMESTAMP'; break;
                    case 'json': colDef += ' JSONB'; break;
                }

                if (!columnDef.nullable) colDef += ' NOT NULL';
                if (columnDef.default) colDef += ` DEFAULT ${columnDef.default}`;
                if (columnName === metadata.primaryKey) colDef += ' PRIMARY KEY';

                columnDefs.push(colDef);
            }

            createTable += columnDefs.join(',\n') + '\n);';
            ddlStatements.push(createTable);

            // CREATE INDEXES
            for (const index of metadata.indexes) {
                const indexDDL = `CREATE ${index.unique ? 'UNIQUE ' : ''}INDEX IF NOT EXISTS ${index.name} ON ${metadata.tableName} (${index.columns.join(', ')});`;
                ddlStatements.push(indexDDL);
            }

            // CREATE CONSTRAINTS
            for (const constraint of metadata.constraints) {
                if (constraint.type === 'foreignKey' && constraint.references) {
                    const fkDDL = `ALTER TABLE ${metadata.tableName} ADD CONSTRAINT fk_${metadata.tableName}_${constraint.columns.join('_')} FOREIGN KEY (${constraint.columns.join(', ')}) REFERENCES ${constraint.references.table} (${constraint.references.columns.join(', ')});`;
                    ddlStatements.push(fkDDL);
                }
            }
        }

        return {
            success: true,
            data: ddlStatements
        };
    }
}