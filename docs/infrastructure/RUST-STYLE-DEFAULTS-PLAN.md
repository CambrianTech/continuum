# RUST-STYLE DEFAULTS IMPLEMENTATION PLAN

## 🎯 **VISION: EFFORTLESS ENTITY CREATION**

**Goal**: Make entity creation as elegant as our architecture - minimal input, maximum intelligence.

**Rust Inspiration**: `serde::Deserialize` with `#[serde(default)]` provides effortless deserialization with smart defaults.

**Our Challenge**: Database constraints require all fields, but users shouldn't need to provide them all.

## 🔧 **CURRENT PROBLEM**

```bash
# ❌ CURRENT: Complex creation requirements
./jtag data/create --collection=users --data='{
  "type": "human",
  "displayName": "Claude",
  "shortDescription": "AI Assistant",
  "status": "online",
  "lastActiveAt": "2025-09-24T19:35:47.801Z",
  "profile": { "displayName": "Claude", "avatar": "🤖", ... },
  "capabilities": { "canSendMessages": true, ... },
  "preferences": { "theme": "dark", ... },
  "sessionsActive": []
}'

# 💀 FAILURE: Database NOT NULL constraint errors
❌ SQLite: NOT NULL constraint failed: rooms.last_message_at
```

## ✨ **PROPOSED SOLUTION: SMART DEFAULTS**

```bash
# ✅ PROPOSED: Effortless creation
./jtag data/create --collection=users --data='{}'                    # All defaults!
./jtag data/create --collection=users --data='{"displayName":"Claude"}' # Override only what matters
./jtag data/create --collection=rooms --data='{"name":"General"}'    # No more constraint failures
```

## 🏗️ **IMPLEMENTATION ARCHITECTURE**

### **Phase 1: Default Infrastructure**

#### **1.1 Entity Default Decorators**
```typescript
// File: system/data/decorators/DefaultDecorators.ts
export function TextField(options?: { default?: string | (() => string) }) {
  return function(target: any, propertyKey: string) {
    // Store default metadata
    Reflect.defineMetadata('field:default', options?.default, target, propertyKey);
    Reflect.defineMetadata('field:type', 'text', target, propertyKey);
  };
}

export function DateField(options?: { default?: Date | (() => Date) }) {
  return function(target: any, propertyKey: string) {
    Reflect.defineMetadata('field:default', options?.default, target, propertyKey);
    Reflect.defineMetadata('field:type', 'date', target, propertyKey);
  };
}

export function BooleanField(options?: { default?: boolean | (() => boolean) }) {
  return function(target: any, propertyKey: string) {
    Reflect.defineMetadata('field:default', options?.default, target, propertyKey);
    Reflect.defineMetadata('field:type', 'boolean', target, propertyKey);
  };
}
```

#### **1.2 Default Application Logic**
```typescript
// File: system/data/entities/BaseEntity.ts
export abstract class BaseEntity {
  // Existing code...

  /**
   * Apply entity-defined defaults to partial data
   * Rust equivalent: serde::Deserialize implementation
   */
  static applyDefaults<T extends BaseEntity>(
    this: new() => T,
    partialData: Partial<T>
  ): T {
    const instance = new this();
    const proto = Object.getPrototypeOf(instance);

    // Get all properties with default metadata
    const properties = Object.getOwnPropertyNames(proto);

    for (const prop of properties) {
      const defaultValue = Reflect.getMetadata('field:default', proto, prop);

      if (defaultValue !== undefined && !(prop in partialData)) {
        // Apply default value (function or static)
        (partialData as any)[prop] = typeof defaultValue === 'function'
          ? defaultValue()
          : defaultValue;
      }
    }

    return Object.assign(instance, partialData) as T;
  }
}
```

#### **1.3 Enhanced User Entity with Defaults**
```typescript
// File: system/data/entities/UserEntity.ts
import { TextField, DateField, BooleanField, ObjectField } from '../decorators/DefaultDecorators';

export class UserEntity extends BaseEntity {
  @TextField({ default: 'human' })
  type: 'human' | 'ai';

  @TextField({ default: () => `User_${Date.now()}` })
  displayName: string;

  @TextField({ default: 'New user' })
  shortDescription: string;

  @TextField({ default: 'online' })
  status: 'online' | 'offline' | 'away';

  @DateField({ default: () => new Date() })
  lastActiveAt: Date;

  @ObjectField({
    default: () => ({
      displayName: 'User',
      avatar: '👤',
      bio: 'New user',
      location: 'Unknown',
      joinedAt: new Date().toISOString()
    })
  })
  profile: UserProfile;

  @ObjectField({
    default: () => ({
      canSendMessages: true,
      canReceiveMessages: true,
      canTrain: false,
      canCreateRooms: true,
      canInviteOthers: true,
      canModerate: false,
      autoResponds: false,
      providesContext: false,
      canAccessPersonas: true
    })
  })
  capabilities: UserCapabilities;

  @ObjectField({
    default: () => ({
      theme: 'dark',
      language: 'en',
      timezone: 'UTC',
      notifications: {
        mentions: true,
        directMessages: true,
        roomUpdates: false
      },
      privacy: {
        showOnlineStatus: true,
        allowDirectMessages: true,
        shareActivity: false
      }
    })
  })
  preferences: UserPreferences;

  @ObjectField({ default: () => [] })
  sessionsActive: string[];

  static get collection(): string { return 'users'; }
}
```

### **Phase 2: Creation Integration**

#### **2.1 Enhanced CREATE Command**
```typescript
// File: commands/data/create/server/DataCreateServerCommand.ts
async execute(params: DataCreateParams): Promise<DataCreateResult> {
  const collection = params.collection;
  console.debug(`🗄️ DATA SERVER: Creating ${collection} entity with smart defaults`);

  // NEW: Apply defaults before creation
  const EntityClass = EntityRegistry.getEntityClass(collection);
  if (EntityClass && EntityClass.applyDefaults) {
    const dataWithDefaults = EntityClass.applyDefaults(params.data);
    const entity = await DataDaemon.store(collection, dataWithDefaults);
  } else {
    // Fallback to original behavior
    const entity = await DataDaemon.store(collection, params.data);
  }

  console.debug(`✅ DATA SERVER: Created ${collection}/${entity.id} with smart defaults`);

  // Emit event (unchanged)
  const eventName = BaseEntity.getEventName(collection, 'created');
  await Events.emit(eventName, entity, this.context, this.commander);

  return createDataCreateResultFromParams(params, {
    success: true,
    data: entity,
  });
}
```

#### **2.2 Schema Integration**
```typescript
// File: commands/data/schema/server/DataSchemaServerCommand.ts
async execute(params: DataSchemaParams): Promise<DataSchemaResult> {
  // Existing schema validation...

  if (params.data) {
    // NEW: Apply defaults before validation
    const EntityClass = EntityRegistry.getEntityClass(params.collection);
    const dataWithDefaults = EntityClass?.applyDefaults
      ? EntityClass.applyDefaults(params.data)
      : params.data;

    // Validate complete entity
    const isValid = await this.validateEntity(params.collection, dataWithDefaults);

    return {
      collection: params.collection,
      schema: schema,
      valid: isValid,
      // Return data with defaults applied
      validatedEntity: isValid ? dataWithDefaults : undefined,
      error: isValid ? undefined : 'Validation failed even with defaults'
    };
  }

  // Return schema only...
}
```

### **Phase 3: Database Schema Harmony**

#### **3.1 Room Entity Fixes**
```typescript
// File: system/data/entities/RoomEntity.ts
export class RoomEntity extends BaseEntity {
  @TextField({ default: () => `Room_${Date.now()}` })
  name: string;

  @TextField({ default: () => 'New Room' })
  displayName: string;

  @TextField({ default: 'A new room for conversations' })
  description: string;

  @DateField({ default: () => new Date() }) // FIX: Prevents NOT NULL constraint
  lastMessageAt: Date;

  @TextField({ default: 'public' })
  type: 'public' | 'private';

  @TextField({ default: 'active' })
  status: 'active' | 'archived';

  // ... other fields with appropriate defaults
}
```

## 🧪 **TESTING STRATEGY**

### **Test Suite Enhancement**
```typescript
// File: tests/integration/smart-defaults.test.ts
describe('Smart Defaults System', () => {
  test('Empty data creates valid user', async () => {
    const result = await runJtagCommand('data/create --collection=users --data="{}"');

    expect(result.success).toBe(true);
    expect(result.data.displayName).toMatch(/^User_\d+$/);
    expect(result.data.status).toBe('online');
    expect(result.data.capabilities.canSendMessages).toBe(true);
  });

  test('Partial data merges with defaults', async () => {
    const result = await runJtagCommand('data/create --collection=users --data=\'{"displayName":"Claude"}\'');

    expect(result.success).toBe(true);
    expect(result.data.displayName).toBe('Claude'); // Override
    expect(result.data.status).toBe('online'); // Default
    expect(result.data.type).toBe('human'); // Default
  });

  test('No more constraint failures', async () => {
    const result = await runJtagCommand('data/create --collection=rooms --data=\'{"name":"Test"}\'');

    expect(result.success).toBe(true);
    expect(result.data.lastMessageAt).toBeDefined(); // Default prevents NOT NULL failure
  });
});
```

## 🎯 **SUCCESS CRITERIA**

### **Effortless Creation**
- [ ] `./jtag data/create --collection=users --data='{}'` creates valid user
- [ ] `./jtag data/create --collection=rooms --data='{}'` creates valid room
- [ ] `./jtag data/create --collection=messages --data='{}'` creates valid message

### **Smart Overrides**
- [ ] `{"displayName":"Custom"}` overrides default, keeps other defaults
- [ ] Complex objects merge intelligently (partial profile updates)
- [ ] Function defaults execute at creation time (dynamic timestamps)

### **Zero Constraint Failures**
- [ ] All NOT NULL database fields have reasonable defaults
- [ ] Creation never fails due to missing required fields
- [ ] Database migrations support adding new defaulted fields

### **Type Safety Maintained**
- [ ] Default decorators preserve TypeScript type safety
- [ ] Entity.applyDefaults() returns properly typed instances
- [ ] No loss of existing type constraints

## 🌟 **THE RUST-STYLE VISION REALIZED**

**Before (Complex)**:
```bash
./jtag data/create --collection=users --data='{...100+ lines of boilerplate...}'
```

**After (Elegant)**:
```bash
./jtag data/create --collection=users --data='{}'  # Perfect user created!
./jtag data/create --collection=users --data='{"displayName":"Claude"}' # Custom name, smart defaults
```

**Just like Rust's serde**:
- Minimal input required
- Maximum intelligence applied
- Type safety preserved
- Zero boilerplate code

**The dream**: Entity creation as effortless as our already-perfect architecture is extensible.