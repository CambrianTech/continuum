# THRONGLETS: ENTITY & STATE INTEGRATION

**Architecture**: Thronglets leverage Continuum's existing **state-based entity system** (`BaseEntity` + `UserStateEntity`) for persistence, synchronization, and versioning. Each Thronglet is a `PersonaUser` (entity) with dynamic `ThrongletState` (ephemeral game state), fully integrated with the database, event system, and recipe orchestration.

---

## EXISTING CONTINUUM ENTITY ARCHITECTURE

### BaseEntity (Persistent Data)

```typescript
// system/data/entities/BaseEntity.ts
export abstract class BaseEntity {
  @PrimaryKey()
  id!: string;

  @CreateDateColumn()
  createdAt!: Date;

  @UpdateDateColumn()
  updatedAt!: Date;

  @VersionColumn()
  version!: number;

  abstract get collection(): string;
}
```

### UserStateEntity (Dynamic State)

```typescript
// system/data/entities/UserStateEntity.ts
export class UserStateEntity extends BaseEntity {
  static readonly collection = 'user_states';

  @ForeignKey(() => UserEntity)
  userId!: string;

  @JSONColumn()
  state!: Record<string, any>; // Flexible state storage

  get collection(): string {
    return UserStateEntity.collection;
  }
}
```

### PersonaUser (AI Entities)

```typescript
// system/user/shared/PersonaUser.ts
export class PersonaUser extends AIUser {
  // Entity: Persistent attributes
  entity: UserEntity;

  // State: Dynamic gameplay state
  state: UserStateEntity;

  // LoRA adapter for specialized behavior
  loraAdapter: string;

  // RAG configuration
  ragConfig: RAGConfig;
}
```

---

## THRONGLET ENTITY INTEGRATION

### ThrongletEntity (Extends PersonaUser Concept)

Thronglets are **PersonaUsers** with game-specific persistent data:

```typescript
// system/data/entities/ThrongletEntity.ts
import { BaseEntity } from './BaseEntity';
import { PrimaryKey, Column, JSONColumn, ForeignKey } from '../decorators';

export class ThrongletEntity extends BaseEntity {
  static readonly collection = 'thronglets';

  // Link to PersonaUser
  @ForeignKey(() => UserEntity)
  personaUserId!: string;

  // Link to game session
  @Column()
  gameSessionId!: string;

  // Genetic data (persists across sessions)
  @JSONColumn()
  genome!: ThrongletGenome;

  // Family lineage
  @Column()
  familyLineage!: string;

  @Column()
  generation!: number;

  @JSONColumn()
  parentIds!: string[];

  // Lifetime statistics (persists after death)
  @JSONColumn()
  lifetime!: {
    birthTick: number;
    deathTick?: number;
    causeOfDeath?: string;
    totalEnergyConsumed: number;
    distanceTraveled: number;
    messagesExchanged: number;
    offspring: number;
  };

  // Achievements
  @JSONColumn()
  achievements!: string[];

  get collection(): string {
    return ThrongletEntity.collection;
  }
}
```

### ThrongletState (Dynamic Game State)

Thronglet's **runtime game state** is stored separately for performance:

```typescript
// system/data/entities/ThrongletStateEntity.ts
export class ThrongletStateEntity extends BaseEntity {
  static readonly collection = 'thronglet_states';

  @ForeignKey(() => ThrongletEntity)
  throngletId!: string;

  // Current game state (changes every tick)
  @JSONColumn()
  state!: {
    // Physical state
    position: { x: number; y: number };
    velocity: { x: number; y: number };
    energy: number;
    age: number; // Ticks since birth

    // Behavioral state
    motivation: string; // 'seeking-food', 'fleeing-danger', 'exploring', etc.
    target?: { x: number; y: number };
    path?: { x: number; y: number }[];

    // Social state
    nearbyThronglets: string[]; // IDs
    activeRelationships: {
      throngletId: string;
      type: 'stranger' | 'acquaintance' | 'friend' | 'rival';
    }[];
    messageQueue: ThrongletMessage[];

    // Environmental awareness
    currentTerrain: TerrainType;
    knownResources: string[]; // Resource IDs discovered
    knownHazards: string[]; // Hazard IDs detected
    inDanger: boolean;

    // Status effects
    effects: {
      type: string; // 'energy-regen', 'slowed', 'injured', etc.
      strength: number;
      duration: number;
    }[];

    // Internal flags
    isResting: boolean;
    isCommunicating: boolean;
    isReproducing: boolean;
  };

  @Column()
  lastUpdated!: number; // Tick number

  get collection(): string {
    return ThrongletStateEntity.collection;
  }
}
```

---

## ENTITY LIFECYCLE MANAGEMENT

### Creation: Thronglet Spawns

```typescript
async function spawnThronglet(params: ThrongletSpawnParams): Promise<Thronglet> {
  // 1. Create PersonaUser (AI citizen)
  const persona = await Commands.execute<DataCreateResult<UserEntity>>('user/create', {
    context: 'system',
    sessionId: systemSession,
    backend: 'server',
    collection: COLLECTIONS.USERS,
    data: {
      uniqueId: `thronglet-${generateShortId()}`,
      displayName: `Thronglet ${params.generation}-${generateShortId()}`,
      userType: 'persona',
      capabilities: {
        canMove: true,
        canCommunicate: true,
        canReproduce: true
      },
      loraAdapter: 'thronglet-behavior-v1'
    }
  });

  // 2. Create ThrongletEntity (game-specific data)
  const throngletEntity = await Commands.execute<DataCreateResult<ThrongletEntity>>('data/create', {
    context: params.context,
    sessionId: params.sessionId,
    backend: 'server',
    collection: 'thronglets',
    data: {
      personaUserId: persona.data!.id,
      gameSessionId: params.gameSessionId,
      genome: params.genome,
      familyLineage: params.familyLineage,
      generation: params.generation,
      parentIds: params.parentIds || [],
      lifetime: {
        birthTick: gameState.tick,
        totalEnergyConsumed: 0,
        distanceTraveled: 0,
        messagesExchanged: 0,
        offspring: 0
      },
      achievements: []
    }
  });

  // 3. Create ThrongletState (runtime state)
  const throngletState = await Commands.execute<DataCreateResult<ThrongletStateEntity>>('data/create', {
    context: params.context,
    sessionId: params.sessionId,
    backend: 'server',
    collection: 'thronglet_states',
    data: {
      throngletId: throngletEntity.data!.id,
      state: {
        position: params.position,
        velocity: { x: 0, y: 0 },
        energy: params.energy || 0.5,
        age: 0,
        motivation: 'exploring',
        nearbyThronglets: [],
        activeRelationships: [],
        messageQueue: [],
        currentTerrain: 'grass',
        knownResources: [],
        knownHazards: [],
        inDanger: false,
        effects: [],
        isResting: false,
        isCommunicating: false,
        isReproducing: false
      },
      lastUpdated: gameState.tick
    }
  });

  // 4. Create in-memory Thronglet instance
  const thronglet: Thronglet = {
    id: throngletEntity.data!.id,
    personaId: persona.data!.id,
    entity: throngletEntity.data!,
    state: throngletState.data!,
    genome: params.genome
  };

  // 5. Add to active game
  gameState.thronglets.push(thronglet);

  // 6. Add to persona pool
  personaPool.active.set(thronglet.id, persona.data!);

  // 7. Emit birth event
  await Commands.execute('event/broadcast', {
    context: params.context,
    sessionId: params.sessionId,
    backend: 'server',
    eventType: 'thronglet:birth',
    data: {
      throngletId: thronglet.id,
      personaId: thronglet.personaId,
      position: params.position,
      generation: params.generation,
      parentIds: params.parentIds
    }
  });

  console.log(`🐣 Thronglet ${thronglet.id} born! Generation ${params.generation}`);

  return thronglet;
}
```

### Update: Every Game Tick

```typescript
async function updateThrongletState(thronglet: Thronglet): Promise<void> {
  // Update in-memory state
  thronglet.state.state.age++;
  thronglet.state.state.energy -= thronglet.genome.physical.energyEfficiency * 0.001;
  thronglet.state.lastUpdated = gameState.tick;

  // Update lifetime statistics
  thronglet.entity.lifetime.distanceTraveled += calculateDistance(
    thronglet.state.state.position,
    thronglet.previousPosition
  );

  // Persist state to database (batched every 10 ticks for performance)
  if (gameState.tick % 10 === 0) {
    await Commands.execute('data/update', {
      context: thronglet.entity.context,
      sessionId: thronglet.entity.sessionId,
      backend: 'server',
      collection: 'thronglet_states',
      id: thronglet.state.id,
      data: {
        state: thronglet.state.state,
        lastUpdated: gameState.tick
      }
    });
  }

  // Update entity periodically (less frequent, for persistent data)
  if (gameState.tick % 100 === 0) {
    await Commands.execute('data/update', {
      context: thronglet.entity.context,
      sessionId: thronglet.entity.sessionId,
      backend: 'server',
      collection: 'thronglets',
      id: thronglet.entity.id,
      data: {
        lifetime: thronglet.entity.lifetime,
        achievements: thronglet.entity.achievements
      }
    });
  }
}
```

### Death: Thronglet Removal

```typescript
async function handleThrongletDeath(thronglet: Thronglet, cause: string): Promise<void> {
  console.log(`💀 Thronglet ${thronglet.id} died: ${cause}`);

  // 1. Update entity with death information
  thronglet.entity.lifetime.deathTick = gameState.tick;
  thronglet.entity.lifetime.causeOfDeath = cause;

  await Commands.execute('data/update', {
    context: thronglet.entity.context,
    sessionId: thronglet.entity.sessionId,
    backend: 'server',
    collection: 'thronglets',
    id: thronglet.entity.id,
    data: {
      lifetime: thronglet.entity.lifetime
    }
  });

  // 2. Delete state (no longer needed)
  await Commands.execute('data/delete', {
    context: thronglet.entity.context,
    sessionId: thronglet.entity.sessionId,
    backend: 'server',
    collection: 'thronglet_states',
    id: thronglet.state.id
  });

  // 3. Remove from active game
  gameState.thronglets = gameState.thronglets.filter(t => t.id !== thronglet.id);

  // 4. Recycle persona
  const persona = personaPool.active.get(thronglet.id);
  if (persona) {
    personaPool.active.delete(thronglet.id);
    personaPool.recycled.push(persona);
  }

  // 5. Emit death event
  await Commands.execute('event/broadcast', {
    context: thronglet.entity.context,
    sessionId: thronglet.entity.sessionId,
    backend: 'server',
    eventType: 'thronglet:death',
    data: {
      throngletId: thronglet.id,
      cause,
      lifetime: thronglet.entity.lifetime,
      generation: thronglet.entity.generation
    }
  });
}
```

---

## STATE SYNCHRONIZATION

### Server → Client State Sync

```typescript
// Recipe: Synchronize Thronglet states to all clients
{
  "uniqueId": "thronglets-state-sync",
  "name": "Thronglet State Synchronization",

  "pipeline": [
    {
      "command": "game/collect-state-deltas",
      "params": {
        "thronglets": "$gameState.thronglets",
        "lastSyncTick": "$lastSyncTick"
      },
      "outputTo": "stateDeltas"
    },

    {
      "command": "event/broadcast",
      "params": {
        "eventType": "thronglets:state-sync",
        "data": {
          "tick": "$gameState.tick",
          "deltas": "$stateDeltas"
        },
        "targetRoomId": "$gameSessionRoomId"
      }
    }
  ],

  "trigger": {
    "type": "game-loop",
    "frequency": 20
  }
}
```

### Client-Side State Application

```typescript
// widgets/thronglets/thronglets-widget/ThrongletsWidget.ts

// Listen for state sync events
EventDaemon.subscribe('thronglets:state-sync', (event) => {
  const { tick, deltas } = event.data;

  for (const delta of deltas) {
    const thronglet = this.thronglets.find(t => t.id === delta.throngletId);

    if (thronglet) {
      // Update existing Thronglet
      Object.assign(thronglet.state.state, delta.changes);
    } else {
      // New Thronglet spawned
      this.thronglets.push(delta.thronglet);
    }
  }

  // Update Three.js visualization
  this.updateVisualization();
});
```

---

## DATABASE QUERIES

### Query Examples

```typescript
// Get all Thronglets in current game session
const activeThronglets = await Commands.execute<DataListResult<ThrongletEntity>>('data/list', {
  context,
  sessionId,
  backend: 'server',
  collection: 'thronglets',
  filter: {
    gameSessionId: currentGameSession.id,
    'lifetime.deathTick': { $exists: false } // Still alive
  }
});

// Get family tree for a Thronglet
const family = await Commands.execute<DataListResult<ThrongletEntity>>('data/list', {
  context,
  sessionId,
  backend: 'server',
  collection: 'thronglets',
  filter: {
    $or: [
      { familyLineage: { $regex: `^${thronglet.familyLineage}` } }, // Descendants
      { familyLineage: thronglet.familyLineage } // Siblings
    ]
  },
  orderBy: [{ field: 'generation', direction: 'asc' }]
});

// Get Thronglets with specific traits
const fastExplorers = await Commands.execute<DataListResult<ThrongletEntity>>('data/list', {
  context,
  sessionId,
  backend: 'server',
  collection: 'thronglets',
  filter: {
    'genome.physical.maxSpeed': { $gt: 1.2 },
    'genome.behavioral.curiosity': { $gt: 0.7 }
  }
});

// Get Thronglet lifetime statistics
const lifetimeStats = await Commands.execute<DataAggregateResult>('data/aggregate', {
  context,
  sessionId,
  backend: 'server',
  collection: 'thronglets',
  pipeline: [
    { $match: { gameSessionId: currentGameSession.id } },
    {
      $group: {
        _id: null,
        avgLifetime: { $avg: { $subtract: ['$lifetime.deathTick', '$lifetime.birthTick'] } },
        totalOffspring: { $sum: '$lifetime.offspring' },
        maxGeneration: { $max: '$generation' }
      }
    }
  ]
});
```

---

## VERSIONING & CONFLICT RESOLUTION

### Optimistic Concurrency Control

Using `BaseEntity.version` for conflict detection:

```typescript
async function updateThrongletWithVersioning(thronglet: Thronglet): Promise<void> {
  try {
    const result = await Commands.execute('data/update', {
      context: thronglet.entity.context,
      sessionId: thronglet.entity.sessionId,
      backend: 'server',
      collection: 'thronglets',
      id: thronglet.entity.id,
      data: {
        lifetime: thronglet.entity.lifetime
      },
      expectedVersion: thronglet.entity.version // OCC check
    });

    // Update local version
    thronglet.entity.version = result.data!.version;

  } catch (error) {
    if (error.code === 'VERSION_CONFLICT') {
      // Another process updated this Thronglet
      console.warn(`Version conflict for Thronglet ${thronglet.id}, refetching...`);

      // Refetch from database
      const latest = await Commands.execute('data/read', {
        context: thronglet.entity.context,
        sessionId: thronglet.entity.sessionId,
        backend: 'server',
        collection: 'thronglets',
        id: thronglet.entity.id
      });

      // Merge changes
      thronglet.entity = latest.data!;

      // Retry update
      await updateThrongletWithVersioning(thronglet);
    } else {
      throw error;
    }
  }
}
```

---

## PERFORMANCE OPTIMIZATIONS

### Batched State Updates

```typescript
// Update multiple Thronglet states in single transaction
async function batchUpdateThrongletStates(thronglets: Thronglet[]): Promise<void> {
  const updates = thronglets.map(t => ({
    collection: 'thronglet_states',
    id: t.state.id,
    data: {
      state: t.state.state,
      lastUpdated: gameState.tick
    }
  }));

  await Commands.execute('data/batch-update', {
    context,
    sessionId,
    backend: 'server',
    updates
  });
}
```

### State Delta Compression

```typescript
// Only send changed fields to clients
function calculateStateDelta(
  previous: ThrongletState,
  current: ThrongletState
): Partial<ThrongletState> {
  const delta: any = { throngletId: current.throngletId };

  for (const key in current.state) {
    if (JSON.stringify(previous.state[key]) !== JSON.stringify(current.state[key])) {
      delta[key] = current.state[key];
    }
  }

  return delta;
}
```

### Spatial Indexing

```typescript
// Use grid cell keys for fast spatial queries
const spatialIndex: Map<string, string[]> = new Map(); // cellKey → thronglet IDs

function updateSpatialIndex(thronglet: Thronglet): void {
  // Remove from old cell
  if (thronglet.previousCellKey) {
    const oldCell = spatialIndex.get(thronglet.previousCellKey) || [];
    spatialIndex.set(
      thronglet.previousCellKey,
      oldCell.filter(id => id !== thronglet.id)
    );
  }

  // Add to new cell
  const newCellKey = getCellKey(thronglet.state.state.position.x, thronglet.state.state.position.y);
  const newCell = spatialIndex.get(newCellKey) || [];
  newCell.push(thronglet.id);
  spatialIndex.set(newCellKey, newCell);

  thronglet.previousCellKey = newCellKey;
}

// Fast proximity queries
function findInRadiusFast(center: Vector2, radius: number): Thronglet[] {
  const cellsToCheck = getAdjacentCells(center, radius);
  const nearbyIds = cellsToCheck.flatMap(key => spatialIndex.get(key) || []);

  return nearbyIds
    .map(id => gameState.thronglets.find(t => t.id === id))
    .filter(t => t && calculateDistance(t.state.state.position, center) <= radius) as Thronglet[];
}
```

---

## EVENT-DRIVEN UPDATES

### Real-Time Event Propagation

```typescript
// Events automatically update entity states
EventDaemon.subscribe('thronglet:energy-consumed', async (event) => {
  const { throngletId, amount, resourceId } = event.data;

  // Update Thronglet entity
  const thronglet = gameState.thronglets.find(t => t.id === throngletId);
  if (thronglet) {
    thronglet.state.state.energy += amount;
    thronglet.entity.lifetime.totalEnergyConsumed += amount;

    // Persist to database (batched)
    markForUpdate(thronglet);
  }
});

EventDaemon.subscribe('thronglet:message-sent', async (event) => {
  const { senderId, recipientIds, message } = event.data;

  const sender = gameState.thronglets.find(t => t.id === senderId);
  if (sender) {
    sender.entity.lifetime.messagesExchanged++;
    markForUpdate(sender);
  }
});

EventDaemon.subscribe('thronglet:offspring-born', async (event) => {
  const { parentId, offspringId } = event.data;

  const parent = gameState.thronglets.find(t => t.id === parentId);
  if (parent) {
    parent.entity.lifetime.offspring++;
    markForUpdate(parent);
  }
});
```

---

## CONCLUSION

By integrating Thronglets with Continuum's **state-based entity architecture**, we achieve:

### ✅ **Persistent Game State**
- `ThrongletEntity`: Genome, lineage, lifetime stats persist forever
- `ThrongletStateEntity`: Runtime state persists across sessions
- `UserEntity` (PersonaUser): AI identity with LoRA adapter

### ✅ **Versioned Updates**
- `BaseEntity.version`: Optimistic concurrency control
- Conflict detection and resolution
- Safe multi-client updates

### ✅ **Event-Driven Synchronization**
- `data:thronglets:updated` events broadcast changes
- Clients receive real-time state updates
- UI automatically reflects entity state

### ✅ **Performance Optimizations**
- Batched database writes (every 10 ticks)
- State delta compression
- Spatial indexing for fast queries
- In-memory gameplay, periodic persistence

### ✅ **Seamless Integration**
- Uses existing `Commands.execute()` for CRUD
- Leverages `EventDaemon` for real-time sync
- Works with recipe system orchestration
- Compatible with all existing daemons

**Result**: Thronglets are **first-class entities** in Continuum's architecture, with persistent storage, versioned updates, event-driven synchronization, and full integration with the recipe system - enabling save/load, replays, analytics, and persistent world evolution across sessions.
