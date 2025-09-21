# Entity Adapter Architecture - Serde-Style Data Layer

## Core Philosophy

**Problem**: Current system uses "lame ass adapters" - raw SQL with JSON blob storage, no field extraction, no type safety, no storage optimization.

**Solution**: Serde-style declarative ORM where entities declare their storage requirements and adapters handle the conversion mechanics transparently.

## 1. Entity Field Declaration System

Entities declare their storage requirements using TypeScript decorators:

```typescript
// ChatMessage.ts
export class ChatMessage extends BaseEntity {
  @PrimaryField('messageId')
  messageId: MessageId;

  @ForeignKeyField('senderId', { references: 'User.userId' })
  senderId: UserId;

  @ForeignKeyField('roomId', { references: 'Room.roomId' })
  roomId: RoomId;

  @TextField('content', { maxLength: 4000, searchable: true })
  content: string;

  @DateField('timestamp', { index: true, sortable: true })
  timestamp: Date;

  @EnumField('status', MessageStatus, { default: 'sent' })
  status: MessageStatus;

  @JsonField('metadata', { compressed: true })
  metadata: MessageMetadata;

  @ArrayField('reactions', MessageReaction, { lazy: true })
  reactions: MessageReaction[];
}
```

## 2. Storage-Agnostic Field Mapping

Each adapter interprets decorators for optimal performance in their domain:

### SQLite Adapter
```sql
-- Optimized for relational queries and performance
CREATE TABLE chat_messages (
  messageId TEXT PRIMARY KEY,
  senderId TEXT NOT NULL,
  roomId TEXT NOT NULL,
  content TEXT NOT NULL,
  timestamp TEXT NOT NULL,  -- ISO 8601 for lexicographic sorting
  status TEXT NOT NULL,
  metadata TEXT,            -- JSON for complex objects
  created_at TEXT,
  updated_at TEXT,
  version INTEGER,
  FOREIGN KEY (senderId) REFERENCES users(userId),
  FOREIGN KEY (roomId) REFERENCES rooms(roomId)
);

CREATE INDEX idx_chat_messages_timestamp ON chat_messages(timestamp);
CREATE INDEX idx_chat_messages_room ON chat_messages(roomId, timestamp);
```

### JSON Adapter
```json
{
  "chat_messages": {
    "schema": {
      "messageId": { "type": "primary", "format": "uuid" },
      "timestamp": { "type": "date", "stored_as": "iso_string" },
      "metadata": { "type": "object", "nested": true }
    },
    "data": [
      {
        "messageId": "msg-123",
        "timestamp": "2025-09-20T20:12:43.578Z",
        "metadata": { "source": "user", "deviceType": "web" }
      }
    ]
  }
}
```

### XML Adapter (Hypothetical)
```xml
<entities>
  <chat_message messageId="msg-123" timestamp="2025-09-20T20:12:43.578Z">
    <content><![CDATA[Hello world]]></content>
    <metadata>
      <source>user</source>
      <deviceType>web</deviceType>
    </metadata>
  </chat_message>
</entities>
```

## 3. Bidirectional Type Conversion

Adapters handle automatic conversion between domain types and storage formats:

```typescript
// Domain Layer (TypeScript)
message.timestamp = new Date('2025-09-20T20:12:43.578Z');

// Storage Layer (SQLite)
// Adapter automatically converts: Date → ISO 8601 TEXT
INSERT INTO chat_messages (timestamp) VALUES ('2025-09-20T20:12:43.578Z');

// Retrieval (SQLite → Domain)
// Adapter automatically converts: ISO 8601 TEXT → Date
SELECT timestamp FROM chat_messages;
// Returns: new Date('2025-09-20T20:12:43.578Z')
```

**Key Insight**: Domain code works with clean TypeScript types (`Date`, `string`, `MessageStatus`). Storage works with optimal formats (ISO strings, enums as TEXT, foreign keys as TEXT). Adapters handle the impedance mismatch.

## 4. Dynamic Query Generation

Instead of writing repetitive SQL, adapters generate queries from entity metadata:

```typescript
// High-level query (storage-agnostic)
const messages = await DataDaemon.query<ChatMessage>({
  collection: 'chat_messages',
  filters: {
    roomId: 'general',
    timestamp: { after: '2025-09-20T00:00:00Z' }
  },
  orderBy: [{ field: 'timestamp', direction: 'desc' }],
  limit: 50
});

// SQLite Adapter generates:
SELECT * FROM chat_messages
WHERE roomId = ? AND timestamp > ?
ORDER BY timestamp DESC
LIMIT 50;

// JSON Adapter filters in-memory:
data.chat_messages
  .filter(msg => msg.roomId === 'general' && msg.timestamp > '2025-09-20T00:00:00Z')
  .sort((a, b) => b.timestamp.localeCompare(a.timestamp))
  .slice(0, 50);
```

## 5. Entity-Defined Relationships

**Core Principle**: Entities define relationships declaratively, adapters implement them optimally.

```typescript
// User.ts - Entity defines what it relates to
export class User extends BaseEntity {
  @PrimaryField('userId')
  userId: UserId;

  @HasManyField('sentMessages', ChatMessage, 'senderId')
  sentMessages: ChatMessage[];

  @HasManyField('joinedRooms', Room, { through: 'RoomMember', foreignKey: 'userId' })
  joinedRooms: Room[];
}

// ChatMessage.ts - Entity defines its relationships
export class ChatMessage extends BaseEntity {
  @ForeignKeyField('senderId', { references: 'User.userId' })
  senderId: UserId;

  @ForeignKeyField('roomId', { references: 'Room.roomId' })
  roomId: RoomId;

  @BelongsToField('sender', User, 'senderId')
  sender: User;

  @BelongsToField('room', Room, 'roomId')
  room: Room;

  @HasManyField('replies', ChatMessage, 'replyToId')
  replies: ChatMessage[];
}

// Room.ts - Entity defines its relationships
export class Room extends BaseEntity {
  @PrimaryField('roomId')
  roomId: RoomId;

  @HasManyField('messages', ChatMessage, 'roomId')
  messages: ChatMessage[];

  @HasManyField('members', User, { through: 'RoomMember', foreignKey: 'roomId' })
  members: User[];
}
```

**Adapter Implementation**: Each adapter implements relationships optimally:

**SQLite**: Foreign keys, JOINs, referential integrity
```sql
-- Adapter generates proper foreign key constraints
FOREIGN KEY (senderId) REFERENCES users(userId)

-- Automatic JOIN generation for includes
SELECT m.*, u.name as sender_name
FROM chat_messages m
JOIN users u ON m.senderId = u.userId
WHERE m.roomId = ?
```

**JSON**: Reference resolution, in-memory joins
```typescript
// Adapter resolves references in-memory
const messages = data.chat_messages.map(msg => ({
  ...msg,
  sender: data.users.find(u => u.userId === msg.senderId)
}));
```

**Graph**: Native relationship traversal
```cypher
-- Neo4j adapter could generate Cypher
MATCH (u:User)-[:SENT]->(m:Message)-[:IN]->(r:Room)
WHERE r.roomId = $roomId
RETURN u, m, r
```

## 6. Migration and Schema Evolution

Field changes handled through versioned migrations:

```typescript
// Version 1: Simple content
@TextField('content')
content: string;

// Version 2: Rich content with formatting
@JsonField('content', { version: 2,
  migration: (oldContent: string) => ({
    text: oldContent,
    formatting: DEFAULT_MESSAGE_FORMATTING,
    attachments: []
  })
})
content: MessageContent;
```

## 7. Performance Optimizations

Each adapter optimizes for its strengths:

**SQLite Optimizations**:
- Extracted columns for common queries (`senderId`, `roomId`, `timestamp`)
- Proper indexes on frequently queried fields
- Foreign key constraints for referential integrity
- JSON extraction functions for nested queries: `JSON_EXTRACT(metadata, '$.source')`

**JSON Optimizations**:
- In-memory indexing for fast filters
- Lazy loading for large collections
- Compressed JSON for metadata fields
- Schema validation and type coercion

**Memory Optimizations**:
- Field-level lazy loading (`@ArrayField({ lazy: true })`)
- Pagination support built into query interface
- Connection pooling and prepared statements

## 8. Implementation Strategy

**Phase 1: Core Infrastructure**
1. `@Field` decorator system with metadata collection
2. `FieldMetadata` interface for storing field requirements
3. Enhanced `SqliteStorageAdapter` that reads field metadata
4. Type-safe query builder using field metadata

**Phase 2: Field Type Support**
1. `@DateField` with automatic Date ↔ ISO string conversion
2. `@JsonField` with automatic JSON serialization
3. `@EnumField` with validation and type safety
4. `@ForeignKeyField` with relationship mapping

**Phase 3: Advanced Features**
1. Relationship decorators (`@HasMany`, `@BelongsTo`)
2. Automatic JOIN generation for `include` queries
3. Migration system for schema evolution
4. Performance monitoring and query optimization

## 9. Benefits

**Developer Experience**:
- No more manual SQL writing
- Type-safe queries with autocompletion
- Automatic relationship handling
- Storage-agnostic code (swap SQLite ↔ JSON ↔ XML)

**Performance**:
- Optimal storage format per backend
- Proper indexing and foreign keys
- Efficient queries generated from metadata
- Lazy loading and pagination built-in

**Maintainability**:
- Single source of truth (entity declarations)
- Automatic migrations
- Clear separation: domain logic vs storage mechanics
- Easy to add new storage backends

This is the "serde-style declarative ORM" - entities declare what they need, adapters figure out how to store it optimally.

## 10. Auto-Conversion System - The Key to Serde Success

**Core Principle**: Adapters handle automatic bidirectional conversion between domain types and storage formats. Domain code never knows about storage representation.

### Domain Layer (Clean TypeScript)
```typescript
// ChatMessage domain code works with native types
const message: ChatMessageData = {
  messageId: MessageId('msg-123'),
  senderId: UserId('user-joel-12345'),
  roomId: RoomId('general'),
  timestamp: new Date('2025-09-20T20:12:43.578Z'),
  status: MessageStatus.SENT,
  content: {
    text: "Hello world",
    attachments: [],
    formatting: { markdown: false, mentions: [], hashtags: [], links: [], codeBlocks: [] }
  },
  metadata: {
    source: 'user',
    deviceType: 'web'
  }
};
```

### Storage Layer (Optimized SQLite)
```sql
-- Adapter auto-converts TO storage on INSERT
INSERT INTO chat_messages (
  messageId, senderId, roomId, timestamp, status, content, metadata
) VALUES (
  'msg-123',                              -- String as-is
  'user-joel-12345',                      -- UserId → TEXT
  'general',                              -- RoomId → TEXT
  '2025-09-20T20:12:43.578Z',            -- Date → ISO string
  'sent',                                 -- MessageStatus → TEXT
  '{"text":"Hello world","attachments":[],...}',  -- MessageContent → JSON
  '{"source":"user","deviceType":"web"}'          -- MessageMetadata → JSON
);

-- Adapter auto-converts FROM storage on SELECT
SELECT messageId, senderId, roomId, timestamp, status, content, metadata
FROM chat_messages WHERE messageId = 'msg-123';

-- Returns domain objects with proper types:
-- timestamp: Date object, status: MessageStatus enum, content: MessageContent interface
```

### Auto-Conversion Implementation
```typescript
/**
 * Enhanced SqliteStorageAdapter with automatic type conversion
 */
class SqliteStorageAdapter {

  /**
   * Convert domain value TO storage format
   */
  private toStorage(field: string, value: any, fieldType: FieldType): any {
    switch (fieldType) {
      case 'date':
        return value instanceof Date ? value.toISOString() : value;

      case 'enum':
        return String(value);

      case 'foreign_key':
        return String(value); // UserId, RoomId → TEXT

      case 'json':
        return typeof value === 'object' ? JSON.stringify(value) : value;

      case 'primary':
      case 'text':
      default:
        return value;
    }
  }

  /**
   * Convert storage value FROM storage format to domain type
   */
  private fromStorage(field: string, value: any, fieldType: FieldType): any {
    if (value === null || value === undefined) return value;

    switch (fieldType) {
      case 'date':
        return new Date(value);

      case 'enum':
        return value; // TypeScript handles enum conversion

      case 'foreign_key':
        return value; // TypeScript handles branded type conversion (UserId, RoomId)

      case 'json':
        return typeof value === 'string' ? JSON.parse(value) : value;

      case 'primary':
      case 'text':
      default:
        return value;
    }
  }

  /**
   * Create record with automatic field conversion
   */
  async create<T>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    const fieldMappings = this.getFieldMappings(record.collection);
    const convertedData: Record<string, any> = {};

    // Auto-convert each field TO storage format
    for (const [field, fieldType] of Object.entries(fieldMappings)) {
      if (field in record.data) {
        convertedData[field] = this.toStorage(field, record.data[field], fieldType);
      }
    }

    // Execute SQL with converted values
    const sql = this.buildInsertSQL(record.collection, fieldMappings);
    await this.execute(sql, convertedData);

    return { success: true, data: record };
  }

  /**
   * Read record with automatic field conversion
   */
  async read<T>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    const fieldMappings = this.getFieldMappings(collection);
    const sql = this.buildSelectSQL(collection, fieldMappings);
    const row = await this.executeOne(sql, { id });

    if (!row) {
      return { success: false, error: 'Record not found' };
    }

    // Auto-convert each field FROM storage format
    const convertedData: any = {};
    for (const [field, fieldType] of Object.entries(fieldMappings)) {
      if (field in row) {
        convertedData[field] = this.fromStorage(field, row[field], fieldType);
      }
    }

    return {
      success: true,
      data: {
        id,
        collection,
        data: convertedData,
        metadata: { createdAt: row.created_at, updatedAt: row.updated_at, version: row.version }
      }
    };
  }
}
```

### Field Mapping from Existing Entities
```typescript
/**
 * Analyze existing TypeScript interfaces to determine field types
 * No decorators needed - introspect the existing well-designed interfaces
 */
const ENTITY_FIELD_MAPPINGS = {
  'chat_messages': {
    // Primary and foreign keys
    messageId: 'primary',
    senderId: 'foreign_key',  // UserId → TEXT, references users.userId
    roomId: 'foreign_key',    // RoomId → TEXT, references rooms.roomId

    // Date fields (auto-convert Date ↔ ISO string)
    timestamp: 'date',
    editedAt: 'date',

    // Enum fields (auto-convert enum ↔ TEXT)
    status: 'enum',           // MessageStatus → TEXT
    priority: 'enum',         // MessagePriority → TEXT

    // Simple text fields
    senderName: 'text',

    // Complex objects (auto-convert object ↔ JSON)
    content: 'json',          // MessageContent → JSON TEXT
    metadata: 'json',         // MessageMetadata → JSON TEXT
    reactions: 'json'         // MessageReaction[] → JSON TEXT
  },

  'users': {
    userId: 'primary',
    citizenId: 'text',
    personaId: 'text',
    type: 'enum',             // UserType → TEXT
    status: 'enum',           // UserStatus → TEXT
    lastActiveAt: 'date',     // Date → ISO string
    profile: 'json',          // UserProfile → JSON TEXT
    capabilities: 'json',     // UserCapabilities → JSON TEXT
    preferences: 'json',      // UserPreferences → JSON TEXT
    sessionsActive: 'json'    // SessionId[] → JSON TEXT
  },

  'rooms': {
    roomId: 'primary',
    name: 'text',
    type: 'enum',             // RoomType → TEXT
    status: 'enum',           // RoomStatus → TEXT
    createdAt: 'date',
    lastActivityAt: 'date',
    privacy: 'json',          // RoomPrivacy → JSON TEXT
    settings: 'json',         // RoomSettings → JSON TEXT
    members: 'json',          // RoomMember[] → JSON TEXT
    stats: 'json'             // RoomStats → JSON TEXT
  }
};
```

### Generated Schema with Proper Types
```sql
-- ChatMessage table with extracted fields + optimal indexes
CREATE TABLE chat_messages (
  messageId TEXT PRIMARY KEY,
  senderId TEXT NOT NULL,
  roomId TEXT NOT NULL,
  timestamp TEXT NOT NULL,    -- Date as ISO 8601 for sorting
  status TEXT NOT NULL,       -- MessageStatus as TEXT
  priority TEXT DEFAULT 'normal',
  senderName TEXT,
  content TEXT,               -- MessageContent as JSON
  metadata TEXT,              -- MessageMetadata as JSON
  reactions TEXT,             -- MessageReaction[] as JSON
  editedAt TEXT,              -- Optional Date as ISO 8601
  -- BaseEntity fields
  id TEXT,
  created_at TEXT,
  updated_at TEXT,
  version INTEGER,
  FOREIGN KEY (senderId) REFERENCES users(userId),
  FOREIGN KEY (roomId) REFERENCES rooms(roomId)
);

-- Performance indexes on commonly queried extracted fields
CREATE INDEX idx_chat_messages_timestamp ON chat_messages(timestamp);
CREATE INDEX idx_chat_messages_room_time ON chat_messages(roomId, timestamp);
CREATE INDEX idx_chat_messages_sender ON chat_messages(senderId);
CREATE INDEX idx_chat_messages_status ON chat_messages(status);
```

### Benefits of Auto-Conversion

**Developer Experience**:
```typescript
// Domain code is clean and type-safe
message.timestamp = new Date();     // Works with Date objects
message.status = MessageStatus.SENT; // Works with enums
message.senderId = UserId('user-123'); // Works with branded types

// No manual serialization needed!
await DataDaemon.store('chat_messages', message);
```

**Performance**:
- Extracted fields enable proper indexes and fast queries
- Foreign key constraints ensure referential integrity
- Date fields stored as ISO strings sort lexicographically
- Complex objects still stored as JSON where appropriate

**Storage Agnostic**:
- Same auto-conversion logic works for JSON adapter, XML adapter, etc.
- Each adapter optimizes conversion for its storage format
- Domain code remains completely unchanged

## 11. Pragmatic Enhancement Architecture - Fix Existing Code

**What We Have (Working)**:
- ✅ `ChatMessageData`, `UserData`, `ChatRoomData` interfaces
- ✅ `SqliteStorageAdapter`, `FileStorageAdapter` classes
- ✅ `DataDaemon` with static interface
- ✅ Working message system with JSON blob storage
- ✅ Data seeding system

**Goal**: Enhance existing adapters to support both JSON and SQL field extraction without breaking anything.

### Enhanced Adapter Architecture

**1. Field Mapping Configuration (Simple Static Config)**
```typescript
// system/data/core/FieldMappings.ts - Simple static configuration
export const COLLECTION_FIELD_MAPPINGS = {
  'chat_messages': {
    extracted: ['messageId', 'senderId', 'roomId', 'timestamp', 'status'], // Extract to columns
    json: ['content', 'metadata', 'reactions'] // Keep as JSON
  },
  'users': {
    extracted: ['userId', 'type', 'status', 'lastActiveAt'],
    json: ['profile', 'capabilities', 'preferences', 'sessionsActive']
  },
  'rooms': {
    extracted: ['roomId', 'name', 'type', 'status', 'createdAt'],
    json: ['privacy', 'settings', 'members', 'stats']
  }
};
```

**2. Enhanced SqliteStorageAdapter (Modify Existing Class)**
```typescript
// daemons/data-daemon/server/SqliteStorageAdapter.ts - Enhance existing
class SqliteStorageAdapter {

  // NEW: Check if collection uses field extraction
  private usesFieldExtraction(collection: string): boolean {
    return COLLECTION_FIELD_MAPPINGS[collection] !== undefined;
  }

  // ENHANCED: create() method supports both modes
  async create<T>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    if (this.usesFieldExtraction(record.collection)) {
      return await this.createWithFieldExtraction(record);
    } else {
      return await this.createJsonBlob(record); // Existing behavior
    }
  }

  // NEW: Field extraction storage
  private async createWithFieldExtraction<T>(record: DataRecord<T>) {
    const mapping = COLLECTION_FIELD_MAPPINGS[record.collection];
    const tableName = record.collection;

    // Ensure table exists with proper schema
    await this.ensureTableSchema(tableName, mapping, record.data);

    // Extract fields vs JSON
    const extractedFields = {};
    const jsonData = {};

    for (const [key, value] of Object.entries(record.data)) {
      if (mapping.extracted.includes(key)) {
        extractedFields[key] = this.convertToStorage(key, value);
      } else {
        jsonData[key] = value;
      }
    }

    // Store extracted fields + remaining JSON
    const allData = {
      ...extractedFields,
      _json_data: JSON.stringify(jsonData), // Remaining complex fields
      // BaseEntity fields
      id: record.id,
      created_at: record.metadata?.createdAt || new Date().toISOString(),
      updated_at: record.metadata?.updatedAt || new Date().toISOString(),
      version: record.metadata?.version || 1
    };

    const sql = this.buildInsertSQL(tableName, allData);
    await this.execute(sql, Object.values(allData));

    return { success: true, data: record };
  }

  // Keep existing JSON blob method for backward compatibility
  private async createJsonBlob<T>(record: DataRecord<T>) {
    // Existing entities table approach - unchanged
  }
}
```

**3. Auto-Schema Creation**
```typescript
// NEW: Auto-create tables with extracted fields
private async ensureTableSchema(tableName: string, mapping: FieldMapping, sampleData: any) {
  const exists = await this.tableExists(tableName);
  if (!exists) {
    const columns = [];

    // Add extracted fields with proper types
    for (const fieldName of mapping.extracted) {
      const sqlType = this.inferSqlType(fieldName, sampleData[fieldName]);
      columns.push(`${fieldName} ${sqlType}`);
    }

    // Add JSON data column for remaining fields
    columns.push(`_json_data TEXT`);

    // Add BaseEntity fields
    columns.push(`id TEXT PRIMARY KEY`);
    columns.push(`created_at TEXT`);
    columns.push(`updated_at TEXT`);
    columns.push(`version INTEGER`);

    const sql = `CREATE TABLE ${tableName} (${columns.join(', ')})`;
    await this.execute(sql);

    // Create indexes on key fields
    await this.createIndexes(tableName, mapping.extracted);
  }
}
```

**4. Smart Type Inference**
```typescript
private inferSqlType(fieldName: string, value: any): string {
  // Smart type inference from field names and values
  if (fieldName.includes('Id')) return 'TEXT'; // IDs are TEXT
  if (fieldName.includes('timestamp') || fieldName.includes('At')) return 'TEXT'; // Dates as ISO
  if (fieldName === 'status' || fieldName === 'type') return 'TEXT'; // Enums as TEXT
  if (typeof value === 'number') return 'INTEGER';
  if (typeof value === 'boolean') return 'INTEGER'; // SQLite booleans as INTEGER
  return 'TEXT'; // Default to TEXT
}
```

**5. Hybrid Read with Field Reconstruction**
```typescript
async read<T>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
  if (this.usesFieldExtraction(collection)) {
    const row = await this.executeOne(`SELECT * FROM ${collection} WHERE id = ?`, [id]);
    if (!row) return { success: false, error: 'Not found' };

    const mapping = COLLECTION_FIELD_MAPPINGS[collection];

    // Reconstruct full object from extracted fields + JSON
    const reconstructed: any = {};

    // Add extracted fields with conversion
    for (const fieldName of mapping.extracted) {
      reconstructed[fieldName] = this.convertFromStorage(fieldName, row[fieldName]);
    }

    // Merge in JSON data
    if (row._json_data) {
      Object.assign(reconstructed, JSON.parse(row._json_data));
    }

    return {
      success: true,
      data: {
        id,
        collection,
        data: reconstructed,
        metadata: { createdAt: row.created_at, updatedAt: row.updated_at, version: row.version }
      }
    };
  } else {
    // Existing JSON blob read - unchanged
  }
}
```

### Migration Strategy

**Phase 1**: Enable field extraction for `chat_messages`
- Add field mapping config
- Deploy enhanced SqliteStorageAdapter
- New messages use field extraction, existing queries work via JSON fallback

**Phase 2**: Data migration script
- Copy existing JSON blob messages to new field-extracted format
- Atomic table swap

**Phase 3**: Enable for `users` and `rooms`

### Benefits
- **No breaking changes**: Existing code keeps working
- **Immediate performance**: New records get proper indexing
- **Both JSON and SQL**: FileStorageAdapter gets JSON, SqliteStorageAdapter gets field extraction
- **Existing seeding works**: Data seeding continues to work unchanged
- **Incremental**: Migrate collections one at a time

This enhances the existing "lame ass adapters" to be smart about field extraction while maintaining all existing functionality.