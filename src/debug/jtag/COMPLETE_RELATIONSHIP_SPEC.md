# Complete Relationship Specification & Migration Plan

## All Relationships Defined

### 1. User Relationships

#### Primary Entity: `users`
- **PK**: `user_id` (UUID)
- **Unique Keys**: `email`, `session_id`

#### Outgoing Relationships (One-to-Many)
1. `users.user_id` → `user_sessions.user_id` (One user, many sessions)
2. `users.user_id` → `user_permissions.user_id` (One user, many permissions)
3. `users.user_id` → `room_memberships.user_id` (One user, many room memberships)
4. `users.user_id` → `chat_messages.sender_id` (One user, many sent messages)
5. `users.user_id` → `message_reactions.user_id` (One user, many reactions)
6. `users.user_id` → `chat_rooms.created_by` (One user, many created rooms)
7. `users.user_id` → `user_permissions.granted_by` (One user grants many permissions)

#### Self-Referencing Relationships
- `users.user_id` → `user_blocks.blocker_id` → `user_blocks.blocked_id`
- `users.user_id` → `user_follows.follower_id` → `user_follows.followed_id`

### 2. Chat Room Relationships

#### Primary Entity: `chat_rooms`
- **PK**: `room_id` (UUID)
- **Unique Keys**: `name` (for public rooms)

#### Outgoing Relationships (One-to-Many)
1. `chat_rooms.room_id` → `room_memberships.room_id` (One room, many members)
2. `chat_rooms.room_id` → `chat_messages.room_id` (One room, many messages)
3. `chat_rooms.room_id` → `room_settings.room_id` (One room, many settings)
4. `chat_rooms.room_id` → `room_moderators.room_id` (One room, many moderators)

#### Incoming Relationships (Many-to-One)
1. `users.user_id` ← `chat_rooms.created_by` (Many rooms, one creator)

### 3. Message Relationships

#### Primary Entity: `chat_messages`
- **PK**: `message_id` (UUID)

#### Outgoing Relationships (One-to-Many)
1. `chat_messages.message_id` → `message_reactions.message_id` (One message, many reactions)
2. `chat_messages.message_id` → `message_attachments.message_id` (One message, many attachments)
3. `chat_messages.message_id` → `message_edits.message_id` (One message, many edit history)

#### Incoming Relationships (Many-to-One)
1. `users.user_id` ← `chat_messages.sender_id` (Many messages, one sender)
2. `chat_rooms.room_id` ← `chat_messages.room_id` (Many messages, one room)

#### Self-Referencing Relationships
1. `chat_messages.message_id` ← `chat_messages.reply_to_id` (Thread replies)

### 4. Junction Table Relationships (Many-to-Many)

#### `room_memberships` (Users ↔ Rooms)
- **Composite PK**: `(room_id, user_id)` or separate `membership_id`
- **FKs**: `room_id` → `chat_rooms.room_id`, `user_id` → `users.user_id`
- **Additional Fields**: `role`, `joined_at`, `left_at`, `is_active`, `message_count`

#### `message_reactions` (Users ↔ Messages ↔ Emojis)
- **Composite PK**: `(message_id, user_id, emoji)` or separate `reaction_id`
- **FKs**: `message_id` → `chat_messages.message_id`, `user_id` → `users.user_id`

### 5. Additional Relationship Tables

#### `user_blocks` (User ↔ User blocking)
```sql
CREATE TABLE user_blocks (
  block_id UUID PRIMARY KEY,
  blocker_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  blocked_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  blocked_at TIMESTAMP DEFAULT NOW(),
  reason TEXT,

  UNIQUE KEY unique_block (blocker_id, blocked_id),
  CHECK (blocker_id != blocked_id)
);
```

#### `user_follows` (User ↔ User following)
```sql
CREATE TABLE user_follows (
  follow_id UUID PRIMARY KEY,
  follower_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  followed_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  followed_at TIMESTAMP DEFAULT NOW(),

  UNIQUE KEY unique_follow (follower_id, followed_id),
  CHECK (follower_id != followed_id)
);
```

#### `message_attachments` (Messages ↔ Files)
```sql
CREATE TABLE message_attachments (
  attachment_id UUID PRIMARY KEY,
  message_id UUID REFERENCES chat_messages(message_id) ON DELETE CASCADE,
  filename VARCHAR(255) NOT NULL,
  file_size INTEGER NOT NULL,
  mime_type VARCHAR(100) NOT NULL,
  file_url TEXT NOT NULL,
  thumbnail_url TEXT,
  uploaded_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_message_attachments (message_id)
);
```

#### `message_edits` (Message edit history)
```sql
CREATE TABLE message_edits (
  edit_id UUID PRIMARY KEY,
  message_id UUID REFERENCES chat_messages(message_id) ON DELETE CASCADE,
  old_content TEXT NOT NULL,
  new_content TEXT NOT NULL,
  edited_at TIMESTAMP DEFAULT NOW(),
  edited_by UUID REFERENCES users(user_id) ON DELETE SET NULL,

  INDEX idx_message_edits (message_id, edited_at)
);
```

## ORM/Database Compatibility Matrix

### PostgreSQL (Production)
```sql
-- Full support for all features
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm"; -- For full-text search

-- JSONB columns with GIN indexes
ALTER TABLE users ADD COLUMN preferences JSONB DEFAULT '{}';
CREATE INDEX idx_users_preferences_gin ON users USING GIN (preferences);

-- Arrays with GIN indexes
ALTER TABLE chat_messages ADD COLUMN mentions UUID[] DEFAULT '{}';
CREATE INDEX idx_messages_mentions_gin ON chat_messages USING GIN (mentions);

-- Full-text search
ALTER TABLE chat_messages ADD COLUMN content_tsvector tsvector;
CREATE INDEX idx_messages_fulltext ON chat_messages USING GIN (content_tsvector);
```

### SQLite (Development/Small Scale)
```sql
-- Limited JSON support, use TEXT columns
CREATE TABLE users (
  user_id TEXT PRIMARY KEY,
  preferences TEXT DEFAULT '{}', -- JSON as TEXT
  -- No arrays, use JSON TEXT: '["uuid1", "uuid2"]'
);

-- No native UUID, use TEXT
-- No ENUM, use TEXT with CHECK constraints
-- No GIN indexes, use regular B-tree
```

### MongoDB (Document Store)
```javascript
// Users collection with embedded relationships
{
  _id: ObjectId(),
  userId: UUID(),
  userType: "human",
  displayName: "Joel",

  // Embedded one-to-many
  sessions: [
    { sessionId: UUID(), startedAt: Date(), isActive: true },
    { sessionId: UUID(), startedAt: Date(), isActive: false }
  ],

  // Reference many-to-many
  roomMemberships: [
    { roomId: UUID(), role: "admin", joinedAt: Date() }
  ],

  // Denormalized for performance
  messageCount: 1234,
  lastActiveAt: Date()
}

// Messages collection with populated references
{
  _id: ObjectId(),
  messageId: UUID(),
  roomId: UUID(),
  senderId: UUID(),
  content: "Hello world",

  // Embedded reactions
  reactions: [
    { userId: UUID(), emoji: "👍", reactedAt: Date() }
  ],

  // Reference to parent message
  replyTo: {
    messageId: UUID(),
    content: "Original message snippet"
  }
}
```

### Redis (Caching/Sessions)
```redis
# User session cache
HMSET user:session:{sessionId}
  userId {userId}
  isActive true
  lastActivity {timestamp}

# Room member lists
SADD room:members:{roomId} {userId1} {userId2} {userId3}

# Message cache (recent messages per room)
LPUSH room:messages:{roomId} {messageJson}
LTRIM room:messages:{roomId} 0 99  # Keep last 100
```

## Adapter-Agnostic Relationship Handling

### 1. Repository Pattern with Relationship Loading

```typescript
interface UserRepository {
  // Basic CRUD
  findById(id: UUID): Promise<User | null>;

  // Relationship loading
  findWithSessions(id: UUID): Promise<User & { sessions: UserSession[] }>;
  findWithRooms(id: UUID): Promise<User & { rooms: ChatRoom[] }>;
  findWithPermissions(id: UUID): Promise<User & { permissions: UserPermission[] }>;

  // Complex queries
  findActiveInRoom(roomId: UUID): Promise<User[]>;
  findWithRole(roomId: UUID, role: string): Promise<User[]>;
}
```

### 2. Relationship Loading Strategies per Adapter

#### SQL Adapters (JOIN-based)
```typescript
class SQLUserRepository implements UserRepository {
  async findWithSessions(id: UUID): Promise<User & { sessions: UserSession[] }> {
    const query = `
      SELECT u.*, s.session_id, s.is_active, s.started_at
      FROM users u
      LEFT JOIN user_sessions s ON u.user_id = s.user_id
      WHERE u.user_id = ? AND s.is_active = true
    `;

    const rows = await this.db.query(query, [id]);
    return this.mapWithSessions(rows);
  }
}
```

#### NoSQL Adapters (Lookup-based)
```typescript
class MongoUserRepository implements UserRepository {
  async findWithSessions(id: UUID): Promise<User & { sessions: UserSession[] }> {
    const user = await this.users.findOne({ userId: id });
    const sessions = await this.sessions.find({ userId: id, isActive: true });

    return { ...user, sessions };
  }
}
```

#### File Adapters (Manual resolution)
```typescript
class FileUserRepository implements UserRepository {
  async findWithSessions(id: UUID): Promise<User & { sessions: UserSession[] }> {
    const user = await this.loadFromFile(`users/${id}.json`);
    const sessionsDir = await this.listFiles('user_sessions/');

    const sessions = [];
    for (const file of sessionsDir) {
      const session = await this.loadFromFile(`user_sessions/${file}`);
      if (session.userId === id && session.isActive) {
        sessions.push(session);
      }
    }

    return { ...user, sessions };
  }
}
```

### 3. Universal Query Builder

```typescript
class RelationshipQueryBuilder {
  // Build relationship queries per adapter type
  static buildJoinQuery(adapterType: AdapterType, relationships: string[]): Query {
    switch (adapterType) {
      case 'sql':
        return this.buildSQLJoins(relationships);
      case 'nosql':
        return this.buildLookupPipeline(relationships);
      case 'file':
        return this.buildFileLoadPlan(relationships);
    }
  }

  private static buildSQLJoins(relationships: string[]): SQLQuery {
    let query = 'SELECT u.*';
    let joins = '';

    if (relationships.includes('sessions')) {
      query += ', s.session_id, s.is_active, s.started_at';
      joins += ' LEFT JOIN user_sessions s ON u.user_id = s.user_id';
    }

    if (relationships.includes('rooms')) {
      query += ', r.room_id, r.name, rm.role';
      joins += `
        LEFT JOIN room_memberships rm ON u.user_id = rm.user_id
        LEFT JOIN chat_rooms r ON rm.room_id = r.room_id
      `;
    }

    return { select: query, joins, where: 'u.user_id = ?' };
  }
}
```

## Detailed Data Migration Plan

### Phase 1: Schema Creation & Dual-Write System (Week 1-2)

#### Step 1: Create New Schema
```sql
-- Deploy new tables alongside existing JSON system
-- Start with core entities: users, chat_rooms, chat_messages
-- Add foreign key constraints
-- Create indexes for performance
```

#### Step 2: Implement Dual-Write
```typescript
class DualWriteUserRepository {
  async createUser(userData: CreateUserData): Promise<User> {
    // Write to new database
    const newUser = await this.sqlRepo.create(userData);

    // Also write to legacy JSON (for compatibility)
    try {
      await this.jsonRepo.create(userData);
    } catch (error) {
      console.warn('Legacy JSON write failed:', error);
      // Don't fail the operation - new system is source of truth
    }

    return newUser;
  }
}
```

### Phase 2: Data Migration Scripts (Week 3-4)

#### Migration Script Architecture
```typescript
class DataMigrationManager {
  async migrateUsers(): Promise<MigrationResult> {
    const jsonUsers = await this.loadAllJSONUsers();
    const migrationStats = { success: 0, failed: 0, errors: [] };

    for (const jsonUser of jsonUsers) {
      try {
        // Transform JSON to new schema
        const newUser = this.transformUser(jsonUser);

        // Insert with relationship mapping
        await this.sqlRepo.createWithRelationships(newUser);
        migrationStats.success++;

      } catch (error) {
        migrationStats.failed++;
        migrationStats.errors.push({ user: jsonUser.id, error });
      }
    }

    return migrationStats;
  }

  private transformUser(jsonUser: any): UserWithRelationships {
    return {
      userId: jsonUser.id,
      userType: this.mapUserType(jsonUser.citizenType),
      displayName: jsonUser.name,
      // Map relationships
      sessions: this.findUserSessions(jsonUser.id),
      roomMemberships: this.findUserRooms(jsonUser.id)
    };
  }
}
```

#### Relationship Mapping
```typescript
class RelationshipMapper {
  async mapChatMessages(): Promise<void> {
    const messages = await this.loadJSONMessages();

    for (const msg of messages) {
      // Find existing user by session or create placeholder
      const userId = await this.findOrCreateUser(msg.citizenId);

      // Find or create room
      const roomId = await this.findOrCreateRoom(msg.roomId);

      // Create message with proper foreign keys
      await this.sqlRepo.createMessage({
        messageId: msg.messageId,
        senderId: userId,
        roomId: roomId,
        content: msg.content,
        sentAt: msg.timestamp
      });
    }
  }
}
```

### Phase 3: Read Migration & Testing (Week 5-6)

#### Gradual Read Migration
```typescript
class HybridReadRepository {
  async findUser(id: UUID): Promise<User | null> {
    // Try new database first
    let user = await this.sqlRepo.findById(id);

    if (!user) {
      // Fallback to JSON and trigger migration
      user = await this.jsonRepo.findById(id);
      if (user) {
        // Lazy migration
        await this.migrateUserAndRelationships(user);
      }
    }

    return user;
  }
}
```

### Phase 4: Command Migration (Week 7-8)

#### Update Commands to Use New Repositories
```typescript
// OLD: Direct JSON file operations
class ChatSendMessageCommand {
  async execute(params: ChatSendMessageParams): Promise<ChatSendMessageResult> {
    // Create data record
    const dataRecord = { /* ... */ };

    // Save to JSON file
    await this.remoteExecute(dataRecord, 'data/create');
  }
}

// NEW: Repository with relationships
class ChatSendMessageCommand {
  async execute(params: ChatSendMessageParams): Promise<ChatSendMessageResult> {
    // Get user and room (with validation)
    const user = await this.userRepo.findById(params.senderId);
    const room = await this.roomRepo.findById(params.roomId);

    // Verify user is member of room
    const membership = await this.roomRepo.getMembership(room.roomId, user.userId);
    if (!membership?.isActive) {
      throw new Error('User not member of room');
    }

    // Create message with proper relationships
    const message = await this.messageRepo.create({
      content: params.content,
      senderId: user.userId,
      roomId: room.roomId,
      replyToId: params.replyToId
    });

    // Update room activity and user message count
    await this.roomRepo.updateActivity(room.roomId);
    await this.userRepo.incrementMessageCount(user.userId);

    return { success: true, message };
  }
}
```

### Phase 5: Legacy Cleanup (Week 9-10)

#### Remove JSON File System
1. Stop dual-writing to JSON files
2. Remove file-based data commands
3. Clean up `.continuum/database` directory
4. Update all commands to use repositories only

#### Validation & Rollback Plan
- Compare data integrity between old/new systems
- Performance benchmarks
- If critical issues found, rollback to JSON with lessons learned
- Document migration learnings for future database changes

### Migration Monitoring

#### Success Metrics
- 100% data migrated without loss
- Relationship integrity verified
- Performance equal or better than JSON system
- Zero breaking changes to API contracts
- All tests passing with new system

#### Risk Mitigation
- Full backup before migration
- Rollback procedure tested
- Staging environment validation
- Gradual rollout (dev → staging → production)
- 24/7 monitoring during migration window

---

**Result**: Complete relationship specification with concrete migration plan, ORM compatibility matrix, and adapter-agnostic relationship handling strategy.