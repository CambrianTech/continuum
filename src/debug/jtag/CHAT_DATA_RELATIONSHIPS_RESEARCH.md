# Chat & Data Architecture Research - Current vs Future

## Current System Analysis (The Hack)

### What Exists Now
- **Chat Daemon**: Participant-agnostic architecture with `SessionParticipant`
- **Domain Objects**: `BaseUser` → `HumanUser`/`AIUser` → `AgentUser`/`PersonaUser`, `ChatMessage`, `ChatRoom`
- **Commands**: `chat/send-message`, `data/create`, `data/list`, etc.
- **Storage**: JSON files `.continuum/database/{collection}/{id}.json`
- **Events**: Real-time event distribution via `RoomEventSystem`

### Current Problems (Why It's a Hack)
1. **No Real Relationships**: Domain objects exist but no foreign keys, joins, or proper relationships
2. **Command Chaining**: `chat/send-message` → calls `data/create` → saves JSON file (brittle)
3. **Inconsistent Data Models**: Multiple type definitions for same entities
4. **JSON File Storage**: Not a real database, no transactions, no referential integrity
5. **Mixed Abstractions**: Chat daemon knows about data commands, data commands know about files

## Future System Design - Proper Database Relationships

### Core Entities with Real Relationships

#### Users Table
```sql
CREATE TABLE users (
  user_id UUID PRIMARY KEY,
  session_id UUID NOT NULL,
  user_type ENUM('human', 'ai') NOT NULL,
  ai_type ENUM('agent', 'persona') NULL, -- Only for AI users
  display_name VARCHAR(255) NOT NULL,
  email VARCHAR(255) UNIQUE,
  avatar_url TEXT,
  status ENUM('online', 'offline', 'away', 'busy') DEFAULT 'offline',
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),
  last_active_at TIMESTAMP DEFAULT NOW(),

  -- AI-specific fields
  model_config JSONB, -- For AI users: provider, model, context_window, etc.
  system_prompt TEXT, -- For personas
  personality_traits JSONB, -- For personas
  portal_config JSONB, -- For agents: endpoint, api_key, etc.

  -- User preferences
  preferences JSONB DEFAULT '{}',

  INDEX idx_user_type (user_type, ai_type),
  INDEX idx_status (status),
  INDEX idx_last_active (last_active_at)
);
```

#### Chat Rooms Table
```sql
CREATE TABLE chat_rooms (
  room_id UUID PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  description TEXT,
  room_type ENUM('public', 'private', 'direct', 'system') DEFAULT 'public',
  created_by UUID REFERENCES users(user_id) ON DELETE SET NULL,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),
  last_activity_at TIMESTAMP DEFAULT NOW(),
  is_archived BOOLEAN DEFAULT FALSE,
  max_members INTEGER DEFAULT 100,

  -- Room settings
  allow_ai BOOLEAN DEFAULT TRUE,
  require_moderation BOOLEAN DEFAULT FALSE,
  message_retention_days INTEGER DEFAULT 90,
  room_settings JSONB DEFAULT '{}',

  INDEX idx_room_type (room_type),
  INDEX idx_created_by (created_by),
  INDEX idx_last_activity (last_activity_at),
  INDEX idx_archived (is_archived)
);
```

#### Room Memberships Table (Many-to-Many)
```sql
CREATE TABLE room_memberships (
  membership_id UUID PRIMARY KEY,
  room_id UUID REFERENCES chat_rooms(room_id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  role ENUM('member', 'moderator', 'admin', 'owner') DEFAULT 'member',
  joined_at TIMESTAMP DEFAULT NOW(),
  left_at TIMESTAMP NULL,
  is_active BOOLEAN DEFAULT TRUE,
  message_count INTEGER DEFAULT 0,
  last_message_at TIMESTAMP NULL,

  -- Unique constraint: one active membership per user per room
  UNIQUE KEY unique_active_membership (room_id, user_id, is_active),
  INDEX idx_room_members (room_id, is_active),
  INDEX idx_user_rooms (user_id, is_active),
  INDEX idx_role (role)
);
```

#### Chat Messages Table
```sql
CREATE TABLE chat_messages (
  message_id UUID PRIMARY KEY,
  room_id UUID REFERENCES chat_rooms(room_id) ON DELETE CASCADE,
  sender_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
  content TEXT NOT NULL,
  message_type ENUM('text', 'image', 'file', 'system', 'bot_response') DEFAULT 'text',
  sent_at TIMESTAMP DEFAULT NOW(),
  edited_at TIMESTAMP NULL,
  deleted_at TIMESTAMP NULL,
  reply_to_id UUID REFERENCES chat_messages(message_id) ON DELETE SET NULL,

  -- Message metadata
  mentions JSONB DEFAULT '[]', -- Array of user_ids mentioned
  attachments JSONB DEFAULT '[]', -- File attachments
  ai_context JSONB, -- For AI responses: model, processing_time, etc.

  INDEX idx_room_messages (room_id, sent_at DESC),
  INDEX idx_sender_messages (sender_id, sent_at DESC),
  INDEX idx_reply_thread (reply_to_id),
  INDEX idx_mentions ((CAST(mentions AS CHAR(255) ARRAY))), -- GIN index for mentions
  FULLTEXT INDEX idx_content_search (content)
);
```

#### User Sessions Table
```sql
CREATE TABLE user_sessions (
  session_id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  session_token VARCHAR(255) UNIQUE NOT NULL,
  device_info JSONB, -- user_agent, ip_address, platform
  is_active BOOLEAN DEFAULT TRUE,
  started_at TIMESTAMP DEFAULT NOW(),
  last_activity_at TIMESTAMP DEFAULT NOW(),
  ended_at TIMESTAMP NULL,

  INDEX idx_user_sessions (user_id, is_active),
  INDEX idx_session_token (session_token),
  INDEX idx_last_activity (last_activity_at)
);
```

#### User Permissions Table
```sql
CREATE TABLE user_permissions (
  permission_id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  permission VARCHAR(100) NOT NULL, -- 'read_messages', 'send_messages', 'admin', etc.
  resource VARCHAR(255) DEFAULT '*', -- '*', 'room:{room_id}', 'user:{user_id}'
  granted_by UUID REFERENCES users(user_id) ON DELETE SET NULL,
  granted_at TIMESTAMP DEFAULT NOW(),
  expires_at TIMESTAMP NULL,

  UNIQUE KEY unique_permission (user_id, permission, resource),
  INDEX idx_user_permissions (user_id),
  INDEX idx_resource_permissions (resource)
);
```

#### Message Reactions Table
```sql
CREATE TABLE message_reactions (
  reaction_id UUID PRIMARY KEY,
  message_id UUID REFERENCES chat_messages(message_id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  emoji VARCHAR(50) NOT NULL,
  reacted_at TIMESTAMP DEFAULT NOW(),

  UNIQUE KEY unique_reaction (message_id, user_id, emoji),
  INDEX idx_message_reactions (message_id),
  INDEX idx_user_reactions (user_id)
);
```

### Key Relationships

1. **Users ↔ Chat Rooms**: Many-to-many via `room_memberships`
2. **Users ↔ Messages**: One-to-many (`messages.sender_id`)
3. **Rooms ↔ Messages**: One-to-many (`messages.room_id`)
4. **Messages ↔ Messages**: Self-referencing via `reply_to_id` (threads)
5. **Users ↔ Sessions**: One-to-many (multiple devices/tabs)
6. **Users ↔ Permissions**: One-to-many (granular permissions)
7. **Messages ↔ Reactions**: One-to-many (emoji reactions)

### Proper User Profiles for Chat

#### Human User Profile
```typescript
interface HumanUserProfile {
  userId: UUID;
  displayName: string;
  email?: string;
  avatarUrl?: string;
  bio?: string;
  location?: string;
  timezone: string;

  // Preferences
  theme: 'light' | 'dark' | 'auto';
  language: string;
  notificationSettings: {
    mentions: boolean;
    directMessages: boolean;
    roomUpdates: boolean;
    email: boolean;
    push: boolean;
  };

  // Privacy settings
  showOnlineStatus: boolean;
  allowDirectMessages: boolean;
  shareActivity: boolean;

  // Activity data
  totalMessages: number;
  joinedRooms: number;
  lastActiveAt: string;
  accountCreatedAt: string;
}
```

#### AI User Profile
```typescript
interface AIUserProfile {
  userId: UUID;
  displayName: string;
  aiType: 'agent' | 'persona';

  // Model configuration
  modelConfig: {
    provider: 'openai' | 'anthropic' | 'local';
    model: string;
    contextWindow: number;
    temperature?: number;
    maxTokens?: number;
  };

  // Personality (for personas)
  personality?: {
    systemPrompt: string;
    traits: string[];
    expertise: string[];
    responseStyle: string;
  };

  // Agent portal (for agents)
  portalConfig?: {
    endpoint: string;
    apiKey?: string;
    webhookUrl?: string;
  };

  // Response behavior
  responseSettings: {
    autoRespond: boolean;
    triggers: Array<{
      type: 'mention' | 'keyword' | 'question';
      value?: string;
      probability?: number;
    }>;
    maxResponsesPerMinute: number;
    respectTurns: boolean;
  };

  // Usage stats
  totalResponses: number;
  averageResponseTime: number;
  lastActiveAt: string;
}
```

### Future Architecture Benefits

1. **Real Relationships**: Foreign keys, joins, referential integrity
2. **Scalability**: Proper indexing, query optimization
3. **Data Integrity**: Transactions, constraints, validation
4. **Flexibility**: JSON fields for extensibility, structured data for queries
5. **Performance**: Efficient queries, caching strategies
6. **Multi-backend Support**: Same schema works with PostgreSQL, MySQL, SQLite

### Migration Strategy

1. **Phase 1**: Create new schema alongside existing JSON system
2. **Phase 2**: Migrate existing data with relationship mapping
3. **Phase 3**: Update domain objects to use new relationships
4. **Phase 4**: Replace JSON commands with proper repository layer
5. **Phase 5**: Remove legacy JSON file system

---

**Bottom Line**: Current system is JSON files with command chaining. Future system is proper relational database with real foreign keys, relationships, and integrity. Design for the future, not the hack.