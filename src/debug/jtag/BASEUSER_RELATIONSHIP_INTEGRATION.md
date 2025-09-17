# BaseUser Architecture Integration with Database Relationships

## Domain Object Hierarchy Integration

### BaseUser Inheritance Hierarchy
```
BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (also abstract)
    ├── AgentUser extends AIUser (external API services)
    └── PersonaUser extends AIUser (prompt + RAG → LoRA)
```

### Database Schema for Inheritance Hierarchy

#### Single Table Inheritance: `users`
```sql
-- Maps directly to BaseUser domain hierarchy
CREATE TABLE users (
  user_id UUID PRIMARY KEY,
  session_id UUID NOT NULL,

  -- BaseUser common fields
  display_name VARCHAR(255) NOT NULL,
  citizen_type ENUM('human', 'ai') NOT NULL, -- BaseUser.citizenType
  capabilities TEXT NOT NULL DEFAULT '[]', -- JSON array: BaseUser.capabilities
  created_at TIMESTAMP DEFAULT NOW(), -- BaseUser.createdAt
  updated_at TIMESTAMP DEFAULT NOW(), -- BaseUser.updatedAt
  last_active_at TIMESTAMP DEFAULT NOW(), -- BaseUser.lastActiveAt
  preferences JSONB DEFAULT '{}', -- BaseUser.preferences
  is_online BOOLEAN DEFAULT FALSE, -- BaseUser.isOnline

  -- HumanUser specific fields (NULL for AI users)
  email VARCHAR(255) UNIQUE,
  avatar_url TEXT,
  is_authenticated BOOLEAN DEFAULT FALSE,
  auth_info JSONB, -- lastLoginAt, loginCount, isEmailVerified

  -- AIUser base fields (NULL for human users)
  ai_type ENUM('agent', 'persona') NULL, -- AIUser.aiType
  model_config JSONB NOT NULL, -- AIModelConfig object
  specialization VARCHAR(255), -- AIUser.specialization
  context_memory JSONB DEFAULT '[]', -- AIUser.contextMemory array

  -- AgentUser fields (NULL for persona/human users)
  agent_portal_type VARCHAR(100), -- AgentPortalConfig.portalType
  agent_endpoint TEXT, -- AgentPortalConfig.endpoint
  agent_api_key TEXT, -- AgentPortalConfig.apiKey
  agent_config JSONB, -- AgentPortalConfig.config

  -- PersonaUser fields (NULL for agent/human users)
  persona_prompt TEXT, -- PersonaConfig.prompt
  personality_traits JSONB, -- PersonaConfig.personality
  rag_config JSONB, -- PersonaConfig.ragConfig

  -- Database constraints enforce domain hierarchy
  CHECK (
    (citizen_type = 'human' AND ai_type IS NULL AND model_config IS NULL) OR
    (citizen_type = 'ai' AND ai_type IN ('agent', 'persona') AND model_config IS NOT NULL)
  ),
  CHECK (
    (ai_type != 'agent') OR
    (ai_type = 'agent' AND agent_portal_type IS NOT NULL AND persona_prompt IS NULL)
  ),
  CHECK (
    (ai_type != 'persona') OR
    (ai_type = 'persona' AND persona_prompt IS NOT NULL AND agent_portal_type IS NULL)
  ),

  INDEX idx_citizen_type_ai_type (citizen_type, ai_type),
  INDEX idx_last_active (last_active_at),
  INDEX idx_online_users (is_online, last_active_at)
);
```

### Domain Object to Database Mapping

#### BaseUser → Database Mapping
```typescript
interface BaseUserDatabaseRecord {
  // BaseUser fields
  user_id: UUID;                    // BaseUser.userId
  session_id: UUID;                 // BaseUser.sessionId
  display_name: string;             // BaseUser.displayName
  citizen_type: 'human' | 'ai';     // BaseUser.citizenType
  capabilities: string;             // JSON.stringify(BaseUser.capabilities)
  created_at: string;               // BaseUser.createdAt
  updated_at: string;               // BaseUser.updatedAt
  last_active_at: string;           // BaseUser.lastActiveAt
  preferences: object;              // BaseUser.preferences
  is_online: boolean;               // BaseUser.isOnline

  // Type-specific fields (polymorphic)
  ai_type?: 'agent' | 'persona';
  model_config?: object;
  // ... other type-specific fields
}
```

#### Repository Pattern for Polymorphic Loading
```typescript
class UserRepository {
  /**
   * Load user and return correct domain object type
   */
  async findById(userId: UUID): Promise<BaseUser | null> {
    const record = await this.db.findOne('users', { user_id: userId });
    if (!record) return null;

    return this.mapToDomainObject(record);
  }

  /**
   * Polymorphic mapping to correct domain class
   */
  private mapToDomainObject(record: BaseUserDatabaseRecord): BaseUser {
    const baseData = {
      userId: record.user_id,
      sessionId: record.session_id,
      displayName: record.display_name,
      citizenType: record.citizen_type,
      capabilities: JSON.parse(record.capabilities),
      createdAt: record.created_at,
      updatedAt: record.updated_at,
      lastActiveAt: record.last_active_at,
      preferences: record.preferences,
      isOnline: record.is_online
    };

    if (record.citizen_type === 'human') {
      return HumanUser.fromData({
        ...baseData,
        email: record.email,
        isAuthenticated: record.is_authenticated,
        authInfo: record.auth_info
      });
    }

    // AI users
    const aiData = {
      ...baseData,
      aiType: record.ai_type!,
      modelConfig: record.model_config!,
      specialization: record.specialization,
      contextMemory: JSON.parse(record.context_memory || '[]')
    };

    if (record.ai_type === 'agent') {
      return AgentUser.fromData({
        ...aiData,
        agentPortalConfig: {
          portalType: record.agent_portal_type!,
          endpoint: record.agent_endpoint!,
          apiKey: record.agent_api_key,
          config: record.agent_config || {}
        }
      });
    }

    if (record.ai_type === 'persona') {
      return PersonaUser.fromData({
        ...aiData,
        personaConfig: {
          prompt: record.persona_prompt!,
          personality: record.personality_traits!,
          ragConfig: record.rag_config
        }
      });
    }

    throw new Error(`Unknown AI type: ${record.ai_type}`);
  }

  /**
   * Save polymorphic domain object to database
   */
  async save(user: BaseUser): Promise<void> {
    const baseRecord = {
      user_id: user.userId,
      session_id: user.sessionId,
      display_name: user.displayName,
      citizen_type: user.citizenType,
      capabilities: JSON.stringify(user.capabilities),
      created_at: user.createdAt,
      updated_at: user.updatedAt,
      last_active_at: user.lastActiveAt,
      preferences: user.preferences,
      is_online: user.isOnline
    };

    if (user.isHuman()) {
      const humanUser = user as HumanUser;
      await this.db.upsert('users', {
        ...baseRecord,
        email: humanUser.email,
        is_authenticated: humanUser.isAuthenticated,
        auth_info: humanUser.authInfo,
        // NULL out AI fields
        ai_type: null,
        model_config: null,
        agent_portal_type: null,
        persona_prompt: null
      });
    }

    if (user.isAI()) {
      const aiUser = user as AIUser;
      const aiRecord = {
        ...baseRecord,
        ai_type: aiUser.aiType,
        model_config: aiUser.modelConfig,
        specialization: aiUser.specialization,
        context_memory: JSON.stringify(aiUser.contextMemory),
        // NULL out human fields
        email: null,
        is_authenticated: null,
        auth_info: null
      };

      if (aiUser.isAgent()) {
        const agentUser = aiUser as AgentUser;
        await this.db.upsert('users', {
          ...aiRecord,
          agent_portal_type: agentUser.agentPortalConfig.portalType,
          agent_endpoint: agentUser.agentPortalConfig.endpoint,
          agent_api_key: agentUser.agentPortalConfig.apiKey,
          agent_config: agentUser.agentPortalConfig.config,
          // NULL out persona fields
          persona_prompt: null,
          personality_traits: null,
          rag_config: null
        });
      }

      if (aiUser.isPersona()) {
        const personaUser = aiUser as PersonaUser;
        await this.db.upsert('users', {
          ...aiRecord,
          persona_prompt: personaUser.personaConfig.prompt,
          personality_traits: personaUser.personaConfig.personality,
          rag_config: personaUser.personaConfig.ragConfig,
          // NULL out agent fields
          agent_portal_type: null,
          agent_endpoint: null,
          agent_api_key: null,
          agent_config: null
        });
      }
    }
  }
}
```

### Type-Safe Repository Methods

#### Human-Specific Operations
```typescript
class HumanUserRepository extends UserRepository {
  async findByEmail(email: string): Promise<HumanUser | null> {
    const record = await this.db.findOne('users', {
      email,
      citizen_type: 'human'
    });

    return record ? this.mapToDomainObject(record) as HumanUser : null;
  }

  async findAuthenticated(): Promise<HumanUser[]> {
    const records = await this.db.findMany('users', {
      citizen_type: 'human',
      is_authenticated: true
    });

    return records.map(r => this.mapToDomainObject(r) as HumanUser);
  }
}
```

#### AI-Specific Operations
```typescript
class AIUserRepository extends UserRepository {
  async findByProvider(provider: string): Promise<AIUser[]> {
    const records = await this.db.query('users', {
      citizen_type: 'ai',
      'model_config->provider': provider // JSONB query
    });

    return records.map(r => this.mapToDomainObject(r) as AIUser);
  }

  async findAgents(): Promise<AgentUser[]> {
    const records = await this.db.findMany('users', {
      citizen_type: 'ai',
      ai_type: 'agent'
    });

    return records.map(r => this.mapToDomainObject(r) as AgentUser);
  }

  async findPersonas(): Promise<PersonaUser[]> {
    const records = await this.db.findMany('users', {
      citizen_type: 'ai',
      ai_type: 'persona'
    });

    return records.map(r => this.mapToDomainObject(r) as PersonaUser);
  }
}
```

### Relationship Integration with BaseUser Hierarchy

#### User Sessions (Polymorphic)
```sql
CREATE TABLE user_sessions (
  session_id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  session_token VARCHAR(255) UNIQUE NOT NULL,
  device_info JSONB, -- Different for humans vs AI
  is_active BOOLEAN DEFAULT TRUE,
  started_at TIMESTAMP DEFAULT NOW(),
  last_activity_at TIMESTAMP DEFAULT NOW(),
  ended_at TIMESTAMP NULL,

  INDEX idx_user_sessions (user_id, is_active),
  INDEX idx_session_token (session_token)
);
```

#### User Permissions (Role-Based)
```sql
CREATE TABLE user_permissions (
  permission_id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE,
  permission VARCHAR(100) NOT NULL,
  resource VARCHAR(255) DEFAULT '*',
  granted_by UUID REFERENCES users(user_id) ON DELETE SET NULL,
  granted_at TIMESTAMP DEFAULT NOW(),
  expires_at TIMESTAMP NULL,

  -- Same permissions work for any user type
  UNIQUE KEY unique_permission (user_id, permission, resource),
  INDEX idx_user_permissions (user_id)
);
```

#### Room Memberships (Polymorphic Participants)
```sql
CREATE TABLE room_memberships (
  membership_id UUID PRIMARY KEY,
  room_id UUID REFERENCES chat_rooms(room_id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(user_id) ON DELETE CASCADE, -- Any BaseUser type
  role ENUM('member', 'moderator', 'admin', 'owner') DEFAULT 'member',
  joined_at TIMESTAMP DEFAULT NOW(),
  left_at TIMESTAMP NULL,
  is_active BOOLEAN DEFAULT TRUE,

  -- AI users have different participation patterns
  auto_respond BOOLEAN DEFAULT FALSE, -- For AI users
  response_triggers JSONB, -- AI response configuration

  UNIQUE KEY unique_membership (room_id, user_id),
  INDEX idx_room_members (room_id, is_active),
  INDEX idx_user_rooms (user_id, is_active)
);
```

### Migration Strategy for BaseUser Hierarchy

#### Phase 1: Create Users Table with Inheritance Support
```typescript
class BaseUserMigration {
  async up(): Promise<void> {
    // Create table with all inheritance fields
    await this.db.exec(CREATE_USERS_TABLE_SQL);

    // Migrate existing domain objects
    await this.migrateHumanUsers();
    await this.migrateAIUsers();
  }

  private async migrateHumanUsers(): Promise<void> {
    const humanUsers = await this.loadExistingHumans();

    for (const user of humanUsers) {
      await this.db.insert('users', {
        user_id: user.userId,
        citizen_type: 'human',
        display_name: user.displayName,
        // ... other fields
        // NULL out AI fields
        ai_type: null,
        model_config: null
      });
    }
  }

  private async migrateAIUsers(): Promise<void> {
    const aiUsers = await this.loadExistingAI();

    for (const user of aiUsers) {
      const record = {
        user_id: user.userId,
        citizen_type: 'ai',
        ai_type: user.aiType,
        // ... map AI-specific fields
      };

      await this.db.insert('users', record);
    }
  }
}
```

## Benefits of Integrated Architecture

1. **Type Safety**: Database constraints enforce domain object hierarchy
2. **Polymorphic Queries**: Single table supports all user types
3. **Efficient Storage**: No JOIN required for basic user operations
4. **Extensibility**: Easy to add new AI types or human variations
5. **Relationship Consistency**: All relationships work with any user type

## Domain Object Integration Verified

✅ **BaseUser** → `users` table with `citizen_type` discrimination
✅ **HumanUser** → Records where `citizen_type = 'human'`
✅ **AIUser** → Records where `citizen_type = 'ai'` (abstract, never stored directly)
✅ **AgentUser** → Records where `ai_type = 'agent'`
✅ **PersonaUser** → Records where `ai_type = 'persona'`

The database schema perfectly mirrors your domain object inheritance hierarchy while supporting efficient queries and maintaining referential integrity.