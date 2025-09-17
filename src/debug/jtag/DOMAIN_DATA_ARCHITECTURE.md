# Domain Data Architecture Design Document

## Current State Analysis

### Existing Architecture Components

#### 1. Domain Layer (`/domain/user/`)
- ✅ `BaseUser` abstract class - proper inheritance hierarchy
- ✅ `HumanUser extends BaseUser` - human citizens
- ✅ `AIUser extends BaseUser` (abstract) - AI base class
- ✅ `AgentUser extends AIUser` - external AI services
- ✅ `PersonaUser extends AIUser` - internal AI with prompts/RAG
- ✅ Immutable data patterns with factory methods
- ✅ Type guards and proper TypeScript typing

#### 2. Data Daemon System (`/daemons/data-daemon/`)
- ✅ `DataDaemonBase extends DaemonBase` - follows daemon pattern
- ✅ Message-based operations with `DataOperationPayload`
- ✅ `DataDaemon` class - universal storage orchestrator
- ✅ `StorageAdapterFactory` - pluggable backends (SQL, NoSQL, File, Memory)
- ✅ `DataStorageAdapter` interface - adapter pattern implementation
- ✅ Storage strategies: sql, nosql, file, memory, network, hybrid

#### 3. JTAG System Integration
- ✅ Auto-discovery manifests for daemon registration
- ✅ Message routing via `JTAGRouter`
- ✅ Cross-environment communication (browser ↔ server)
- ✅ Universal command execution pattern

## Problems Identified

### 1. **Missing Domain ↔ Daemon Integration**
- Domain objects (`BaseUser`, `HumanUser`, etc.) exist in isolation
- `DataDaemon` exists but no domain-specific operations
- No repository pattern connecting domain objects to storage
- No relationship mapping (foreign keys, joins, etc.)

### 2. **No ORM Layer**
- Raw `DataDaemon` operations require manual mapping
- No type-safe queries for domain objects
- No relationship loading (sessions, permissions, room participation)
- No domain validation integration with storage

### 3. **Storage Adapter Gaps**
- Existing adapters but unclear which are implemented
- No clear migration path between storage types
- No relationship handling in different backends

## Proposed Solution: Domain Data Architecture

### Architecture Principles
1. **Use existing daemon pattern** - integrate with `DataDaemonBase`
2. **Extend domain objects** - add relationships without breaking existing code
3. **Repository pattern** - domain-specific operations over `DataDaemon`
4. **Adapter-agnostic** - work with any `DataStorageAdapter`
5. **Message-based** - use existing JTAG message routing

### Component Design

#### 1. Domain Extensions
```typescript
// Extend existing domain objects with relationships
interface BaseUserWithRelationships extends BaseUserData {
  readonly sessions?: readonly UserSession[];
  readonly permissions?: readonly UserPermission[];
  readonly roomParticipations?: readonly RoomParticipation[];
}

// Keep existing classes, add relationship methods
export class BaseUser {
  // Existing methods unchanged

  // New relationship methods
  async loadSessions(repo: UserRepository): Promise<UserSession[]>
  async loadPermissions(repo: UserRepository): Promise<UserPermission[]>
  async hasPermission(permission: string, resource: string): Promise<boolean>
}
```

#### 2. Repository Layer Over DataDaemon
```typescript
// Repository uses DataDaemon via daemon messages
export class UserRepository {
  constructor(private dataDaemonClient: DataDaemonClient) {}

  // Type-safe domain operations
  async createUser<T extends BaseUser>(userData: BaseUserData): Promise<T>
  async findById<T extends BaseUser>(userId: UUID): Promise<T | null>
  async findByType(citizenType: 'human' | 'ai'): Promise<BaseUser[]>

  // Relationship operations
  async findUserSessions(userId: UUID): Promise<UserSession[]>
  async grantPermission(userId: UUID, permission: string): Promise<void>
}
```

#### 3. DataDaemonClient (Message-Based)
```typescript
// Client wraps DataDaemon operations in JTAG messages
export class DataDaemonClient {
  constructor(private router: JTAGRouter) {}

  async create<T>(collection: string, data: T): Promise<StorageResult<T>>
  async read<T>(collection: string, id: UUID): Promise<StorageResult<T>>
  async query<T>(query: StorageQuery): Promise<StorageResult<T[]>>

  // Sends messages to data daemon, receives responses
}
```

#### 4. Domain Commands Integration
```typescript
// Commands use repositories, not raw DataDaemon
export class CreateUserCommand extends CommandBase {
  async execute(params: CreateUserParams): Promise<CreateUserResult> {
    const userRepo = await this.getUserRepository();

    if (params.userType === 'human') {
      return await userRepo.createHuman(params.displayName, params.sessionId);
    } else {
      return await userRepo.createAgent(params.displayName, params.config);
    }
  }
}
```

### Storage Backend Support

#### Current DataDaemon Adapters
- ✅ `FileStorageAdapter` - JSON files
- ✅ `MemoryStorageAdapter` - in-memory
- ❓ `SQLiteAdapter` - needs verification
- ❓ `PostgreSQLAdapter` - needs verification
- ❓ `MongoDBAdapter` - needs verification

#### Relationship Handling by Adapter
- **SQL Adapters**: Foreign keys, JOINs, transactions
- **NoSQL Adapters**: Embedded documents, references, lookups
- **File Adapters**: Manual relationship resolution via separate files
- **Memory Adapters**: JavaScript object references

### Implementation Plan

#### Phase 1: Core Integration
1. **Create `UserRepository`** - domain operations over `DataDaemonClient`
2. **Create `DataDaemonClient`** - message-based wrapper around daemon
3. **Extend domain objects** - add relationship methods (non-breaking)
4. **Create relationship entities** - `UserSession`, `UserPermission`, etc.

#### Phase 2: Command Integration
1. **Update user commands** - use repositories instead of raw daemon calls
2. **Add relationship commands** - grant permissions, manage sessions
3. **Create migration commands** - move between storage backends

#### Phase 3: Advanced Features
1. **Query builder** - type-safe, adapter-aware queries
2. **Relationship loading** - efficient joins/lookups per adapter
3. **Validation integration** - domain validation with storage
4. **Performance optimization** - caching, indexing hints

### File Structure
```
domain/user/
├── BaseUser.ts                 # ✅ Existing - extend with relationship methods
├── HumanUser.ts               # ✅ Existing - extend with human-specific ops
├── AgentUser.ts               # ✅ Existing - extend with agent-specific ops
├── PersonaUser.ts             # ✅ Existing - extend with persona-specific ops
├── UserRelationships.ts       # 🆕 Relationship entity definitions
├── UserRepository.ts          # 🆕 Repository pattern over DataDaemon
├── DataDaemonClient.ts        # 🆕 Message-based DataDaemon wrapper
└── UserCommands.ts            # 🆕 Domain commands using repositories

daemons/data-daemon/
├── shared/DataDaemonBase.ts   # ✅ Existing - message handling
├── shared/DataDaemon.ts       # ✅ Existing - storage orchestrator
├── server/DataDaemonServer.ts # 🆕 Server implementation needed
└── browser/DataDaemonBrowser.ts # 🆕 Browser implementation needed
```

### Success Criteria
1. **Non-breaking** - existing domain objects work unchanged
2. **Daemon integration** - follows existing JTAG daemon patterns
3. **Adapter-agnostic** - same API works with any storage backend
4. **Type-safe** - full TypeScript support for domain operations
5. **Relationship support** - foreign keys, joins, efficient loading
6. **Command integration** - user commands use domain repositories

### Next Steps
1. ✅ **Research existing implementations** - understand current daemon setup
2. 🚧 **Create `DataDaemonClient`** - message-based wrapper
3. 🚧 **Implement `UserRepository`** - domain operations
4. 🚧 **Add relationship entities** - sessions, permissions, rooms
5. 🚧 **Test with existing adapters** - validate storage backend support

---

**Key Insight**: Don't rebuild what exists. Integrate domain objects with existing DataDaemon via proper daemon message patterns and repository abstraction.