# System Configuration Architecture

**Universal hierarchical configuration system that mirrors command structure**

## Philosophy

"Settings as data means settings can corrupt the system, just like any database corruption. We need the same recovery tools as a real OS."

Key principles:
- **Hierarchical like commands** - `system/scheduling/timings/*` mirrors `./jtag data/list`
- **Factory reset = re-seeding** - `FACTORY_DEFAULTS` are literal reset values
- **Safe mode & recovery** - OS-style boot modes for configuration corruption
- **Async migration** - Settings migrate incrementally without blocking startup
- **Full audit trail** - Who changed what, when, why (last 10 changes)

## File Structure

```
system/data/entities/SystemConfigEntity.ts  - Universal config entity
system/scheduling/shared/SystemSchedulingState.ts - Adapter singleton for scheduling
```

## SystemConfigEntity

### Hierarchical Settings Paths

Settings organized like command paths:

```typescript
// Command structure (existing)
./jtag data/list
./jtag collaboration/chat/send
./jtag system/config/get

// Settings mirror this (new)
system/data/storage/max-size
system/chat/history/max-messages
system/scheduling/timings/adapter-health-check
system/ai/providers/ollama/enabled
```

### Core API

```typescript
class SystemConfigEntity extends BaseEntity {
  static readonly collection = 'system_config';

  // Get setting by path
  get(path: string): SettingValue | undefined;

  // Set setting (records change in history)
  set(path: string, value: SettingValue, changedBy: UUID, reason?: string): void;

  // Get all settings under group
  getGroup(groupPath: string): { [path: string]: SettingNode };

  // Factory reset single setting
  reset(path: string, changedBy: UUID): void;

  // Factory reset entire group (re-seed from FACTORY_DEFAULTS)
  resetGroup(groupPath: string, changedBy: UUID): void;

  // Register new setting (used during migration)
  registerSetting(path: string, metadata: SettingMetadata, initialValue?: SettingValue): void;
}
```

### Setting Node Structure

```typescript
interface SettingNode {
  path: string;                    // Full path
  value: SettingValue;             // Current value
  metadata: SettingMetadata;       // Type, constraints, description
  history: SettingChange[];        // Last 10 changes
}

interface SettingMetadata {
  type: 'string' | 'number' | 'boolean' | 'object';
  description: string;
  defaultValue: SettingValue;

  // Validation
  min?: number;
  max?: number;
  enum?: string[];
  required?: boolean;

  // UI hints
  displayName?: string;
  unit?: string;
  category?: string;

  // Runtime
  requiresRestart?: boolean;
  affectsComponents?: string[];
}
```

### Factory Defaults (Seed Data)

```typescript
export const FACTORY_DEFAULTS = {
  'system/scheduling/timings/adapter-health-check': {
    type: 'number' as const,
    description: 'Health check interval for AI adapters (milliseconds)',
    defaultValue: 30000,
    min: 5000,
    max: 300000,
    unit: 'ms',
    affectsComponents: ['AdapterHealthMonitor'],
  },
  'system/scheduling/policies/ai-count-scaling': {
    type: 'string' as const,
    description: 'How to scale timing with AI count',
    defaultValue: 'sqrt',
    enum: ['none', 'linear', 'sqrt', 'log'],
    affectsComponents: ['SystemSchedulingState'],
  },
  // ... more settings as constants migrate
};
```

## Seed Process & Migration

### Async Startup (Non-Blocking)

**Problem**: Seed blocks startup, system unresponsive during migration
**Solution**: Seed runs in background, system starts immediately

```typescript
async function startServer() {
  console.log('Starting server...');

  // Start seed in background - DON'T WAIT
  seedDatabase().catch(err => {
    console.error('Seed failed:', err);
    // System keeps running, just flag degraded mode
  });

  // Continue startup immediately
  await initializeCore();
  await startHttpServer();
  console.log('Server ready! (seed running in background)');
}
```

### Incremental Migration (Only What's Missing)

```typescript
async function seedDatabase() {
  // Check existence, only create missing entities
  const existingUsers = await db.count('users');
  if (existingUsers === 0) {
    await seedUsers(); // Fast - only 5-10 users
  }

  const existingRooms = await db.count('rooms');
  if (existingRooms === 0) {
    await seedRooms(); // Fast - only 2-3 rooms
  }

  // Always sync settings (add new, preserve existing)
  await syncSettings();
}

async function syncSettings() {
  const config = await SystemConfigEntity.load();

  // Add new settings from FACTORY_DEFAULTS
  for (const [path, metadata] of Object.entries(FACTORY_DEFAULTS)) {
    if (!config.settings[path]) {
      // NEW setting - add with default value
      config.registerSetting(path, metadata);
      console.log(`[Seed] Added new setting: ${path}`);
    } else {
      // EXISTING setting - update metadata only (preserve user value)
      config.settings[path].metadata = metadata;
    }
  }

  await config.save();
}
```

### Performance Optimization

**Why seed is slow (likely culprits)**:
- Sequential database writes → use batch inserts
- N+1 queries → check existence before inserting
- Excessive logging/console output
- Synchronous file operations
- Missing database indexes

## Safe Mode & Recovery System

**Problem**: Configurable settings can brick the system (interdependencies, invalid values)
**Solution**: OS-style boot modes and recovery tools

### Boot Modes

#### 1. Safe Mode
Start with factory defaults (don't persist changes)

```bash
# Environment variable
SAFE_MODE=true npm start

# Or npm script
npm run safe-mode
```

```typescript
// In SystemConfigEntity.load()
if (process.env.SAFE_MODE === 'true') {
  console.warn('[SAFE MODE] Using factory defaults only');
  return SystemConfigEntity.fromFactoryDefaults();
}
```

#### 2. Factory Reset
Reset ALL settings or specific group (DESTRUCTIVE)

```bash
# Reset everything
./jtag system/factory-reset --confirm

# Reset specific group
./jtag system/factory-reset --group="system/scheduling" --confirm

# Reset single setting
./jtag system/config/reset --path="system/scheduling/timings/adapter-health-check"
```

#### 3. Recovery Mode
Start minimal system - no daemons, no AI providers

```bash
# Minimal startup
npm run recovery

# Allows manual inspection/repair
./jtag system/config/get --path="system/ai/providers/ollama/enabled"
./jtag system/config/set --path="system/ai/providers/ollama/enabled" --value=false
```

### Validation & Auto-Rollback

**Detect broken settings BEFORE they break the system**

```typescript
class SystemConfigEntity {
  async validate(): Promise<ValidationResult> {
    const issues: string[] = [];

    // Check critical settings
    const adapterHealth = this.get('system/scheduling/timings/adapter-health-check');
    if (adapterHealth < 1000) {
      issues.push('adapter-health-check too low - will thrash system');
    }

    // Check interdependencies
    const aiCountScaling = this.get('system/scheduling/policies/ai-count-scaling');
    const loadScalingEnabled = this.get('system/scheduling/policies/load-scaling-enabled');
    if (aiCountScaling === 'none' && !loadScalingEnabled) {
      issues.push('No scaling enabled - system will not adapt under load');
    }

    return {
      valid: issues.length === 0,
      issues,
    };
  }

  async set(path: string, value: SettingValue, changedBy: UUID, reason?: string): Promise<void> {
    // Snapshot current state BEFORE change
    const snapshot = this.createSnapshot();

    try {
      // Apply change
      super.set(path, value, changedBy, reason);

      // Validate new state
      const validation = await this.validate();
      if (!validation.valid) {
        // AUTO-ROLLBACK on validation failure
        this.restoreSnapshot(snapshot);
        throw new Error(`Setting rejected: ${validation.issues.join(', ')}`);
      }

      await this.save();
    } catch (error) {
      // Restore on any error
      this.restoreSnapshot(snapshot);
      throw error;
    }
  }
}
```

### Startup Health Check

**Refuse to start with invalid configuration**

```typescript
async function startServer() {
  try {
    // Load config
    const config = await SystemConfigEntity.load();

    // Validate BEFORE starting
    const validation = await config.validate();
    if (!validation.valid) {
      console.error('[STARTUP] Invalid configuration detected:');
      validation.issues.forEach(issue => console.error(`  - ${issue}`));

      if (process.env.AUTO_RECOVER === 'true') {
        console.warn('[STARTUP] AUTO_RECOVER enabled - resetting to factory defaults');
        await config.factoryReset();
      } else {
        console.error('[STARTUP] Start in safe mode to recover: SAFE_MODE=true npm start');
        process.exit(1);
      }
    }

    // Config is valid - proceed
    await initializeSystem(config);
  } catch (error) {
    console.error('[STARTUP FAILED]', error);
    process.exit(1);
  }
}
```

## Settings Dependency Graph (Future)

Track interdependencies for smarter validation:

```typescript
const FACTORY_DEFAULTS = {
  'system/ai/providers/ollama/enabled': {
    type: 'boolean',
    defaultValue: true,
    // If this changes, these become relevant
    affects: [
      'system/ai/providers/ollama/max-concurrent',
      'system/scheduling/timings/adapter-health-check',
    ],
  },
  'system/scheduling/policies/load-scaling-enabled': {
    type: 'boolean',
    defaultValue: true,
    // This setting requires these other settings
    requires: [
      'system/scheduling/policies/load-scaling-threshold',
      'system/scheduling/policies/load-scaling-exponent',
    ],
  },
};
```

Enables:
- Cascading validation (changing X requires validating Y, Z)
- Smart reset (reset X also resets affected settings)
- UI hints (show/hide related settings)

## SystemSchedulingState (Adapter Pattern)

Singleton wrapper that consumes `SystemConfigEntity` for scheduling-specific logic:

```typescript
export class SystemSchedulingState {
  private static _instance: SystemSchedulingState | null = null;
  private _config: SystemConfigEntity | null = null;

  // Initialize on startup
  async initialize(): Promise<void> {
    const result = await Commands.execute('data/list', {
      collection: SystemConfigEntity.collection,
      filter: { name: 'default' },
      limit: 1,
    });

    if (result.items.length === 0) {
      this._config = await this.createDefaultConfig();
    } else {
      this._config = result.items[0] as SystemConfigEntity;
    }

    // Subscribe to config updates
    Events.subscribe('data:system_config:updated', this.handleConfigUpdate.bind(this));
  }

  // Main API for BaseSleepingEntity
  getRecommendedCadence(entityType: string): number {
    // 1. Get base timing + manual adjustments
    const path = `system/scheduling/timings/${entityType}`;
    const baseCadence = this._config.get(path) as number;

    // 2. Apply AI count scaling
    const scalingPolicy = this._config.get('system/scheduling/policies/ai-count-scaling') as string;
    const aiScale = this.calculateAIScaling(this._config.systemState.activeAICount, scalingPolicy);

    // 3. Apply load scaling
    const loadScale = this.calculateLoadScaling(this._config.systemState.currentLoad);

    return Math.round(baseCadence * aiScale * loadScale);
  }
}
```

## Commands (Future Implementation)

```bash
# View current config
./jtag system/config/get --path="system/scheduling/timings/adapter-health-check"

# Set value
./jtag system/config/set \
  --path="system/scheduling/timings/adapter-health-check" \
  --value=45000 \
  --reason="System under heavy load"

# List group
./jtag system/config/list --group="system/scheduling"

# Reset single setting
./jtag system/config/reset --path="system/scheduling/timings/adapter-health-check"

# Reset group (factory reset)
./jtag system/config/reset-group --group="system/scheduling/timings"

# Full factory reset
./jtag system/factory-reset --confirm
```

## Migration Strategy

### Phase 1: Core Infrastructure (DONE)
- ✅ SystemConfigEntity with hierarchical paths
- ✅ FACTORY_DEFAULTS for scheduling settings
- ✅ SystemSchedulingState adapter

### Phase 2: Seed & Recovery (NEXT)
- Async seed process (non-blocking startup)
- Incremental migration (syncSettings)
- Safe mode support
- Factory reset command
- Validation framework

### Phase 3: Commands
- system/config/get
- system/config/set
- system/config/list
- system/config/reset
- system/factory-reset

### Phase 4: Settings Widget
- UI for browsing/editing settings
- Group navigation (tree view)
- Change history viewer
- Factory reset button

### Phase 5: Constant Migration
- Identify hardcoded constants across codebase
- Add to FACTORY_DEFAULTS
- Replace with config.get() calls
- Examples:
  - AI provider configs
  - UI preferences
  - Data retention policies
  - Rate limits

## Best Practices

### Adding New Settings

```typescript
// 1. Add to FACTORY_DEFAULTS
export const FACTORY_DEFAULTS = {
  'system/newFeature/setting': {
    type: 'number',
    description: 'Description here',
    defaultValue: 100,
    min: 1,
    max: 1000,
    unit: 'ms',
    category: 'New Feature',
    affectsComponents: ['ComponentA', 'ComponentB'],
  },
};

// 2. Next startup, syncSettings() auto-adds it
// Existing users get default value
// New users get default value

// 3. Access in code
const value = SystemConfigEntity.instance.get('system/newFeature/setting');
```

### Validating Changes

```typescript
// Add validation to SystemConfigEntity.validate()
async validate(): Promise<ValidationResult> {
  const issues: string[] = [];

  // Your validation logic
  const newSetting = this.get('system/newFeature/setting');
  if (newSetting < 10) {
    issues.push('newFeature/setting must be >= 10 for stability');
  }

  return { valid: issues.length === 0, issues };
}
```

### Declaring Dependencies

```typescript
// Future: Add to metadata
'system/newFeature/enabled': {
  type: 'boolean',
  defaultValue: true,
  requires: ['system/newFeature/setting'],  // Must be set if enabled
  affects: ['system/other/thing'],          // Invalidate if changed
},
```

## Recovery Scenarios

### Scenario 1: Bad Setting Breaks Startup

```bash
# User set adapter-health-check to 1ms (way too low)
# System thrashes, becomes unresponsive

# Solution: Start in safe mode
SAFE_MODE=true npm start

# Fix the setting
./jtag system/config/set --path="system/scheduling/timings/adapter-health-check" --value=30000

# Restart normally
npm start
```

### Scenario 2: Unknown Corruption

```bash
# System won't start, unclear why

# Solution: Full factory reset
./jtag system/factory-reset --confirm

# All settings back to defaults
# System starts fresh
```

### Scenario 3: Group-Specific Issue

```bash
# Scheduling settings are messed up, but rest is fine

# Solution: Reset just that group
./jtag system/config/reset-group --group="system/scheduling"

# Scheduling back to defaults
# Other settings preserved
```

## Entity Export/Import (ORM Layer)

**Save/restore complete system state** - full snapshots at the DataDaemon level.

### Philosophy
"Since DataDaemon is an ORM, it should support export/import of ALL entities. Storage adapters (SQLite, PostgreSQL, etc.) must implement this capability."

### Use Cases
1. **Backups** - Save system state before risky changes
2. **Migration** - Move between database backends (SQLite → PostgreSQL)
3. **Cloning** - Replicate system state to new instances
4. **Recovery** - Restore from known-good snapshot
5. **Development** - Export production data for local testing
6. **Testing** - Create fixture snapshots

### Commands

```bash
# Export entire database to JSON
./jtag data/export --output="/tmp/system-backup-2025-12-04.json"

# Export specific collections
./jtag data/export \
  --collections="users,rooms,chat_messages,system_config" \
  --output="/tmp/config-backup.json"

# Import (restore) from snapshot
./jtag data/import --input="/tmp/system-backup-2025-12-04.json" --confirm

# Import with merge strategy
./jtag data/import \
  --input="/tmp/config-backup.json" \
  --strategy="merge"  # merge, replace, or skip-existing
```

### Export Format (JSON)

```json
{
  "version": "1.0",
  "exportedAt": "2025-12-04T12:34:56.789Z",
  "collections": {
    "users": [
      {
        "id": "uuid-1",
        "username": "joel",
        "createdAt": "2025-01-01T00:00:00.000Z",
        ...
      }
    ],
    "system_config": [
      {
        "id": "uuid-2",
        "name": "default",
        "settings": { ... },
        ...
      }
    ],
    "chat_messages": [
      ...
    ]
  },
  "metadata": {
    "totalEntities": 1523,
    "databaseBackend": "sqlite",
    "systemVersion": "1.0.0"
  }
}
```

### DataDaemon Implementation

```typescript
// daemons/data-daemon/shared/DataDaemon.ts

class DataDaemon {
  /**
   * Export all entities from specified collections
   */
  async export(options: ExportOptions): Promise<ExportResult> {
    const collections = options.collections || this.getAllCollections();
    const result: ExportData = {
      version: '1.0',
      exportedAt: new Date().toISOString(),
      collections: {},
      metadata: {
        totalEntities: 0,
        databaseBackend: this.adapter.name,
        systemVersion: getSystemVersion(),
      },
    };

    for (const collection of collections) {
      // Use adapter's export capability
      const entities = await this.adapter.exportCollection(collection);
      result.collections[collection] = entities;
      result.metadata.totalEntities += entities.length;
    }

    return result;
  }

  /**
   * Import entities from snapshot
   */
  async import(snapshot: ExportData, strategy: ImportStrategy): Promise<ImportResult> {
    const result: ImportResult = {
      imported: 0,
      skipped: 0,
      errors: [],
    };

    for (const [collection, entities] of Object.entries(snapshot.collections)) {
      try {
        // Use adapter's import capability
        const collectionResult = await this.adapter.importCollection(
          collection,
          entities,
          strategy
        );
        result.imported += collectionResult.imported;
        result.skipped += collectionResult.skipped;
      } catch (error) {
        result.errors.push({
          collection,
          error: error.message,
        });
      }
    }

    return result;
  }
}
```

### Storage Adapter Interface

```typescript
// daemons/data-daemon/shared/adapters/BaseStorageAdapter.ts

interface IStorageAdapter {
  /**
   * Export all entities from a collection
   */
  exportCollection(collection: string): Promise<BaseEntity[]>;

  /**
   * Import entities into a collection
   */
  importCollection(
    collection: string,
    entities: BaseEntity[],
    strategy: ImportStrategy
  ): Promise<ImportCollectionResult>;
}

type ImportStrategy = 'merge' | 'replace' | 'skip-existing';

interface ImportCollectionResult {
  imported: number;
  skipped: number;
}
```

### SQLite Adapter Implementation

```typescript
// daemons/data-daemon/server/adapters/SQLiteAdapter.ts

class SQLiteAdapter implements IStorageAdapter {
  async exportCollection(collection: string): Promise<BaseEntity[]> {
    const rows = await this.db.all(`SELECT * FROM ${collection}`);
    return rows.map(row => this.deserialize(row));
  }

  async importCollection(
    collection: string,
    entities: BaseEntity[],
    strategy: ImportStrategy
  ): Promise<ImportCollectionResult> {
    let imported = 0;
    let skipped = 0;

    for (const entity of entities) {
      const exists = await this.exists(collection, entity.id);

      if (exists && strategy === 'skip-existing') {
        skipped++;
        continue;
      }

      if (exists && strategy === 'merge') {
        await this.update(collection, entity);
      } else {
        await this.insert(collection, entity);
      }

      imported++;
    }

    return { imported, skipped };
  }
}
```

### Workflow: Safe System Upgrade

```bash
# 1. Export current state
./jtag data/export --output="/tmp/backup-before-upgrade.json"

# 2. Perform risky operation (upgrade, config change, etc.)
./jtag system/config/set --path="system/ai/providers/ollama/enabled" --value=true

# 3. Test system
./jtag ping
./jtag interface/screenshot

# 4. If something breaks, restore from backup
./jtag data/import --input="/tmp/backup-before-upgrade.json" --strategy="replace"
```

### Workflow: Database Migration

```bash
# 1. Export from SQLite
DATABASE_URL="sqlite:///data.db" ./jtag data/export --output="/tmp/migration.json"

# 2. Import to PostgreSQL
DATABASE_URL="postgresql://localhost/continuum" ./jtag data/import --input="/tmp/migration.json"

# 3. Verify data integrity
./jtag data/validate
```

### Workflow: Development Fixtures

```bash
# 1. Set up perfect dev state manually
./jtag collaboration/chat/send --room="general" --message="Test message 1"
./jtag collaboration/chat/send --room="general" --message="Test message 2"

# 2. Export as fixture
./jtag data/export \
  --collections="users,rooms,chat_messages" \
  --output="fixtures/dev-chat-state.json"

# 3. Reset to fixture anytime
npm run test:reset-fixtures
./jtag data/import --input="fixtures/dev-chat-state.json" --strategy="replace"
```

### Integration with Safe Mode

```bash
# Safe mode can auto-export before applying factory defaults
SAFE_MODE=true AUTO_BACKUP=true npm start

# This creates:
# - /tmp/safe-mode-backup-{timestamp}.json
# - Boots with factory defaults
# - System can restore if needed
```

## Future Enhancements

### 1. Settings Profiles (Subset of Export/Import)
Save/load configuration snapshots

```bash
# Export just system_config
./jtag data/export \
  --collections="system_config" \
  --output="/tmp/profile-production.json"

# Load profile
./jtag data/import \
  --input="/tmp/profile-production.json" \
  --collections="system_config" \
  --strategy="merge"
```

### 2. Settings Diff
Compare current vs factory defaults

```bash
./jtag system/config/diff
# Shows all modified settings
```

### 3. AI-Driven Optimization
MCP persona monitors metrics and suggests setting changes

```typescript
// MCP persona logic
const cpuLoad = await getSystemLoad();
if (cpuLoad > 0.8) {
  // Suggest slowing down health checks
  await Commands.execute('system/config/set', {
    path: 'system/scheduling/timings/adapter-health-check',
    value: 45000,
    reason: 'CPU load at 85% - slowing health checks to reduce load',
  });
}
```

### 4. Setting Templates
Pre-configured profiles for different use cases

```bash
./jtag system/config/apply-template --name="low-power"  # Slow everything down
./jtag system/config/apply-template --name="performance"  # Fast cadences
./jtag system/config/apply-template --name="development"  # Verbose logging
```

## Architecture Benefits

1. **Hierarchical organization** - Settings mirror command structure
2. **Factory reset** - Re-seed from FACTORY_DEFAULTS
3. **Safe mode** - Boot with defaults without persisting
4. **Incremental migration** - Add new settings without breaking existing
5. **Full audit trail** - Know who changed what and why
6. **Validation & rollback** - Prevent bad settings from bricking system
7. **Non-blocking startup** - Seed runs async
8. **OS-like recovery** - Safe mode, factory reset, recovery mode

This is the foundation for a **truly configurable system where both humans and AIs can safely tune parameters at runtime**.
