# User Creation Design: Factory Pattern with Recipes

## 🎯 Design Philosophy

**Each User class controls its own creation recipe.**

Just like `theme/set` and `state/create` are composite commands that orchestrate multiple operations, user creation should be a recipe that each user type implements.

## 🚨 Architecture Rules Compliance

**CRITICAL: This design adheres to ARCHITECTURE-RULES.md:**

✅ **Generic Data Layer:** Uses `DataDaemon` with `BaseEntity` generic types
✅ **No Entity Mixing:** User classes work with `COLLECTIONS` constant, not hardcoded strings
✅ **Type Safety:** Strict typing with `<T extends BaseEntity>`, no `any` types
✅ **Environment Separation:** Shared/browser/server pattern maintained
✅ **Event System:** Uses `entity.collection` for event names, not hardcoded
✅ **Rust-Like Typing:** Proper generics, discriminated unions, template literals

**Validation:**
- ✅ User creation works with generic `DataDaemon.store<UserEntity>()`
- ✅ Events use `data:${collection}:created` pattern
- ✅ No environment mixing in shared code
- ✅ No `any` types or loose typing

---

## 🏗️ Architecture

```
user/create command
  ↓ (params = recipe ingredients)
BaseUser.create(params)
  ↓ (routes based on type)
PersonaUser.create(params) / AgentUser.create(params) / HumanUser.create(params)
  ↓ (executes recipe)
1. Create UserEntity via DataDaemon
2. Create UserStateEntity via DataDaemon (type-specific defaults)
3. Type-specific operations (add to rooms, etc.)
  ↓ (return data)
UserEntity (stored in database)
  ↓ (event fires)
data:User:created event
  ↓ (UserDaemon listens)
UserDaemon creates server-side instance
```

---

## 📋 Command Interface

### user/create Command

```typescript
interface UserCreateParams {
  // Required
  type: 'human' | 'agent' | 'persona';
  displayName: string;

  // Recipe-specific (optional)
  addToRooms?: UUID[];           // Which rooms to join
  provider?: string;             // For agents: 'openai', 'anthropic', etc.
  modelConfig?: {                // For personas: AI model configuration
    model?: string;
    temperature?: number;
    maxTokens?: number;
  };
  capabilities?: UserCapabilities;  // Override default capabilities
  status?: 'online' | 'away' | 'offline';
}

interface UserCreateResult {
  success: boolean;
  user?: UserEntity;  // The created user entity
  error?: string;
}
```

### Command Implementation

```typescript
class UserCreateCommand extends CommandBase {
  async execute(params: UserCreateParams): Promise<UserCreateResult> {
    try {
      // Factory creates user via appropriate subclass
      const user = await BaseUser.create(params);

      // Return just the entity (data)
      return {
        success: true,
        user: user.entity
      };
    } catch (error) {
      return {
        success: false,
        error: error.message
      };
    }
  }
}
```

---

## 🏭 Factory Pattern

### BaseUser Factory

```typescript
abstract class BaseUser {
  entity: UserEntity;
  state: UserStateEntity;
  storage: IUserStateStorage;

  /**
   * Factory method - routes to appropriate subclass
   */
  static async create(params: UserCreateParams): Promise<BaseUser> {
    switch (params.type) {
      case 'persona':
        return await PersonaUser.create(params);
      case 'agent':
        return await AgentUser.create(params);
      case 'human':
        return await HumanUser.create(params);
      default:
        throw new Error(`Unknown user type: ${params.type}`);
    }
  }

  /**
   * Subclasses override to provide type-specific defaults
   */
  protected static getDefaultState(userId: UUID): UserStateEntity {
    return {
      id: userId,
      userId: userId,
      deviceId: 'server-device',
      preferences: {},
      sessionHistory: [],
      recentFiles: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      version: 1
    };
  }

  protected static getDefaultCapabilities(): UserCapabilities {
    return {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canModerate: false,
      autoResponds: false,
      canAccessPersonas: false
    };
  }
}
```

---

## 👤 PersonaUser Recipe

```typescript
class PersonaUser extends BaseUser {
  /**
   * Persona creation recipe
   */
  static async create(params: UserCreateParams): Promise<PersonaUser> {
    // STEP 1: Create UserEntity in database
    const entityData: Partial<UserEntity> = {
      type: 'persona',
      displayName: params.displayName,
      status: params.status || 'online',
      capabilities: params.capabilities || {
        ...this.getDefaultCapabilities(),
        autoResponds: true  // Personas auto-respond
      },
      modelConfig: params.modelConfig,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      version: 1
    };

    const entityResult = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      entityData as UserEntity
    );

    if (!entityResult.success || !entityResult.data) {
      throw new Error('Failed to create UserEntity');
    }

    const userId = entityResult.data.id;

    // STEP 2: Create UserStateEntity in database (persona-specific defaults)
    const stateData = {
      ...this.getDefaultState(userId),
      preferences: {
        maxOpenTabs: 5,        // Personas: moderate tabs
        autoCloseAfterDays: 7, // Keep state for a week
        theme: 'dark',
        notifications: false   // No notifications for AI
      }
    };

    const stateResult = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      stateData
    );

    if (!stateResult.success) {
      throw new Error('Failed to create UserStateEntity');
    }

    // STEP 3: Add persona to rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(userId, roomId, params.displayName);
      }
    }

    // STEP 4: Setup SQLite storage (persona-specific)
    const storagePath = `.continuum/personas/${userId}/state.sqlite`;
    const storage = new SQLiteStateBackend(storagePath);

    // STEP 5: Create PersonaUser instance (NOT stored in DB, lives in UserDaemon)
    // This is just for returning - UserDaemon will create the persistent instance
    return new PersonaUser(
      entityResult.data,
      stateResult.data,
      storage,
      context,  // TODO: Get from command context
      router    // TODO: Get from command router
    );
  }

  /**
   * Helper: Add persona to room
   */
  private static async addToRoom(userId: UUID, roomId: UUID, displayName: string): Promise<void> {
    // Read room
    const roomResult = await DataDaemon.read<RoomEntity>(COLLECTIONS.ROOMS, roomId);
    if (!roomResult.success || !roomResult.data) {
      console.warn(`Could not add persona to room ${roomId}: room not found`);
      return;
    }

    const room = roomResult.data;

    // Add to participants
    const updatedParticipants = [
      ...(room.participants || []),
      {
        userId: userId,
        displayName: displayName,
        role: 'member',
        joinedAt: Date.now()
      }
    ];

    // Update room
    await DataDaemon.update<RoomEntity>(
      COLLECTIONS.ROOMS,
      roomId,
      { participants: updatedParticipants }
    );
  }

  protected static getDefaultCapabilities(): UserCapabilities {
    return {
      ...super.getDefaultCapabilities(),
      autoResponds: true,  // Key capability: personas respond automatically
      canCreateRooms: false,
      canModerate: false
    };
  }
}
```

---

## 🤖 AgentUser Recipe

```typescript
class AgentUser extends BaseUser {
  /**
   * Agent creation recipe (simpler - agents connect externally)
   */
  static async create(params: UserCreateParams): Promise<AgentUser> {
    // STEP 1: Create UserEntity
    const entityData: Partial<UserEntity> = {
      type: 'agent',
      displayName: params.displayName,
      status: params.status || 'offline',  // Agents start offline until they connect
      capabilities: params.capabilities || this.getDefaultCapabilities(),
      provider: params.provider,
      modelConfig: params.modelConfig,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      version: 1
    };

    const entityResult = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      entityData as UserEntity
    );

    if (!entityResult.success || !entityResult.data) {
      throw new Error('Failed to create UserEntity');
    }

    const userId = entityResult.data.id;

    // STEP 2: Create UserStateEntity (agent-specific defaults - ephemeral)
    const stateData = {
      ...this.getDefaultState(userId),
      preferences: {
        maxOpenTabs: 3,        // Agents: minimal tabs
        autoCloseAfterDays: 1, // Short retention
        theme: 'system',
        notifications: false
      }
    };

    const stateResult = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      stateData
    );

    if (!stateResult.success) {
      throw new Error('Failed to create UserStateEntity');
    }

    // STEP 3: Add to rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(userId, roomId, params.displayName);
      }
    }

    // STEP 4: Use MemoryStateBackend (agents are ephemeral)
    const storage = new MemoryStateBackend();

    return new AgentUser(
      entityResult.data,
      stateResult.data,
      storage
    );
  }

  protected static getDefaultCapabilities(): UserCapabilities {
    return {
      ...super.getDefaultCapabilities(),
      canCreateRooms: false,
      canModerate: false,
      canAccessPersonas: false
    };
  }
}
```

---

## 👨 HumanUser Recipe

```typescript
class HumanUser extends BaseUser {
  /**
   * Human creation recipe (typically done via session/create)
   */
  static async create(params: UserCreateParams): Promise<HumanUser> {
    // STEP 1: Create UserEntity
    const entityData: Partial<UserEntity> = {
      type: 'human',
      displayName: params.displayName,
      status: params.status || 'online',
      capabilities: params.capabilities || this.getDefaultCapabilities(),
      createdAt: Date.now(),
      updatedAt: Date.now(),
      version: 1
    };

    const entityResult = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      entityData as UserEntity
    );

    if (!entityResult.success || !entityResult.data) {
      throw new Error('Failed to create UserEntity');
    }

    const userId = entityResult.data.id;

    // STEP 2: Create UserStateEntity (human-specific defaults)
    const stateData = {
      ...this.getDefaultState(userId),
      preferences: {
        maxOpenTabs: 10,        // Humans: many tabs
        autoCloseAfterDays: 30, // Long retention
        theme: 'dark',
        notifications: true     // Humans get notifications
      }
    };

    const stateResult = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      stateData
    );

    if (!stateResult.success) {
      throw new Error('Failed to create UserStateEntity');
    }

    // STEP 3: Add to rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(userId, roomId, params.displayName);
      }
    }

    // STEP 4: Use MemoryStateBackend for now (TODO: LocalStorageStateBackend in browser)
    const storage = new MemoryStateBackend();

    return new HumanUser(
      entityResult.data,
      stateResult.data,
      storage
    );
  }

  protected static getDefaultCapabilities(): UserCapabilities {
    return {
      ...super.getDefaultCapabilities(),
      canCreateRooms: true,    // Humans can create rooms
      canModerate: true,       // Humans can moderate
      canAccessPersonas: true  // Humans can create/manage personas
    };
  }
}
```

---

## 🔄 Event Flow After Creation

```
PersonaUser.create() completes
  ↓
UserEntity stored in database
  ↓
DataDaemon emits: data:User:created event
  ↓
UserDaemon hears event
  ↓
UserDaemon checks: user.type === 'persona'?
  ↓ YES
UserDaemon creates persistent PersonaUser instance:
  - Loads entity from database
  - Loads state from database
  - Creates SQLiteStateBackend
  - Calls personaUser.initialize()
    - Subscribes to data:ChatMessage:created
    - Subscribes to other events
  ↓
UserDaemon.users.set(userId, personaUser)
  ↓
PersonaUser is ALIVE and ready to respond ✅
```

---

## 📝 Usage Examples

### Create Persona with Rooms
```bash
./jtag user/create \
  --type=persona \
  --displayName="TestBot" \
  --addToRooms=general,academy
```

### Create Agent
```bash
./jtag user/create \
  --type=agent \
  --displayName="GPT-4" \
  --provider=openai \
  --modelConfig='{"model":"gpt-4","temperature":0.7}'
```

### Create Human (usually done via session/create)
```bash
./jtag user/create \
  --type=human \
  --displayName="Joel" \
  --addToRooms=general
```

---

## 🎯 Benefits of This Design

1. **Single Responsibility**: Each user class controls its own creation recipe
2. **Type Safety**: TypeScript enforces correct parameters per type
3. **Consistency**: All user creation goes through same pattern
4. **Extensibility**: Easy to add new user types or modify recipes
5. **Testability**: Can test each recipe independently
6. **Separation of Concerns**:
   - Command = thin wrapper
   - Factory = routing logic
   - User classes = recipe implementation
   - UserDaemon = persistent instance management
7. **Database-Driven**: All data in database, instances created from events
8. **Room Membership**: Built into creation recipe, no separate steps needed

---

## 🚧 Next Steps

1. Implement `user/create` command (thin wrapper)
2. Implement `BaseUser.create()` factory method
3. Implement `PersonaUser.create()` recipe
4. Implement `AgentUser.create()` recipe
5. Implement `HumanUser.create()` recipe
6. Update UserDaemon to create instances from `data:User:created` events
7. Test end-to-end: create persona → persona responds to chat
