/**
 * Field Decorators - Entity Field Metadata System
 *
 * Decorators define field storage requirements in an intermediate language
 * Adapters iterate over decorator metadata to implement storage
 */

export type FieldType = 'primary' | 'foreign_key' | 'date' | 'enum' | 'text' | 'json' | 'blob' | 'number' | 'boolean';

/**
 * Text length constants
 */
export const TEXT_LENGTH = {
  UNLIMITED: 0,        // Use 0 to indicate unlimited/LONGTEXT
  DEFAULT: 256,        // Standard varchar length
  SHORT: 30,          // For short descriptions in UI
  MEDIUM: 128,        // Medium text fields
  LONG: 1024          // Long text fields
} as const;

/**
 * Type for entity constructor function
 */
type EntityConstructor = new (...args: unknown[]) => unknown;

/**
 * Type for objects with constructor property
 */
type ConstructorInstance = { constructor: EntityConstructor };

export interface FieldMetadata {
  fieldName: string;
  fieldType: FieldType;
  options?: {
    references?: string;        // For foreign keys: 'User.userId'
    index?: boolean;
    unique?: boolean;
    nullable?: boolean;
    default?: any;
    maxLength?: number;
    description?: boolean;      // Mark this field as the entity's description (for data/list summaries)
    summary?: boolean;          // Mark field for inclusion in data/list results (default: false)
    /**
     * For JSON fields: automatically store as blob if size exceeds threshold (bytes)
     * Default: undefined (no blob storage)
     * Recommended: 4096 (4KB) for large JSON objects like RAG context
     */
    blobThreshold?: number;
    /**
     * Field to store blob reference when data is externalized
     * Must be a companion TextField on the same entity
     * Example: ragContext + ragContextRef pair
     */
    blobRefField?: string;
  };
}

/**
 * Composite Index Definition
 * Defines indexes on multiple columns for query optimization
 */
export interface CompositeIndexMetadata {
  name: string;                 // Index name (e.g., 'idx_room_timestamp')
  fields: string[];             // Column names in order
  unique?: boolean;             // UNIQUE constraint
  direction?: 'ASC' | 'DESC';   // Sort direction (applies to last field)
}

/**
 * Archive Configuration
 * Defines automatic data movement between handles (archiving, migration, etc.)
 *
 * Uses two-handle pattern with IDENTICAL table names:
 * - Both handles point to the same collection name
 * - Different storage locations (databases, adapters, etc.)
 * - UUIDs prevent conflicts when merging
 * - Flexible: archive, migrate, restore, merge
 */
export interface ArchiveConfig {
  sourceHandle: string;         // Source handle identifier (e.g., 'primary', 'local', 'sqlite-main')
  destHandle: string;           // Destination handle identifier (e.g., 'archive', 'postgres-prod')
  maxRows: number;              // Max rows in source before archiving (e.g., 10000)
  rowsPerArchive: number;       // Rows to move per operation (e.g., 1000)
  maxArchiveFileRows: number;   // Max rows per archive file before creating new one (e.g., 100000)
  orderByField: string;         // Field to order by for oldest-first movement (e.g., 'timestamp')
}

// Global field metadata storage
const FIELD_METADATA = new Map<EntityConstructor, Map<string, FieldMetadata>>();

// Global composite index metadata storage
const COMPOSITE_INDEXES = new Map<EntityConstructor, CompositeIndexMetadata[]>();

// Global archive configuration storage
const ARCHIVE_CONFIGS = new Map<EntityConstructor, ArchiveConfig>();

/**
 * Get composite indexes for an entity class
 */
export function getCompositeIndexes(entityClass: EntityConstructor): CompositeIndexMetadata[] {
  const indexes: CompositeIndexMetadata[] = [];

  // Walk up the prototype chain to collect inherited indexes
  let currentClass = entityClass;
  while (currentClass) {
    const classIndexes = COMPOSITE_INDEXES.get(currentClass);
    if (classIndexes) {
      indexes.push(...classIndexes);
    }

    // Move to parent class
    const parent = Object.getPrototypeOf(currentClass);
    currentClass = parent === Function.prototype ? null : parent;
  }

  return indexes;
}

/**
 * Add composite index metadata for an entity class
 */
function addCompositeIndex(constructor: EntityConstructor, index: CompositeIndexMetadata) {
  if (!COMPOSITE_INDEXES.has(constructor)) {
    COMPOSITE_INDEXES.set(constructor, []);
  }
  COMPOSITE_INDEXES.get(constructor)!.push(index);
}

/**
 * Get field metadata for an entity class, including inherited fields
 */
export function getFieldMetadata(entityClass: EntityConstructor): Map<string, FieldMetadata> {
  const allMetadata = new Map<string, FieldMetadata>();

  // Walk up the prototype chain to collect inherited field metadata
  let currentClass = entityClass;
  while (currentClass) {
    const classMetadata = FIELD_METADATA.get(currentClass);
    if (classMetadata) {
      // Add parent class metadata first (so child can override)
      for (const [fieldName, metadata] of classMetadata) {
        if (!allMetadata.has(fieldName)) {
          allMetadata.set(fieldName, metadata);
        }
      }
    }

    // Move to parent class
    const parent = Object.getPrototypeOf(currentClass);
    currentClass = parent === Function.prototype ? null : parent;
  }

  return allMetadata;
}

/**
 * Check if entity has field metadata
 */
export function hasFieldMetadata(entityClass: EntityConstructor): boolean {
  return FIELD_METADATA.has(entityClass) && FIELD_METADATA.get(entityClass)!.size > 0;
}

/**
 * Get the description field name for an entity (for data/list summaries)
 * Returns the field name marked with description:true, or null if none found
 */
export function getDescriptionField(entityClass: EntityConstructor): string | null {
  const metadata = getFieldMetadata(entityClass);
  for (const [fieldName, fieldMetadata] of metadata) {
    if (fieldMetadata.options?.description === true) {
      return fieldName;
    }
  }
  return null;
}

/**
 * Add field metadata for a property
 * In Stage 3 decorators, we get the class from context, not target
 */
function addFieldMetadata(constructor: EntityConstructor, fieldName: string, metadata: FieldMetadata) {
  if (!FIELD_METADATA.has(constructor)) {
    FIELD_METADATA.set(constructor, new Map());
  }
  FIELD_METADATA.get(constructor)!.set(fieldName, metadata);
}

/**
 * Primary key field
 */
export function PrimaryField(options?: { unique?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);

    // In Stage 3 decorators, we use addInitializer to defer metadata storage
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'primary',
        options: { unique: true, nullable: false, ...options }
      });
    });
  };
}

/**
 * Foreign key field
 */
export function ForeignKeyField(options: { references: string; index?: boolean; nullable?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'foreign_key',
        options: { index: true, nullable: false, ...options }
      });
    });
  };
}

/**
 * Date field (auto-converts Date ↔ ISO string)
 */
export function DateField(options?: { index?: boolean; nullable?: boolean; summary?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'date',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * Enum field
 */
export function EnumField(options?: { index?: boolean; nullable?: boolean; default?: unknown }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'enum',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * Text field
 */
export function TextField(options?: { maxLength?: number; index?: boolean; nullable?: boolean; unique?: boolean; description?: boolean; summary?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'text',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * JSON field for complex objects
 *
 * Supports automatic blob storage for large objects:
 * @example
 * ```typescript
 * // Auto-externalize JSON > 4KB to blob storage
 * @JsonField({ nullable: true, blobThreshold: 4096, blobRefField: 'ragContextRef' })
 * ragContext?: RAGContext;
 *
 * @TextField({ nullable: true })
 * ragContextRef?: string;  // Stores blob hash when externalized
 * ```
 */
export function JsonField(options?: {
  nullable?: boolean;
  /** Auto-externalize to blob storage if size exceeds threshold (bytes) */
  blobThreshold?: number;
  /** Companion field to store blob hash reference */
  blobRefField?: string;
}) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'json',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * Blob field for large binary or JSON data stored externally
 *
 * Data is automatically:
 * - Compressed with gzip
 * - Stored content-addressably (SHA256 hash)
 * - Deduplicated (same content = same blob)
 *
 * The field stores the blob hash reference, not the actual data.
 * Use BlobStorage.retrieve(hash) to fetch the data.
 *
 * @example
 * ```typescript
 * @BlobField()
 * largeDataRef?: string;  // Stores "sha256:abc123..."
 * ```
 */
export function BlobField(options?: { nullable?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'blob',
        options: { nullable: true, ...options }  // Usually nullable since data may be inline
      });
    });
  };
}

/**
 * Number field
 */
export function NumberField(options?: { nullable?: boolean; default?: number; summary?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'number',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * Boolean field
 */
export function BooleanField(options?: { nullable?: boolean; default?: boolean }) {
  return function (target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      addFieldMetadata((this as ConstructorInstance).constructor, fieldName, {
        fieldName,
        fieldType: 'boolean',
        options: { nullable: false, ...options }
      });
    });
  };
}

/**
 * Composite Index Decorator (Class-level)
 *
 * Defines multi-column indexes for query optimization.
 * Use when queries filter/sort on multiple columns together.
 *
 * @example
 * ```typescript
 * @CompositeIndex({
 *   name: 'idx_room_timestamp',
 *   fields: ['roomId', 'timestamp'],
 *   direction: 'DESC'
 * })
 * export class ChatMessageEntity extends BaseEntity {
 *   @TextField({ index: true })
 *   roomId: UUID;
 *
 *   @DateField({ index: true })
 *   timestamp: Date;
 * }
 * ```
 *
 * Generated SQL:
 * CREATE INDEX IF NOT EXISTS idx_room_timestamp
 * ON chat_messages(room_id, timestamp DESC);
 */
export function CompositeIndex(index: CompositeIndexMetadata) {
  return function <T extends EntityConstructor>(target: T) {
    // Add composite index metadata to the class constructor
    addCompositeIndex(target, index);
    return target;
  };
}

/**
 * Get archive configuration for an entity class
 */
export function getArchiveConfig(entityClass: EntityConstructor): ArchiveConfig | null {
  return ARCHIVE_CONFIGS.get(entityClass) || null;
}

/**
 * Archive Decorator (Class-level)
 *
 * Defines automatic data movement between handles using the two-handle pattern.
 * The ArchiveDaemon reads from source handle and writes to destination handle.
 *
 * Both handles use IDENTICAL collection names - only storage location differs.
 * This enables: archiving, migration, restoration, merging archives, etc.
 *
 * @example Archive to separate database:
 * ```typescript
 * @Archive({
 *   sourceHandle: 'primary',    // SQLite main.db
 *   destHandle: 'archive',      // SQLite archive.db
 *   maxRows: 10000,             // Archive when source exceeds 10k rows
 *   rowsPerArchive: 1000,       // Move 1000 rows per batch
 *   orderByField: 'timestamp'   // Oldest first
 * })
 * export class ChatMessageEntity extends BaseEntity {
 *   static readonly collection = 'chat_messages';  // Same name in BOTH handles!
 *   @DateField({ index: true })
 *   timestamp: Date;
 * }
 * ```
 *
 * @example Migrate to Postgres:
 * ```typescript
 * @Archive({
 *   sourceHandle: 'sqlite-local',
 *   destHandle: 'postgres-prod',  // Different adapter!
 *   maxRows: 10000,
 *   rowsPerArchive: 1000,
 *   orderByField: 'timestamp'
 * })
 * ```
 *
 * Process:
 * 1. Open source handle (e.g., SQLite) → chat_messages
 * 2. Open dest handle (e.g., Postgres) → chat_messages (SAME name!)
 * 3. Read oldest rows from source
 * 4. Write to dest, verify, then delete from source
 * 5. UUIDs prevent conflicts, foreign keys may break (acceptable)
 */
export function Archive(config: ArchiveConfig) {
  return function <T extends EntityConstructor>(target: T) {
    // Add archive config metadata to the class constructor
    ARCHIVE_CONFIGS.set(target, config);
    return target;
  };
}