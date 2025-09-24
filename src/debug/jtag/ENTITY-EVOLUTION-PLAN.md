# ENTITY EVOLUTION & MIGRATION SYSTEM

## 🎯 **THE CHALLENGE**

**Current Problem**: Entity schema changes break the system because:
1. **Database constraints** (NOT NULL) don't match entity definitions (optional fields)
2. **No migration system** - adding/changing fields requires manual intervention
3. **Schema drift** - entities and database get out of sync over time
4. **Production deployment** - can't just delete and recreate tables

**Example Issue**:
```typescript
// RoomEntity.ts:78
@DateField({ index: true })
lastMessageAt?: Date;  // ❌ Optional in entity, but NOT NULL in database
```

## 🚀 **PROPOSED: RUST-INSPIRED ENTITY EVOLUTION**

### **The Vision**: Automatic schema migration with decorator-driven defaults

**Rust Inspiration**:
- `serde` handles missing fields gracefully with `#[serde(default)]`
- Database migrations in Rust (Diesel) are code-driven and type-safe
- Schema evolution is explicit and traceable

**Our Evolution**:
- **Decorator-driven defaults** prevent constraint failures
- **Automatic migrations** when fields are added/changed
- **Version tracking** for schema evolution
- **Backwards compatibility** with old entity versions

## 🏗️ **IMPLEMENTATION ARCHITECTURE**

### **Phase 1: Enhanced Field Decorators**

#### **1.1 Migration-Aware Decorators**
```typescript
// File: system/data/decorators/MigrationDecorators.ts

export interface FieldOptions {
  // Existing options
  index?: boolean;
  nullable?: boolean;
  references?: string;

  // NEW: Migration options
  default?: any | (() => any);
  migrationStrategy?: 'add' | 'rename' | 'delete' | 'transform';
  previousName?: string;
  addedInVersion?: string;
  removedInVersion?: string;
}

export function DateField(options?: FieldOptions) {
  return function(target: any, propertyKey: string) {
    // Store enhanced metadata
    Reflect.defineMetadata('field:type', 'date', target, propertyKey);
    Reflect.defineMetadata('field:options', options || {}, target, propertyKey);
    Reflect.defineMetadata('field:migration', {
      strategy: options?.migrationStrategy || 'add',
      default: options?.default,
      version: options?.addedInVersion
    }, target, propertyKey);
  };
}
```

#### **1.2 Fixed Room Entity with Migration Info**
```typescript
export class RoomEntity extends BaseEntity {
  static readonly collection = 'Room';
  static readonly version = '1.1.0'; // NEW: Schema version tracking

  @TextField({
    index: true,
    default: () => `Room_${Date.now()}` // Automatic meaningful names
  })
  name: string;

  @TextField({
    default: () => 'New Room'
  })
  displayName: string;

  @TextField({
    nullable: true,
    default: null
  })
  description?: string;

  @EnumField({
    index: true,
    default: 'public'
  })
  type: RoomType;

  @EnumField({
    index: true,
    default: 'active'
  })
  status: RoomStatus;

  @ForeignKeyField({
    references: 'users.id',
    index: true,
    // This should be required - who creates the room
  })
  ownerId: UUID;

  @DateField({
    index: true,
    default: () => new Date(), // FIX: Prevents NOT NULL constraint!
    migrationStrategy: 'add',
    addedInVersion: '1.1.0'
  })
  lastMessageAt: Date; // No longer optional

  @JsonField({
    default: () => ({
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: false,
      searchable: true
    })
  })
  privacy: RoomPrivacy;

  @JsonField({
    default: () => ({
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    })
  })
  settings: RoomSettings;

  @JsonField({
    default: () => []
  })
  members: readonly RoomMember[];

  @JsonField({
    default: () => []
  })
  tags: readonly string[];
}
```

### **Phase 2: Automatic Migration System**

#### **2.1 Migration Engine**
```typescript
// File: system/data/migrations/MigrationEngine.ts

export interface MigrationStep {
  version: string;
  entity: string;
  field: string;
  action: 'add' | 'rename' | 'delete' | 'transform';
  sqlCommand: string;
  defaultValue?: any;
}

export class MigrationEngine {
  /**
   * Analyze entity differences and generate migration steps
   * Rust equivalent: Diesel's schema diff generation
   */
  static async generateMigration(
    entityClass: typeof BaseEntity,
    currentDbSchema: DatabaseSchema
  ): Promise<MigrationStep[]> {
    const entityMetadata = this.extractEntityMetadata(entityClass);
    const dbColumns = currentDbSchema.getColumns(entityClass.collection);

    const migrations: MigrationStep[] = [];

    // Check for new fields that need to be added
    for (const [field, metadata] of entityMetadata) {
      if (!dbColumns.has(field)) {
        migrations.push({
          version: entityClass.version,
          entity: entityClass.collection,
          field: field,
          action: 'add',
          sqlCommand: this.generateAddColumnSQL(field, metadata),
          defaultValue: metadata.default
        });
      }
    }

    // Check for renamed fields
    for (const [field, metadata] of entityMetadata) {
      if (metadata.previousName && dbColumns.has(metadata.previousName)) {
        migrations.push({
          version: entityClass.version,
          entity: entityClass.collection,
          field: field,
          action: 'rename',
          sqlCommand: `ALTER TABLE ${entityClass.collection.toLowerCase()} RENAME COLUMN ${metadata.previousName} TO ${field}`,
        });
      }
    }

    return migrations;
  }

  /**
   * Execute migration steps safely
   */
  static async executeMigrations(steps: MigrationStep[]): Promise<void> {
    console.log(`🔄 Executing ${steps.length} migration steps...`);

    for (const step of steps) {
      try {
        // Execute SQL command
        await DatabaseAdapter.executeSQL(step.sqlCommand);

        // If adding field with default, populate existing rows
        if (step.action === 'add' && step.defaultValue !== undefined) {
          const defaultVal = typeof step.defaultValue === 'function'
            ? step.defaultValue()
            : step.defaultValue;

          await DatabaseAdapter.executeSQL(
            `UPDATE ${step.entity.toLowerCase()} SET ${step.field} = ? WHERE ${step.field} IS NULL`,
            [defaultVal]
          );
        }

        console.log(`✅ Migration: ${step.action} ${step.entity}.${step.field}`);

      } catch (error) {
        console.error(`❌ Migration failed: ${step.action} ${step.entity}.${step.field}`, error);
        throw error;
      }
    }
  }
}
```

#### **2.2 Startup Migration Check**
```typescript
// File: system/data/migrations/AutoMigration.ts

export class AutoMigration {
  /**
   * Check for and execute needed migrations on system startup
   * Like Rust's embed_migrations! but automatic
   */
  static async checkAndMigrate(): Promise<void> {
    console.log('🔍 Checking for entity schema changes...');

    const entityClasses = EntityRegistry.getAllEntityClasses();

    for (const entityClass of entityClasses) {
      const currentSchema = await DatabaseAdapter.getSchema(entityClass.collection);
      const neededMigrations = await MigrationEngine.generateMigration(
        entityClass,
        currentSchema
      );

      if (neededMigrations.length > 0) {
        console.log(`🔄 Found ${neededMigrations.length} migrations for ${entityClass.collection}`);
        await MigrationEngine.executeMigrations(neededMigrations);
      }
    }

    console.log('✅ Entity schema migrations complete');
  }
}

// Integration point: Call during system startup
// In: scripts/system-startup.ts
await AutoMigration.checkAndMigrate();
```

### **Phase 3: Version-Safe Entity Creation**

#### **3.1 Enhanced CREATE Command**
```typescript
// File: commands/data/create/server/DataCreateServerCommand.ts

async execute(params: DataCreateParams): Promise<DataCreateResult> {
  const collection = params.collection;
  console.debug(`🗄️ DATA SERVER: Creating ${collection} with intelligent defaults`);

  try {
    // NEW: Get entity class and apply defaults + migrations
    const EntityClass = EntityRegistry.getEntityClass(collection);

    if (EntityClass) {
      // Apply defaults to prevent constraint failures
      const dataWithDefaults = EntityClass.applyDefaults(params.data);

      // Validate against current schema version
      const validationResult = await EntityClass.validateSchema(dataWithDefaults);

      if (!validationResult.valid) {
        return createDataCreateResultFromParams(params, {
          success: false,
          error: `Schema validation failed: ${validationResult.error}`
        });
      }

      const entity = await DataDaemon.store(collection, dataWithDefaults);
      console.debug(`✅ DATA SERVER: Created ${collection}/${entity.id} with v${EntityClass.version} schema`);

    } else {
      // Fallback for unregistered entities
      const entity = await DataDaemon.store(collection, params.data);
    }

    // Emit event (unchanged)
    const eventName = BaseEntity.getEventName(collection, 'created');
    await Events.emit(eventName, entity, this.context, this.commander);

    return createDataCreateResultFromParams(params, {
      success: true,
      data: entity,
    });

  } catch (error) {
    console.error(`❌ Entity creation failed for ${collection}:`, error);
    return createDataCreateResultFromParams(params, {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}
```

## 🧪 **TESTING STRATEGY**

### **Migration Testing**
```typescript
// File: tests/integration/entity-evolution.test.ts

describe('Entity Evolution System', () => {
  test('Adding field with default prevents constraint failures', async () => {
    // Simulate adding lastMessageAt field to existing rooms
    const oldRoom = { name: 'Test Room', displayName: 'Test' };

    // This should succeed with automatic default
    const result = await runJtagCommand(
      `data/create --collection=Room --data='${JSON.stringify(oldRoom)}'`
    );

    expect(result.success).toBe(true);
    expect(result.data.lastMessageAt).toBeDefined();
    expect(new Date(result.data.lastMessageAt)).toBeInstanceOf(Date);
  });

  test('Schema version tracking works', async () => {
    const schema = await EntityRegistry.getEntitySchema('Room');
    expect(schema.version).toBe('1.1.0');
  });

  test('Migration engine detects field changes', async () => {
    const migrations = await MigrationEngine.generateMigration(
      RoomEntity,
      mockCurrentSchema
    );

    expect(migrations).toContainEqual(
      expect.objectContaining({
        action: 'add',
        field: 'lastMessageAt',
        version: '1.1.0'
      })
    );
  });
});
```

## 🎯 **SUCCESS CRITERIA**

### **Effortless Evolution**
- [ ] Adding fields to entities automatically migrates database
- [ ] Default values prevent constraint failures
- [ ] No manual SQL migration scripts needed
- [ ] Backwards compatible with existing data

### **Production Safety**
- [ ] Migrations are atomic and rollback-safe
- [ ] Schema versions are tracked and logged
- [ ] No data loss during field changes
- [ ] Graceful handling of migration failures

### **Developer Experience**
- [ ] Just add `@DateField({ default: () => new Date() })` and it works
- [ ] Clear migration logs show what changed
- [ ] Schema evolution is explicit and trackable
- [ ] Works seamlessly with existing CRUD operations

## 🌟 **THE RUST-INSPIRED VISION REALIZED**

**Current Reality (Broken)**:
```typescript
@DateField({ index: true })
lastMessageAt?: Date;  // ❌ Optional but database NOT NULL

// Result: Constraint failures, manual fixes needed
```

**Future Reality (Elegant)**:
```typescript
@DateField({
  index: true,
  default: () => new Date(),
  addedInVersion: '1.1.0'
})
lastMessageAt: Date;  // ✅ Required with smart default

// Result: Automatic migration, zero constraint failures
```

**Just like Rust's ecosystem**:
- **Diesel migrations**: Code-driven, type-safe schema evolution
- **serde defaults**: Missing fields handled gracefully
- **Semantic versioning**: Clear evolution tracking

**The dream**: Entity evolution as effortless as entity architecture - change the code, system handles the rest.

## 🔄 **IMPLEMENTATION PRIORITY**

1. **Quick Fix**: Add defaults to RoomEntity.lastMessageAt to unblock current testing
2. **Phase 1**: Enhanced decorators with default support
3. **Phase 2**: Automatic migration detection and execution
4. **Phase 3**: Version-safe entity operations

**Goal**: Never break production, never lose data, never require manual intervention.