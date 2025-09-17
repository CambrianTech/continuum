# Discord-Like BaseUser Features

## Essential Chat Features Every BaseUser Needs

### Core Profile (Simple)
```typescript
interface BaseUserProfile {
  userId: UUID;
  displayName: string;
  avatar?: string;           // Profile picture URL
  status: 'online' | 'away' | 'busy' | 'invisible' | 'offline';
  customStatus?: string;     // "Working on code" or emoji status
}
```

### Presence & Activity
```typescript
interface UserPresence {
  userId: UUID;
  status: 'online' | 'away' | 'busy' | 'invisible' | 'offline';
  lastSeen: string;
  currentActivity?: {
    type: 'typing' | 'speaking' | 'idle';
    location?: string;      // Which room they're active in
    startedAt: string;
  };
}
```

### Chat Relationships (Discord-style)
```typescript
interface UserChatFeatures {
  // Friend system
  friends: UUID[];
  friendRequests: {
    incoming: UUID[];
    outgoing: UUID[];
  };

  // Blocked users
  blockedUsers: UUID[];

  // Direct message history
  dmRooms: UUID[];          // 1-on-1 chat rooms

  // Server/room relationships
  joinedRooms: Array<{
    roomId: UUID;
    nickname?: string;      // Per-room nickname
    role: 'member' | 'moderator' | 'admin';
    joinedAt: string;
    notifications: 'all' | 'mentions' | 'none';
  }>;
}
```

### Message Features
```typescript
interface MessageCapabilities {
  // What they can do in chat
  canSendMessages: boolean;
  canSendAttachments: boolean;
  canReact: boolean;
  canReply: boolean;
  canEdit: boolean;        // Edit their own messages
  canDelete: boolean;      // Delete their own messages
  canPin: boolean;         // Pin messages (if mod/admin)

  // Rate limiting
  messageRateLimit?: {
    maxPerMinute: number;
    cooldownMs: number;
  };
}
```

### Simple Database Schema

#### Updated Users Table
```sql
CREATE TABLE users (
  user_id UUID PRIMARY KEY,
  session_id UUID NOT NULL,

  -- BaseUser + Discord essentials
  display_name VARCHAR(100) NOT NULL,
  citizen_type ENUM('human', 'ai') NOT NULL,
  avatar_url TEXT,
  status ENUM('online', 'away', 'busy', 'invisible', 'offline') DEFAULT 'offline',
  custom_status VARCHAR(200),

  -- Activity tracking
  last_seen TIMESTAMP DEFAULT NOW(),
  is_typing_in_room UUID REFERENCES chat_rooms(room_id), -- NULL when not typing

  -- Simple capabilities (JSON for now)
  capabilities JSONB DEFAULT '{"canSendMessages": true, "canReact": true}',

  -- Basic preferences
  notification_settings JSONB DEFAULT '{"mentions": true, "dms": true}',

  -- Type-specific fields (from previous spec)
  ai_type ENUM('agent', 'persona') NULL,
  model_config JSONB,
  -- ... rest of inheritance fields
);
```

#### User Relationships Table
```sql
CREATE TABLE user_relationships (
  relationship_id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  target_user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  relationship_type ENUM('friend', 'blocked', 'friend_request') NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),

  UNIQUE KEY unique_relationship (user_id, target_user_id, relationship_type),
  CHECK (user_id != target_user_id)
);
```

#### Room Memberships (Enhanced)
```sql
CREATE TABLE room_memberships (
  membership_id UUID PRIMARY KEY,
  room_id UUID REFERENCES chat_rooms(room_id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,

  -- Discord-like features
  nickname VARCHAR(100), -- Per-room nickname
  role ENUM('member', 'moderator', 'admin', 'owner') DEFAULT 'member',
  notification_level ENUM('all', 'mentions', 'none') DEFAULT 'all',

  -- Membership tracking
  joined_at TIMESTAMP DEFAULT NOW(),
  left_at TIMESTAMP NULL,
  is_active BOOLEAN DEFAULT TRUE,

  -- Activity in room
  message_count INTEGER DEFAULT 0,
  last_message_at TIMESTAMP NULL,

  UNIQUE KEY unique_membership (room_id, user_id),
  INDEX idx_room_members (room_id, is_active),
  INDEX idx_user_rooms (user_id, is_active)
);
```

#### Typing Indicators
```sql
CREATE TABLE typing_indicators (
  indicator_id UUID PRIMARY KEY,
  room_id UUID REFERENCES chat_rooms(room_id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  started_at TIMESTAMP DEFAULT NOW(),

  -- Auto-expire after 10 seconds
  expires_at TIMESTAMP DEFAULT (NOW() + INTERVAL '10 seconds'),

  UNIQUE KEY unique_typing (room_id, user_id),
  INDEX idx_room_typing (room_id, expires_at)
);
```

### Repository Methods for Chat Features

```typescript
class BaseUserRepository {
  // Basic profile
  async updateStatus(userId: UUID, status: UserStatus): Promise<void>
  async updateCustomStatus(userId: UUID, customStatus: string): Promise<void>
  async updateAvatar(userId: UUID, avatarUrl: string): Promise<void>

  // Friends & blocking
  async sendFriendRequest(fromUserId: UUID, toUserId: UUID): Promise<void>
  async acceptFriendRequest(userId: UUID, fromUserId: UUID): Promise<void>
  async blockUser(userId: UUID, blockedUserId: UUID): Promise<void>
  async getFriends(userId: UUID): Promise<BaseUser[]>
  async getBlockedUsers(userId: UUID): Promise<UUID[]>

  // Room relationships
  async joinRoom(userId: UUID, roomId: UUID, role?: string): Promise<void>
  async leaveRoom(userId: UUID, roomId: UUID): Promise<void>
  async setRoomNickname(userId: UUID, roomId: UUID, nickname: string): Promise<void>
  async setNotificationLevel(userId: UUID, roomId: UUID, level: string): Promise<void>

  // Activity tracking
  async setTyping(userId: UUID, roomId: UUID): Promise<void>
  async clearTyping(userId: UUID, roomId?: UUID): Promise<void>
  async getTypingUsers(roomId: UUID): Promise<BaseUser[]>

  // Message capabilities
  async canUserSendMessage(userId: UUID, roomId: UUID): Promise<boolean>
  async canUserReact(userId: UUID, roomId: UUID): Promise<boolean>
  async getUserCapabilities(userId: UUID): Promise<MessageCapabilities>
}
```

### Widget Integration

```typescript
// What chat widgets need from BaseUser
interface ChatWidgetUserData {
  userId: UUID;
  displayName: string;
  avatar?: string;
  status: UserStatus;
  customStatus?: string;
  isTyping: boolean;
  isOnline: boolean;
  lastSeen: string;

  // Room-specific
  roomNickname?: string;
  role: string;
  canSendMessages: boolean;
  canReact: boolean;

  // Relationship status (if DM)
  isFriend?: boolean;
  isBlocked?: boolean;
}
```

### Real-time Updates

```typescript
// Events that widgets need to listen for
const CHAT_USER_EVENTS = {
  STATUS_CHANGED: 'user:status-changed',
  CUSTOM_STATUS_CHANGED: 'user:custom-status-changed',
  STARTED_TYPING: 'user:started-typing',
  STOPPED_TYPING: 'user:stopped-typing',
  JOINED_ROOM: 'user:joined-room',
  LEFT_ROOM: 'user:left-room',
  NICKNAME_CHANGED: 'user:nickname-changed',
  FRIEND_REQUEST_RECEIVED: 'user:friend-request-received',
  FRIEND_ADDED: 'user:friend-added',
  USER_BLOCKED: 'user:blocked'
} as const;
```

## Key Insight

Keep it simple but Discord-complete:
- ✅ Status, avatar, custom status
- ✅ Friends, blocking, friend requests
- ✅ Per-room nicknames and notification settings
- ✅ Typing indicators
- ✅ Message capabilities per user/room
- ✅ Real-time presence updates

Every BaseUser (human, agent, persona) gets these features automatically through the inheritance hierarchy and shared database relationships.