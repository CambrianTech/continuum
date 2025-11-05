/**
 * Field Decorators - Entity Field Metadata System
 *
 * Decorators define field storage requirements in an intermediate language
 * Adapters iterate over decorator metadata to implement storage
 */

export type FieldType = 'primary' | 'foreign_key' | 'date' | 'enum' | 'text' | 'json' | 'number' | 'boolean';

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
  };
}

// Global field metadata storage
const FIELD_METADATA = new Map<EntityConstructor, Map<string, FieldMetadata>>();

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
export function DateField(options?: { index?: boolean; nullable?: boolean }) {
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
export function TextField(options?: { maxLength?: number; index?: boolean; nullable?: boolean; unique?: boolean }) {
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
 */
export function JsonField(options?: { nullable?: boolean }) {
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
 * Number field
 */
export function NumberField(options?: { nullable?: boolean; default?: number }) {
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