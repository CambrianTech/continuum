# Entity Architecture - Clean Inheritance with Decorators

## The Simple Truth

Every entity extends BaseEntity. BaseEntity has decorators. Child entities get those decorators for FREE through inheritance. No field replication. Ever.

## BaseEntity - The Foundation

```typescript
export abstract class BaseEntity {
  @PrimaryField()
  id: string;

  @DateField({ index: true })
  createdAt: ISOString;

  @DateField({ index: true })
  updatedAt: ISOString;

  @NumberField()
  version: number;

  static readonly collection: string; // Must be defined in child classes

  constructor() {
    this.id = '';
    this.createdAt = new Date().toISOString() as ISOString;
    this.updatedAt = new Date().toISOString() as ISOString;
    this.version = 1;
  }
}
```

## Example: FartEntity

```typescript
export class FartEntity extends BaseEntity {
  static readonly collection = 'fart';

  @TextField()
  smell: string;

  constructor() {
    super();
    this.smell = '';
  }
}
```

Creates table `fart` with columns:
- `id` (from BaseEntity)
- `createdAt` (from BaseEntity)
- `updatedAt` (from BaseEntity)
- `version` (from BaseEntity)
- `smell` (from FartEntity)

## The Three Real Entities

1. **UserEntity extends BaseEntity**
   - Collection: 'User'
   - Additional fields: userId, displayName, shortDescription, type, etc.

2. **RoomEntity extends BaseEntity**
   - Collection: 'Room'
   - Additional fields: roomId, name, displayName, type, etc.

3. **ChatMessageEntity extends BaseEntity**
   - Collection: 'ChatMessage'
   - Additional fields: messageId, roomId, senderId, content, etc.

## Decorator Inheritance Magic

The `getFieldMetadata()` function walks the prototype chain:

```typescript
export function getFieldMetadata(entityClass: EntityConstructor): Map<string, FieldMetadata> {
  const allMetadata = new Map<string, FieldMetadata>();

  // Walk up the prototype chain to collect inherited field metadata
  let currentClass = entityClass;
  while (currentClass) {
    const classMetadata = FIELD_METADATA.get(currentClass);
    if (classMetadata) {
      for (const [fieldName, metadata] of classMetadata) {
        if (!allMetadata.has(fieldName)) {
          allMetadata.set(fieldName, metadata);
        }
      }
    }
    const parent = Object.getPrototypeOf(currentClass);
    currentClass = parent === Function.prototype ? null : parent;
  }

  return allMetadata;
}
```

## Adapter Contract

When adapter calls `getFieldMetadata(UserEntity)`, it gets:
- BaseEntity fields: id, createdAt, updatedAt, version
- UserEntity fields: userId, displayName, shortDescription, type, status, etc.

Creates `User` table with ALL fields. No manual mapping. No replication. Just works.

## Rules

1. **One class definition per entity**
2. **BaseEntity fields come for FREE**
3. **No field replication EVER**
4. **No declare keywords**
5. **Decorators handle everything**
6. **Adapter sees complete field map automatically**