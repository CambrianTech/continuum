# DataDaemon - Universal Storage Orchestrator

## **🎯 Mission**
Heavy abstraction for organizational data with pluggable storage strategies supporting both SQL and NoSQL paradigms through unified interface with automatic backend selection.

## **🏗️ Architecture Pattern**
Follows the **Sparse Override Pattern** with 85% shared logic:

```
daemons/data-daemon/
├── shared/
│   ├── DataDaemon.ts            # Universal interface (85% of logic)
│   ├── DataDaemonBase.ts        # Abstract base implementation
│   └── DataTypes.ts             # Shared types and contracts
├── browser/
│   └── DataDaemonBrowser.ts     # UI integration (5%)
├── server/
│   └── DataDaemonServer.ts      # Storage operations (10%)
└── README.md                    # This documentation
```

## 🏗️ **Architecture Philosophy**

### **Heavy Abstraction Principle**
- **Single Interface**: Same API for SQL tables, NoSQL collections, file systems, network storage
- **Strategy Pattern**: Storage backend determined by configuration, not code changes
- **Relational + Document**: Supports both SQL (tables/joins/indices) and NoSQL (collections/queries) concepts
- **Plugin Everything**: Storage adapters, query engines, indexing systems all pluggable

### **Core Design Concepts**

**1. Storage Strategy Abstraction**
```typescript
interface StorageStrategyConfig {
  strategy: 'sql' | 'nosql' | 'file' | 'memory' | 'network' | 'hybrid';
  backend: string; // 'postgres', 'sqlite', 'mongodb', 'redis', 'json', etc.
  namespace: string;
  features?: {
    enableTransactions?: boolean;
    enableIndexing?: boolean;
    enableReplication?: boolean;
    enableSharding?: boolean;
  };
}
```

**2. Universal Data Record**
```typescript
interface DataRecord<T = any> {
  id: UUID;
  collection: string;  // Table name (SQL) or Collection name (NoSQL)
  data: T;
  metadata: {
    createdAt: string;
    updatedAt: string;
    version: number;
    tags?: string[];
    schema?: string;
    ttl?: number;
  };
}
```

**3. Query Abstraction**
```typescript
interface StorageQuery {
  collection: string;
  filters?: Record<string, any>;     // WHERE clauses (SQL) or find() filters (NoSQL)
  sort?: { field: string; direction: 'asc' | 'desc' }[];
  limit?: number;
  offset?: number;
  tags?: string[];
  timeRange?: { start?: string; end?: string; };
}
```

## 🔧 **Storage Adapters**

### **File Storage Adapter** (Current Implementation)
- **Strategy**: `file` + `json`
- **Organization**: Collection = Directory, Record = JSON file
- **Features**: Atomic writes, filesystem indexing, directory-based queries
- **Use Case**: Development, lightweight persistence, session-based storage

### **SQL Storage Adapters** (Planned)
- **PostgreSQL Adapter**: Full ACID transactions, complex joins, advanced indexing
- **SQLite Adapter**: Embedded database, zero-config, file-based
- **Features**: Schema migration, foreign keys, triggers, views

### **NoSQL Storage Adapters** (Planned)
- **MongoDB Adapter**: Document storage, GridFS, aggregation pipelines
- **Redis Adapter**: Key-value store, pub/sub, caching, TTL support
- **Features**: Dynamic schemas, horizontal scaling, eventual consistency

### **Network Storage Adapters** (Planned)
- **Distributed Adapter**: Multi-node storage with consensus
- **P2P Adapter**: Peer-to-peer storage network
- **Features**: CAP theorem options, conflict resolution, replication

## 📊 **Query System Design**

### **Universal Query Interface**
Works across all storage backends by translating to appropriate backend queries:

```typescript
// Same query works for:
const query: StorageQuery = {
  collection: 'chat_messages',
  filters: { 
    'data.sender': 'claude',
    'metadata.createdAt': { '$gt': '2025-08-01' }
  },
  sort: [{ field: 'metadata.createdAt', direction: 'desc' }],
  limit: 100
};

// SQL Translation: SELECT * FROM chat_messages WHERE data->>'sender' = 'claude' AND created_at > '2025-08-01' ORDER BY created_at DESC LIMIT 100
// MongoDB Translation: db.chat_messages.find({ 'data.sender': 'claude', 'metadata.createdAt': { $gt: '2025-08-01' } }).sort({ 'metadata.createdAt': -1 }).limit(100)
// File Translation: Read all JSON files, filter objects, sort, paginate
```

### **Advanced Query Features** (Extensible)
- **Indexing**: Automatic index creation for frequently queried fields
- **Full-text Search**: Content search across data fields
- **Aggregation**: Statistical queries (count, sum, average, grouping)
- **Relationships**: Cross-collection references and joins
- **Temporal Queries**: Time-based filtering and windowing

## 🗃️ **Data Organization Patterns**

### **Collection Design**
- **Namespace Isolation**: Each client gets isolated namespace (`/user/{sessionId}`, `/system`, `/shared`)
- **Collection Types**: 
  - `messages` - Chat messages with participant metadata
  - `sessions` - Conversation sessions and context
  - `personas` - Persistent identity and memory data
  - `knowledge` - Learning and knowledge artifacts
  - `relationships` - Cross-participant relationship data

### **Schema Evolution**
- **Versioned Records**: Each record has version number for migration
- **Backward Compatibility**: Old versions readable by new code
- **Schema Registry**: Optional type definitions for validation
- **Migration System**: Automatic data structure updates

## 🚀 **Plugin Architecture**

### **Storage Adapter Plugin System**
```typescript
interface StorageAdapterPlugin {
  name: string;
  strategy: string;
  backends: string[];
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter;
  getCapabilities(): StorageCapabilities;
}

interface StorageCapabilities {
  supportsTransactions: boolean;
  supportsIndexing: boolean;
  supportsFullTextSearch: boolean;
  supportsReplication: boolean;
  maxRecordSize: number;
  concurrentConnections: number;
}
```

### **Query Engine Plugins**
- **SQL Query Engine**: Translates StorageQuery to SQL
- **NoSQL Query Engine**: Translates StorageQuery to MongoDB/Redis queries
- **File Query Engine**: In-memory filtering and sorting
- **GraphQL Engine**: Graph-based query interface (future)

## 💾 **Current Implementation**

### **Working Data Commands** (Built & Tested)
```bash
# Create records with complex JSON data
./jtag data/create --collection personas --data '{"name":"Claude","capabilities":["reasoning","coding"]}'

# Query with JSON output
./jtag data/list --collection personas --format json

# Read specific records
./jtag data/read --collection personas --id [uuid]
```

### **File Storage Backend** (Session-Based)
- **Path**: `.continuum/jtag/sessions/user/{sessionId}/data/{collection}/{id}.json`
- **Features**: Atomic writes, directory organization, JSON serialization
- **Performance**: Suitable for 1K-10K records per collection
- **⚠️ ARCHITECTURE NOTE**: For filesystem interaction, should use file commands with persistent handles instead of direct fs operations. This provides better abstraction and follows JTAG command patterns.

### **CLI Integration** (Fixed Parameter Parsing)
- **Formats**: Both `--key=value` and `--key value` work
- **JSON Support**: Complex nested JSON data parsing
- **Type Safety**: Strong typing from CLI through to storage

## 🎯 **Event Integration System**

### **Real-time Data Events** (Built-In)
All CRUD operations automatically emit structured events for real-time widget updates:

```typescript
// Server automatically emits on data operations
await Events.emit('data:users:created', userEntity);
await Events.emit('data:rooms:updated', roomEntity);
await Events.emit('data:messages:deleted', messageEntity);
```

### **Elegant Event Subscription** (New Feature)
Widgets subscribe to data changes using clean, flexible syntax:

```typescript
// Basic patterns - all operations for entity type
Events.subscribe('data:users');           // All user CRUD operations
Events.subscribe('data:rooms');           // All room CRUD operations

// Operation filtering using set syntax
Events.subscribe('data:users {created,updated}');     // Only creates/updates
Events.subscribe('data:users {c,u}');                 // Shorthand
Events.subscribe('data:users !{deleted}');            // Exclude deletes

// Entity-specific subscriptions
Events.subscribe('data:users:user123');               // Specific user changes
Events.subscribe('data:rooms:room456 {updated}');     // Specific room updates

// Parameter filtering with existing ORM syntax
Events.subscribe('data:users {updated}', {
  where: { roomId: 'current-room', status: 'active' }
});
```

### **Subscription Architecture**
- **String parser** - Tokenizes subscription patterns into filters
- **Pattern matching** - Matches emitted events against subscriptions
- **Parameter filtering** - Client-side filtering using ORM-style where clauses
- **Backward compatible** - Existing event emission unchanged

### **Real-world Usage Examples**
```typescript
// User list widget - show all user changes in current room
Events.subscribe('data:users', { where: { roomId: currentRoom.id } });

// Online status indicator - only status updates for this user
Events.subscribe('data:users:user123 {updated}', { where: { status: 'online' } });

// Activity feed - new registrations and updates, exclude deletes
Events.subscribe('data:users {created,updated}');

// Chat participant list - live updates for room members
Events.subscribe('data:users {created,updated,deleted}', {
  where: { roomId: roomId, active: true }
});
```

## 🎯 **Next Steps**

### **Immediate Architecture Tasks**
1. ✅ **DataDaemon Server**: Concrete implementation using storage adapters
2. ✅ **Memory Storage Adapter**: In-memory backend for fast operations
3. ✅ **Event Integration**: Real-time CRUD events with elegant subscription syntax
4. **File Command Integration**: Replace direct fs operations with file commands for persistent handles
5. **Query System**: Advanced filtering, sorting, pagination
6. **Index System**: Automatic indexing for performance
7. **Migration System**: Upgrade path from current direct-filesystem commands

### **Advanced Features** (Post-Foundation)
1. **Relational Queries**: JOIN-like operations across collections
2. **Backup/Restore**: Data export/import across storage backends
3. **Replication**: Multi-node data synchronization
4. **Caching Layer**: Intelligent caching with invalidation

## 🧠 **Memory Architecture Insights** (From Previous Research)

### **Persona Memory Patterns** (From Git Stash Analysis)
- **Context Relationships**: Cross-room conversation context tracking
- **Learning Moments**: Teaching interaction capture and retrieval
- **Capability Evolution**: Skill development tracking over time
- **Social Relationships**: Participant interaction patterns and history

### **Chat System Requirements** (From Stashed Chat Architecture)
- **Message History**: Full conversation replay with context
- **Participant Tracking**: Detailed participant behavior analysis
- **Academy Integration**: Learning session context and progress
- **Multi-Context**: Cross-room conversation relationship tracking

### **Query Requirements** (Derived from Chat Research)
- **Temporal Queries**: Messages in time ranges, conversation threads
- **Relational Queries**: Participant relationships, room connections
- **Content Analysis**: Keyword search, topic extraction, relevance scoring
- **Statistical Queries**: Activity patterns, learning progress, engagement metrics

## 📈 **Performance Considerations**

### **Storage Backend Selection Guide**
- **File Storage**: < 10K records, development, single-node
- **SQLite**: < 100K records, moderate complexity, embedded
- **PostgreSQL**: > 100K records, complex queries, ACID requirements
- **MongoDB**: > 1M records, flexible schema, horizontal scaling
- **Redis**: Caching, pub/sub, high-frequency reads
- **Hybrid**: Multiple backends for different data types

### **Query Optimization**
- **Indexing Strategy**: Automatic index creation for frequent queries
- **Caching Layer**: Query result caching with TTL
- **Pagination**: Efficient large result set handling
- **Connection Pooling**: Reuse database connections

## 🎯 **Success Criteria**

### **Plugin Architecture Validation**
- [ ] Same DataDaemon API works with File, Memory, SQL backends
- [ ] Storage adapter swappable via configuration only
- [ ] Query system translates correctly to each backend
- [ ] Performance scales appropriately with data size

### **Chat System Foundation**
- [ ] Persona memory persistence across sessions
- [ ] Conversation context retrieval and relationship tracking
- [ ] Learning moment capture and replay
- [ ] Real-time event integration for live chat

**The goal: Universal data persistence that scales from development file storage to production distributed systems while maintaining identical interface and query semantics.**