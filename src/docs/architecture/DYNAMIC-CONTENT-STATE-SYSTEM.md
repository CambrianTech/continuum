# FIRM DESIGN: Dynamic Content State System

## 🎯 PROBLEM STATEMENT

Current state management is fundamentally broken:
- ChatWidget hardcodes `currentRoomId = DEFAULT_ROOMS.GENERAL`
- Zero dynamic content management
- No state persistence
- No multi-user/multi-device support
- No AI agent content manipulation
- Manual event subscriptions everywhere

**This blocks: Multi-user chat, AI agents, code editor, LoRA trainer, collaborative workspaces**

## 🏗️ COMPLETE ENTITY DESIGN

### UserState Entity (DATABASE)
```typescript
interface UserStateEntity extends BaseEntity {
  // Primary identification
  id: UUID;                          // Primary key
  userId: UUID;                      // Foreign key to User entity
  deviceId?: string;                 // Optional device identifier

  // Content management
  openContent: ContentDescriptor[];  // Array of open content pieces
  currentContentId: UUID;            // Active content ID
  contentHistory: UUID[];            // Recently accessed content (max 50)

  // Preferences
  preferences: UserPreferences;

  // Metadata
  version: number;                   // For conflict resolution
  lastModified: Date;               // Sync timestamp
  lastActiveAt: Date;               // User activity timestamp
}

interface ContentDescriptor {
  id: UUID;                         // Unique content instance ID
  contentType: string;              // 'chat-room', 'direct-message', 'code-file'
  title: string;                    // Display name for tab
  context: Record<string, any>;     // Type-specific context data

  // State tracking
  isDirty: boolean;                 // Has unsaved changes
  isPinned: boolean;                // User pinned this content
  position: number;                 // Tab order position

  // Metadata
  createdAt: Date;                  // When content was opened
  lastAccessedAt: Date;             // Last user interaction
  accessCount: number;              // Usage frequency
}

interface UserPreferences {
  // UI preferences
  tabClosePolicy: 'manual' | 'auto-close-after-idle' | 'auto-close-limit';
  maxOpenTabs: number;              // Auto-close after N tabs (default 10)
  autoCloseAfterMinutes: number;    // Auto-close idle tabs (default 60)

  // Content preferences
  defaultContentTypes: string[];    // Preferred content types to open
  contentTypePrefs: Record<string, any>; // Per-type preferences
}
```

### ContentType Registry Entity (DATABASE)
```typescript
interface ContentTypeEntity extends BaseEntity {
  id: UUID;
  contentType: string;              // 'chat-room' (unique)
  displayName: string;              // 'Chat Room'
  description: string;              // 'Real-time messaging in a room'

  // Widget binding
  widgetClass: string;              // 'ChatWidget'
  widgetModule: string;             // Module path for dynamic loading

  // UI properties
  icon: string;                     // '💬' or '/assets/icons/chat.svg'
  category: string;                 // 'communication'
  color: string;                    // '#00d4ff' for tab theming

  // Behavior
  allowMultiple: boolean;           // Can have multiple instances open
  persistContext: boolean;          // Save context across sessions
  defaultContext: Record<string, any>; // Default context values
  contextSchema: JSONSchema;        // Validation schema for context

  // Permissions
  requiredPermissions: string[];    // ['chat.access', 'room.join']
  allowedUserTypes: string[];       // ['human', 'agent', 'persona']

  // Metadata
  isActive: boolean;                // Is this content type enabled
  createdAt: Date;
  version: string;                  // Content type version
}
```

## 🗄️ DATABASE SCHEMA

### UserState Table
```sql
CREATE TABLE user_states (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id VARCHAR(255),

  -- JSON columns for complex data
  open_content JSONB NOT NULL DEFAULT '[]',
  current_content_id UUID,
  content_history JSONB NOT NULL DEFAULT '[]',
  preferences JSONB NOT NULL DEFAULT '{}',

  -- Metadata
  version INTEGER NOT NULL DEFAULT 1,
  last_modified TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_active_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

  -- Constraints
  CONSTRAINT user_states_user_device_unique UNIQUE (user_id, device_id),
  CONSTRAINT user_states_content_history_size CHECK (jsonb_array_length(content_history) <= 50)
);

-- Indexes for performance
CREATE INDEX idx_user_states_user_id ON user_states(user_id);
CREATE INDEX idx_user_states_last_active ON user_states(last_active_at);
CREATE INDEX idx_user_states_current_content ON user_states(current_content_id);
CREATE INDEX idx_user_states_open_content_gin ON user_states USING gin(open_content);
```

### ContentType Registry Table
```sql
CREATE TABLE content_types (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  content_type VARCHAR(255) NOT NULL UNIQUE,
  display_name VARCHAR(255) NOT NULL,
  description TEXT,

  -- Widget binding
  widget_class VARCHAR(255) NOT NULL,
  widget_module VARCHAR(500) NOT NULL,

  -- UI properties
  icon VARCHAR(255) NOT NULL DEFAULT '📄',
  category VARCHAR(100) NOT NULL DEFAULT 'general',
  color VARCHAR(7) NOT NULL DEFAULT '#666666',

  -- Behavior configuration
  allow_multiple BOOLEAN NOT NULL DEFAULT true,
  persist_context BOOLEAN NOT NULL DEFAULT true,
  default_context JSONB NOT NULL DEFAULT '{}',
  context_schema JSONB NOT NULL DEFAULT '{}',

  -- Permissions
  required_permissions JSONB NOT NULL DEFAULT '[]',
  allowed_user_types JSONB NOT NULL DEFAULT '["human"]',

  -- Metadata
  is_active BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  version VARCHAR(50) NOT NULL DEFAULT '1.0.0'
);

-- Indexes
CREATE INDEX idx_content_types_active ON content_types(is_active) WHERE is_active = true;
CREATE INDEX idx_content_types_category ON content_types(category);
```

## 🔄 COMPLETE EVENT ARCHITECTURE

### Event Types
```typescript
// Content management events
interface ContentOpenEvent {
  eventType: 'content:open';
  userId: UUID;
  contentType: string;
  context: Record<string, any>;
  title: string;
  options?: {
    makeCurrent?: boolean;
    position?: number;
    isPinned?: boolean;
  };
}

interface ContentSwitchEvent {
  eventType: 'content:switch';
  userId: UUID;
  fromContentId: UUID;
  toContentId: UUID;
}

interface ContentCloseEvent {
  eventType: 'content:close';
  userId: UUID;
  contentId: UUID;
  saveContext?: boolean;
}

interface ContentUpdateEvent {
  eventType: 'content:update';
  userId: UUID;
  contentId: UUID;
  updates: Partial<ContentDescriptor>;
}

// State change events
interface UserStateChangedEvent {
  eventType: 'user:state:changed';
  userId: UUID;
  previousState: UserStateEntity;
  currentState: UserStateEntity;
  changeType: 'content-opened' | 'content-closed' | 'content-switched' | 'preferences-updated';
}

// Widget lifecycle events
interface WidgetContextChangedEvent {
  eventType: 'widget:context:changed';
  widgetType: string;
  previousContext: Record<string, any> | null;
  currentContext: Record<string, any>;
}
```

### Event Flow Architecture
```typescript
// 1. User Action → Content Event
Events.emit('content:open', {
  userId: currentUser.id,
  contentType: 'chat-room',
  context: { roomId: 'academy-123' },
  title: 'Academy',
  options: { makeCurrent: true }
});

// 2. ContentManager processes event → Database update
class ContentManager {
  async handleContentOpen(event: ContentOpenEvent) {
    const userState = await this.getUserState(event.userId);

    // Generate unique content ID
    const contentId = this.generateContentId(event.contentType, event.context);

    // Check if already open
    const existing = userState.openContent.find(c => c.id === contentId);
    if (existing) {
      // Just switch to existing content
      await this.switchContent(event.userId, contentId);
      return;
    }

    // Create new content descriptor
    const content: ContentDescriptor = {
      id: contentId,
      contentType: event.contentType,
      title: event.title,
      context: event.context,
      isDirty: false,
      isPinned: event.options?.isPinned ?? false,
      position: event.options?.position ?? userState.openContent.length,
      createdAt: new Date(),
      lastAccessedAt: new Date(),
      accessCount: 1
    };

    // Add to user state
    userState.openContent.push(content);
    if (event.options?.makeCurrent !== false) {
      userState.currentContentId = contentId;
    }

    // Save to database
    await this.saveUserState(userState);

    // Emit state changed event
    Events.emit('user:state:changed', {
      userId: event.userId,
      currentState: userState,
      changeType: 'content-opened'
    });
  }
}

// 3. Widgets respond to state changes
class ChatWidget extends BaseContentWidget {
  async onInitialize() {
    Events.subscribe('user:state:changed', this.handleStateChange.bind(this));
  }

  private async handleStateChange(event: UserStateChangedEvent) {
    const currentContent = event.currentState.openContent
      .find(c => c.id === event.currentState.currentContentId);

    if (currentContent && this.handlesContentType(currentContent.contentType)) {
      await this.setContext(currentContent.context);
    }
  }
}
```

## 🎛️ WIDGET CONTRACT SYSTEM

### BaseContentWidget Interface
```typescript
abstract class BaseContentWidget extends BaseWidget {
  // Required static properties
  abstract static readonly handledContentTypes: readonly string[];
  abstract static readonly widgetClass: string;

  // Required instance methods
  abstract async setContext(context: Record<string, any>): Promise<void>;
  abstract getTitle(context: Record<string, any>): string;
  abstract isDirty(): boolean;
  abstract async saveState(): Promise<void>;
  abstract async restoreState(context: Record<string, any>): Promise<void>;

  // Optional lifecycle hooks
  async onContentActivated(context: Record<string, any>): Promise<void> {}
  async onContentDeactivated(): Promise<void> {}
  async onContextChanged(oldContext: Record<string, any>, newContext: Record<string, any>): Promise<void> {}

  // Helper methods
  protected handlesContentType(contentType: string): boolean {
    return (this.constructor as typeof BaseContentWidget).handledContentTypes.includes(contentType);
  }

  protected async updateContentTitle(newTitle: string): Promise<void> {
    Events.emit('content:update', {
      userId: this.getCurrentUserId(),
      contentId: this.getCurrentContentId(),
      updates: { title: newTitle }
    });
  }

  protected async markDirty(isDirty: boolean = true): Promise<void> {
    Events.emit('content:update', {
      userId: this.getCurrentUserId(),
      contentId: this.getCurrentContentId(),
      updates: { isDirty }
    });
  }
}
```

### ChatWidget Implementation
```typescript
class ChatWidget extends BaseContentWidget {
  static readonly handledContentTypes = ['chat-room', 'direct-message', 'group-chat'] as const;
  static readonly widgetClass = 'ChatWidget';

  private currentRoomId: UUID | null = null;
  private currentParticipants: UUID[] = [];
  private messageCache = new Map<string, ChatMessageEntity[]>();

  async setContext(context: Record<string, any>): Promise<void> {
    console.log(`🔧 ChatWidget: Setting context`, context);

    // Validate context
    if (!context.roomId && !context.participants) {
      throw new Error('ChatWidget context must have roomId or participants');
    }

    const oldContext = { roomId: this.currentRoomId, participants: this.currentParticipants };

    // Update internal state
    if (context.roomId) {
      this.currentRoomId = context.roomId;
      this.currentParticipants = [];
    } else if (context.participants) {
      this.currentRoomId = null;
      this.currentParticipants = context.participants;
    }

    // Refresh entity scroller with new context
    if (this.scroller) {
      this.scroller.refresh();
    }

    // Update title display
    this.updateEntityCount();

    // Emit context change event
    Events.emit('widget:context:changed', {
      widgetType: 'ChatWidget',
      previousContext: oldContext,
      currentContext: context
    });
  }

  getTitle(context: Record<string, any>): string {
    if (context.roomId && context.roomName) {
      return context.roomName;
    } else if (context.participants) {
      return context.participants.join(', ');
    }
    return 'Chat';
  }

  isDirty(): boolean {
    // Chat is never dirty - messages are auto-saved
    return false;
  }

  async saveState(): Promise<void> {
    // No state to save for chat
  }

  async restoreState(context: Record<string, any>): Promise<void> {
    await this.setContext(context);
  }

  // Override EntityScrollerWidget methods to use dynamic context
  protected getEntityFilter(): Record<string, any> {
    if (this.currentRoomId) {
      return { roomId: this.currentRoomId };
    } else if (this.currentParticipants.length > 0) {
      return { participants: { $all: this.currentParticipants } };
    }
    return {}; // No filter = no messages
  }

  protected shouldAddEntity(entity: ChatMessageEntity): boolean {
    if (this.currentRoomId) {
      return entity.roomId === this.currentRoomId;
    } else if (this.currentParticipants.length > 0) {
      // For DMs, check if message is between current participants
      return this.currentParticipants.includes(entity.senderId);
    }
    return false;
  }
}
```

## 🔄 CONTENT LIFECYCLE MANAGEMENT

### Content State Machine
```typescript
enum ContentState {
  OPENING = 'opening',       // Being created/loaded
  ACTIVE = 'active',         // Currently displayed
  BACKGROUND = 'background', // Open but not active
  DIRTY = 'dirty',           // Has unsaved changes
  SAVING = 'saving',         // Persisting changes
  CLOSING = 'closing',       // Being closed
  CLOSED = 'closed'          // Fully closed
}

interface ContentLifecycle {
  state: ContentState;
  transitions: {
    from: ContentState;
    to: ContentState;
    trigger: string;
    guard?: (content: ContentDescriptor) => boolean;
    action?: (content: ContentDescriptor) => Promise<void>;
  }[];
}

const CONTENT_LIFECYCLE: ContentLifecycle = {
  state: ContentState.OPENING,
  transitions: [
    {
      from: ContentState.OPENING,
      to: ContentState.ACTIVE,
      trigger: 'content:loaded',
      action: async (content) => {
        await this.activateContent(content.id);
      }
    },
    {
      from: ContentState.ACTIVE,
      to: ContentState.BACKGROUND,
      trigger: 'content:deactivated',
      action: async (content) => {
        await this.saveContentState(content);
      }
    },
    {
      from: ContentState.BACKGROUND,
      to: ContentState.ACTIVE,
      trigger: 'content:activated'
    },
    {
      from: ContentState.ACTIVE,
      to: ContentState.DIRTY,
      trigger: 'content:modified'
    },
    {
      from: ContentState.DIRTY,
      to: ContentState.SAVING,
      trigger: 'content:save'
    },
    {
      from: ContentState.SAVING,
      to: ContentState.ACTIVE,
      trigger: 'content:saved'
    },
    {
      from: ContentState.BACKGROUND,
      to: ContentState.CLOSING,
      trigger: 'content:close',
      guard: (content) => !content.isDirty,
      action: async (content) => {
        await this.cleanupContent(content);
      }
    }
  ]
};
```

### Auto-cleanup Policies
```typescript
class ContentCleanupManager {
  private cleanupInterval: NodeJS.Timeout;

  constructor() {
    // Run cleanup every 5 minutes
    this.cleanupInterval = setInterval(this.runCleanup.bind(this), 5 * 60 * 1000);
  }

  private async runCleanup() {
    const userStates = await this.getAllUserStates();

    for (const userState of userStates) {
      await this.cleanupUserContent(userState);
    }
  }

  private async cleanupUserContent(userState: UserStateEntity) {
    const { preferences } = userState;
    const now = new Date();
    let needsSave = false;

    // Auto-close policy: close after idle time
    if (preferences.tabClosePolicy === 'auto-close-after-idle') {
      const idleThreshold = preferences.autoCloseAfterMinutes * 60 * 1000;

      userState.openContent = userState.openContent.filter(content => {
        const isIdle = (now.getTime() - content.lastAccessedAt.getTime()) > idleThreshold;
        if (isIdle && !content.isPinned && !content.isDirty) {
          console.log(`🧹 Auto-closing idle content: ${content.title}`);
          this.notifyContentClosed(userState.userId, content.id);
          needsSave = true;
          return false;
        }
        return true;
      });
    }

    // Auto-close policy: limit max tabs
    if (preferences.tabClosePolicy === 'auto-close-limit' && userState.openContent.length > preferences.maxOpenTabs) {
      // Sort by last accessed, keep most recent
      userState.openContent.sort((a, b) => b.lastAccessedAt.getTime() - a.lastAccessedAt.getTime());

      const toClose = userState.openContent
        .slice(preferences.maxOpenTabs)
        .filter(content => !content.isPinned && !content.isDirty);

      for (const content of toClose) {
        console.log(`🧹 Auto-closing excess content: ${content.title}`);
        this.notifyContentClosed(userState.userId, content.id);
        userState.openContent = userState.openContent.filter(c => c.id !== content.id);
        needsSave = true;
      }
    }

    if (needsSave) {
      await this.saveUserState(userState);
    }
  }
}
```

## 🌐 MULTI-USER & MULTI-DEVICE SYNC

### Conflict Resolution Strategy
```typescript
interface StateConflictResolution {
  strategy: 'last-writer-wins' | 'merge-arrays' | 'user-preference';

  async resolveConflict(
    localState: UserStateEntity,
    remoteState: UserStateEntity,
    conflictType: 'concurrent-update' | 'offline-sync'
  ): Promise<UserStateEntity> {

    // Version-based conflict detection
    if (localState.version !== remoteState.version) {
      console.warn(`🔄 State conflict detected for user ${localState.userId}`);

      // Strategy: Merge open content arrays, prefer most recent
      const mergedContent = this.mergeOpenContent(
        localState.openContent,
        remoteState.openContent
      );

      // Strategy: Prefer most recently active current content
      const currentContent = localState.lastActiveAt > remoteState.lastActiveAt
        ? localState.currentContentId
        : remoteState.currentContentId;

      // Strategy: Merge preferences (local overrides remote)
      const mergedPreferences = {
        ...remoteState.preferences,
        ...localState.preferences
      };

      return {
        ...remoteState,
        openContent: mergedContent,
        currentContentId: currentContent,
        preferences: mergedPreferences,
        version: Math.max(localState.version, remoteState.version) + 1,
        lastModified: new Date()
      };
    }

    return remoteState;
  }

  private mergeOpenContent(
    localContent: ContentDescriptor[],
    remoteContent: ContentDescriptor[]
  ): ContentDescriptor[] {
    const merged = new Map<UUID, ContentDescriptor>();

    // Add all remote content first
    for (const content of remoteContent) {
      merged.set(content.id, content);
    }

    // Merge local content, preferring more recent access times
    for (const content of localContent) {
      const existing = merged.get(content.id);
      if (!existing || content.lastAccessedAt > existing.lastAccessedAt) {
        merged.set(content.id, {
          ...existing,
          ...content,
          // Merge specific fields
          accessCount: (existing?.accessCount ?? 0) + content.accessCount
        });
      }
    }

    // Sort by position, then by last accessed
    return Array.from(merged.values())
      .sort((a, b) => {
        if (a.position !== b.position) {
          return a.position - b.position;
        }
        return b.lastAccessedAt.getTime() - a.lastAccessedAt.getTime();
      });
  }
}
```

### Real-time Sync Architecture
```typescript
class UserStateSync {
  private syncInterval: NodeJS.Timeout;
  private pendingChanges = new Map<UUID, Partial<UserStateEntity>>();

  constructor() {
    // Sync every 30 seconds
    this.syncInterval = setInterval(this.syncChanges.bind(this), 30000);

    // Listen for state changes to batch
    Events.subscribe('user:state:changed', this.queueStateChange.bind(this));
  }

  private queueStateChange(event: UserStateChangedEvent) {
    // Batch changes for efficiency
    const existing = this.pendingChanges.get(event.userId) || {};
    this.pendingChanges.set(event.userId, {
      ...existing,
      openContent: event.currentState.openContent,
      currentContentId: event.currentState.currentContentId,
      lastModified: new Date(),
      version: event.currentState.version + 1
    });

    // Emit sync event for real-time updates
    Events.emit('user:state:sync', {
      userId: event.userId,
      changes: this.pendingChanges.get(event.userId)
    });
  }

  private async syncChanges() {
    for (const [userId, changes] of this.pendingChanges) {
      try {
        await this.syncUserState(userId, changes);
        this.pendingChanges.delete(userId);
      } catch (error) {
        console.error(`❌ Failed to sync state for user ${userId}:`, error);
      }
    }
  }
}
```

## 🔒 SECURITY & PERMISSIONS

### Content Access Control
```typescript
interface ContentPermissions {
  contentType: string;
  requiredPermissions: string[];
  allowedUserTypes: ('human' | 'agent' | 'persona')[];
  customValidator?: (user: BaseUser, context: Record<string, any>) => Promise<boolean>;
}

class ContentSecurityManager {
  async validateContentAccess(
    userId: UUID,
    contentType: string,
    context: Record<string, any>
  ): Promise<boolean> {
    const user = await this.getUserById(userId);
    const contentTypeConfig = await this.getContentTypeConfig(contentType);

    // Check user type
    if (!contentTypeConfig.allowedUserTypes.includes(user.type)) {
      console.warn(`🔒 User type ${user.type} not allowed for content type ${contentType}`);
      return false;
    }

    // Check required permissions
    for (const permission of contentTypeConfig.requiredPermissions) {
      if (!await this.userHasPermission(user, permission)) {
        console.warn(`🔒 User ${userId} missing permission: ${permission}`);
        return false;
      }
    }

    // Custom validation (e.g., room membership for chat-room)
    if (contentTypeConfig.customValidator) {
      return await contentTypeConfig.customValidator(user, context);
    }

    return true;
  }

  async validateChatRoomAccess(user: BaseUser, context: { roomId: UUID }): Promise<boolean> {
    const room = await Commands.execute('data/read', {
      collection: 'Room',
      id: context.roomId
    });

    if (!room?.success || !room.data) {
      return false;
    }

    // Check room membership
    if (room.data.type === 'private') {
      return room.data.members?.includes(user.id) ?? false;
    }

    return true;
  }
}
```

## 🚀 MIGRATION STRATEGY

### Phase 1: Foundation (Week 1)
```typescript
// 1.1 Create database entities
await Commands.execute('data/create', {
  collection: 'ContentType',
  data: {
    contentType: 'chat-room',
    displayName: 'Chat Room',
    widgetClass: 'ChatWidget',
    allowMultiple: false,
    defaultContext: { roomId: DEFAULT_ROOMS.GENERAL }
  }
});

// 1.2 Create initial user state for existing users
const users = await Commands.execute('data/list', { collection: 'User' });
for (const user of users.items) {
  await Commands.execute('data/create', {
    collection: 'UserState',
    data: {
      userId: user.id,
      openContent: [{
        id: generateUUID(),
        contentType: 'chat-room',
        title: 'General',
        context: { roomId: DEFAULT_ROOMS.GENERAL },
        isDirty: false,
        isPinned: true,
        position: 0,
        createdAt: new Date(),
        lastAccessedAt: new Date(),
        accessCount: 1
      }],
      currentContentId: '...',  // ID from above
      preferences: {
        tabClosePolicy: 'manual',
        maxOpenTabs: 10,
        autoCloseAfterMinutes: 60
      }
    }
  });
}
```

### Phase 2: Widget Migration (Week 2)
```typescript
// 2.1 Modify ChatWidget to extend BaseContentWidget
class ChatWidget extends BaseContentWidget {
  // Remove hardcoded state
  // private currentRoomId: UUID | null = DEFAULT_ROOMS.GENERAL; ❌

  // Add dynamic context handling
  async setContext(context: Record<string, any>): Promise<void> {
    console.log('🔧 ChatWidget: Switching context', context);
    this.currentRoomId = context.roomId || null;
    if (this.scroller) {
      this.scroller.refresh(); // Will use new getEntityFilter()
    }
  }

  // Update filtering to use dynamic context
  protected getEntityFilter(): Record<string, any> {
    return this.currentRoomId ? { roomId: this.currentRoomId } : {};
  }
}
```

### Phase 3: Testing & Validation (Week 3)
```typescript
// 3.1 Ensure all CRUD tests pass
describe('Content State System', () => {
  it('should maintain ChatMessage CRUD functionality', async () => {
    // Create content state
    await ContentManager.openContent(userId, 'chat-room', { roomId: testRoomId }, 'Test Room');

    // Run existing CRUD test
    const result = await runCRUDTest('ChatMessage');
    expect(result.passed).toBe(true);
    expect(result.score).toBe('9/9');
  });

  it('should switch room context dynamically', async () => {
    await ContentManager.openContent(userId, 'chat-room', { roomId: roomA }, 'Room A');
    let messages = await getVisibleMessages();
    expect(messages.every(m => m.roomId === roomA)).toBe(true);

    await ContentManager.switchContent(userId, roomBContentId);
    messages = await getVisibleMessages();
    expect(messages.every(m => m.roomId === roomB)).toBe(true);
  });
});
```

## 🎯 SUCCESS METRICS

### Technical Metrics
- ✅ All 9/9 CRUD tests pass after migration
- ✅ Zero hardcoded `DEFAULT_ROOMS.GENERAL` references
- ✅ Content state persists across browser refresh
- ✅ Room switching works in <100ms
- ✅ Multi-tab support (up to 10 concurrent content pieces)
- ✅ Auto-cleanup removes idle content

### User Experience Metrics
- ✅ Click room → new tab opens in <200ms
- ✅ Click user → DM tab opens in <200ms
- ✅ Tab switching preserves scroll position
- ✅ State synchronizes across devices in <5s
- ✅ No UI glitches during content switching

### Architecture Metrics
- ✅ AI agents can programmatically open user content
- ✅ New content types can be added without core changes
- ✅ Widget registration system supports 20+ content types
- ✅ Event system handles 100+ concurrent users
- ✅ Database queries complete in <50ms (95th percentile)

## 🔥 CRITICAL DEPENDENCIES

### Must Have Before Starting
1. **Entity system working** - UserState/ContentType entities must persist ✅
2. **Event system reliable** - Real-time events between server/browser ✅
3. **CRUD tests passing** - Current 9/9 test suite must be stable ✅
4. **Widget infrastructure** - BaseWidget system must be solid ✅

### Implementation Blockers
1. **Database migration** - Need schema changes for UserState table
2. **ContentManager service** - Core service must be built first
3. **Widget contract** - BaseContentWidget interface must be finalized
4. **Event architecture** - All event types must be defined

**Decision Point: This design is comprehensive and implementable. Proceed with Phase 1?**

## 🏁 FINAL IMPLEMENTATION PLAN

**Week 1**: Database entities + ContentManager service
**Week 2**: Widget migration + event system
**Week 3**: Testing + multi-content support
**Week 4**: AI agent integration + advanced features

**Total effort: 4 weeks to complete foundation for entire multi-content, multi-user, AI-agent architecture.**