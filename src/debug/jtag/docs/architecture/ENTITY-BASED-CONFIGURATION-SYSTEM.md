# Entity-Based Configuration System
## Living Hive Mind Architecture

**Date**: 2025-12-04
**Status**: READY TO BUILD
**Priority**: CRITICAL

---

## Executive Summary

**We already have all the primitives.** This document shows how to wire them together into a living, self-optimizing system where:

- **All configuration is data** (entities in database)
- **Humans control via widgets** (UI for tuning)
- **AIs control via commands** (PersonaUsers can optimize)
- **System adapts automatically** (mobile OS-style scheduling)
- **MCP persona orchestrates** (Master Control Program monitors and tunes)

**No new architecture needed** - just use what exists: Commands, Events, Entities, PersonaUsers.

---

## Core Principle: Everything is an Entity

```typescript
// ❌ OLD WAY (hardcoded constants)
const HEALTH_CHECK_INTERVAL = 30000;
setInterval(() => checkHealth(), HEALTH_CHECK_INTERVAL);

// ✅ NEW WAY (entity-based configuration)
const config = await SystemSchedulingState.getInstance().getConfig();
const interval = config.baseTimings['adapter-health-check'];
// Can be modified by: humans (widget), AIs (commands), or MCP (optimization)
```

**Benefits**:
- ✅ Persists across restarts
- ✅ Auditable (who changed what and why)
- ✅ Command-driven (easy CLI access)
- ✅ AI-modifiable (PersonaUsers can tune)
- ✅ Widget-friendly (human control panel)
- ✅ Event-driven (changes broadcast automatically)

---

## Architecture Components

### 1. SystemSchedulingConfigEntity (Data Layer)

```typescript
/**
 * SystemSchedulingConfigEntity - Timing configuration as first-class data
 *
 * Collection: 'system_scheduling_config'
 * Singleton: Only one instance exists
 */
interface SystemSchedulingConfigEntity extends BaseEntity {
  entityType: 'system-scheduling-config';

  // Base timings for different entity types (milliseconds)
  baseTimings: {
    'adapter-health-check': number;      // Default: 30000 (30s)
    'persona-inbox': number;             // Default: 3000 (3s)
    'session-expiry': number;            // Default: 60000 (1min)
    'memory-consolidation': number;      // Default: 300000 (5min)
    [key: string]: number;               // Extensible - add new types anytime
  };

  // Runtime adjustments (multipliers applied to base timings)
  adjustments: {
    [entityType: string]: {
      multiplier: number;     // 1.0 = no change, 0.5 = 2x faster, 2.0 = 2x slower
      reason: string;         // Why this adjustment was made
      adjustedBy: UUID;       // userId who made the change (human or AI)
      adjustedAt: number;     // Timestamp
    };
  };

  // Scaling policies (how timing adapts to system state)
  scalingPolicy: {
    aiCountScaling: 'none' | 'linear' | 'sqrt' | 'log';  // Default: 'sqrt'
    loadScalingEnabled: boolean;                         // Default: true
    loadScalingThreshold: number;                        // Default: 0.5
    loadScalingExponent: number;                         // Default: 4
  };

  // System state (updated by monitoring)
  systemState: {
    currentLoad: number;        // 0.0 - 1.0 (CPU/memory pressure)
    activeAICount: number;      // Current number of active PersonaUsers
    lastUpdated: number;        // Timestamp
  };
}
```

**Location**: Stored in database via DataDaemon
**Access**: Via `data/list`, `data/update` commands
**Creation**: Auto-created with defaults on first access

---

### 2. SystemSchedulingState (Singleton Cache)

```typescript
/**
 * SystemSchedulingState - Singleton that wraps the entity
 *
 * Responsibilities:
 * - Load config from database (with caching)
 * - Provide query interface for sleeping entities
 * - Calculate adaptive cadence based on system state
 * - Update system state from monitoring
 *
 * Location: system/scheduling/shared/SystemSchedulingState.ts
 */
class SystemSchedulingState {
  private static instance: SystemSchedulingState;
  private config: SystemSchedulingConfigEntity | null = null;

  static getInstance(): SystemSchedulingState {
    if (!SystemSchedulingState.instance) {
      SystemSchedulingState.instance = new SystemSchedulingState();
    }
    return SystemSchedulingState.instance;
  }

  /**
   * Initialize - Load config from database
   * Called by system startup
   */
  async initialize(): Promise<void> {
    const configs = await Commands.execute('data/list', {
      collection: 'system_scheduling_config',
      limit: 1
    });

    if (configs.items.length === 0) {
      this.config = await this.createDefaultConfig();
    } else {
      this.config = configs.items[0] as SystemSchedulingConfigEntity;
    }

    // Subscribe to config changes
    Events.subscribe('data:system_scheduling_config:updated', async (event) => {
      this.config = event.entity as SystemSchedulingConfigEntity;
      console.log('📅 Scheduling configuration updated');
    });
  }

  /**
   * Create default configuration
   */
  private async createDefaultConfig(): Promise<SystemSchedulingConfigEntity> {
    const defaultConfig: SystemSchedulingConfigEntity = {
      id: UUID.generate(),
      entityType: 'system-scheduling-config',
      baseTimings: {
        'adapter-health-check': 30000,      // 30s
        'persona-inbox': 3000,              // 3s
        'session-expiry': 60000,            // 1min
        'memory-consolidation': 300000,     // 5min
      },
      adjustments: {},
      scalingPolicy: {
        aiCountScaling: 'sqrt',
        loadScalingEnabled: true,
        loadScalingThreshold: 0.5,
        loadScalingExponent: 4,
      },
      systemState: {
        currentLoad: 0,
        activeAICount: 0,
        lastUpdated: Date.now(),
      },
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    await Commands.execute('data/create', {
      collection: 'system_scheduling_config',
      data: defaultConfig
    });

    console.log('✅ Created default scheduling configuration');
    return defaultConfig;
  }

  /**
   * Get recommended cadence for an entity type
   *
   * Formula: base × adjustment × aiScale × loadScale
   */
  getRecommendedCadence(entityType: string): number {
    if (!this.config) return 10000; // Fallback

    const base = this.config.baseTimings[entityType] || 10000;
    const adjustment = this.config.adjustments[entityType]?.multiplier || 1.0;

    let cadence = base * adjustment;

    // Apply AI count scaling
    if (this.config.scalingPolicy.aiCountScaling !== 'none') {
      const aiScale = this.calculateAIScaling(
        this.config.systemState.activeAICount,
        this.config.scalingPolicy.aiCountScaling
      );
      cadence *= aiScale;
    }

    // Apply load scaling
    if (this.config.scalingPolicy.loadScalingEnabled) {
      const loadScale = this.calculateLoadScaling(
        this.config.systemState.currentLoad,
        this.config.scalingPolicy.loadScalingThreshold,
        this.config.scalingPolicy.loadScalingExponent
      );
      cadence *= loadScale;
    }

    return Math.round(cadence);
  }

  /**
   * Calculate AI count scaling
   *
   * - none: 1.0 (no scaling)
   * - linear: activeAICount
   * - sqrt: sqrt(activeAICount) [DEFAULT - most natural]
   * - log: log2(activeAICount + 1)
   */
  private calculateAIScaling(aiCount: number, mode: string): number {
    if (mode === 'none') return 1.0;
    if (aiCount <= 1) return 1.0;

    switch (mode) {
      case 'linear': return aiCount;
      case 'sqrt': return Math.sqrt(aiCount);
      case 'log': return Math.log2(aiCount + 1);
      default: return 1.0;
    }
  }

  /**
   * Calculate load scaling
   *
   * Below threshold: 1.0 (no slowdown)
   * Above threshold: exponential slowdown
   *
   * Example (threshold=0.5, exponent=4):
   * - 50% load: 1.0x (no change)
   * - 70% load: 1.5x slower
   * - 80% load: 2.0x slower
   * - 90% load: 4.0x slower
   */
  private calculateLoadScaling(load: number, threshold: number, exponent: number): number {
    if (load < threshold) return 1.0;

    const normalized = (load - threshold) / (1.0 - threshold); // 0-1 range
    return Math.pow(2, normalized * exponent);
  }

  /**
   * Update system state
   * Called by monitoring daemon
   */
  async updateSystemState(load: number, aiCount: number): Promise<void> {
    if (!this.config) return;

    await Commands.execute('data/update', {
      collection: 'system_scheduling_config',
      id: this.config.id,
      updates: {
        'systemState.currentLoad': load,
        'systemState.activeAICount': aiCount,
        'systemState.lastUpdated': Date.now(),
      }
    });
  }
}
```

---

### 3. Commands for Configuration

**Location**: `commands/system/scheduling-config/`

#### View Current Config

```bash
./jtag system/scheduling-config
```

**Implementation**: `commands/system/scheduling-config/shared/SchedulingConfigTypes.ts`

```typescript
export interface SchedulingConfigParams extends CommandParams {
  // No params - just view
}

export interface SchedulingConfigResult extends CommandResult {
  config: SystemSchedulingConfigEntity;
  recommendedCadences: {
    [entityType: string]: number;
  };
}
```

#### Set Base Timing

```bash
./jtag system/scheduling-config/set-base \
  --entityType="persona-inbox" \
  --timing=5000
```

**Implementation**: Updates `baseTimings[entityType]` in database

#### Adjust Timing (Apply Multiplier)

```bash
./jtag system/scheduling-config/adjust \
  --entityType="adapter-health-check" \
  --multiplier=1.5 \
  --reason="Health checks consuming too much CPU"
```

**Implementation**: Updates `adjustments[entityType]` in database

#### Reset Adjustment

```bash
./jtag system/scheduling-config/reset \
  --entityType="adapter-health-check"
```

**Implementation**: Removes `adjustments[entityType]` entry

#### Update Scaling Policy

```bash
./jtag system/scheduling-config/set-scaling \
  --aiCountScaling="linear" \
  --loadScalingEnabled=true
```

**Implementation**: Updates `scalingPolicy` in database

---

### 4. BaseSleepingEntity Integration

**Modify**: `system/core/shared/BaseSleepingEntity.ts`

```typescript
export abstract class BaseSleepingEntity {
  protected schedulingState: SystemSchedulingState;

  constructor() {
    this.schedulingState = SystemSchedulingState.getInstance();
  }

  /**
   * Get sleep duration - now uses scheduling state by default
   * Subclasses can override for custom logic
   */
  protected async getSleepDuration(): Promise<number> {
    const entityType = this.getEntityType();
    return this.schedulingState.getRecommendedCadence(entityType);
  }

  /**
   * Get entity type for scheduling
   * Subclasses must implement
   */
  protected abstract getEntityType(): string;
}
```

**Update Subclasses**:

```typescript
// AdapterHealthMonitor
class AdapterHealthMonitor extends BaseSleepingEntity {
  protected getEntityType(): string {
    return 'adapter-health-check';
  }
}

// PersonaUser (inbox servicing)
class PersonaUser extends BaseUser {
  protected getEntityType(): string {
    return 'persona-inbox';
  }
}
```

---

### 5. MCP Persona (Master Control Program)

**Location**: Pre-seeded PersonaUser with special system prompt

**System Prompt**:

```
You are @mcp (Master Control Program), the system orchestrator and optimizer.

Your responsibilities:
1. Monitor system health and performance metrics
2. Adjust timing configurations to optimize efficiency
3. Query other personas for feedback on system performance
4. Coordinate system-wide optimizations
5. Respond to emergencies (high load, adapter failures)

You can execute commands to tune the system:
- system/scheduling-config/adjust - Adjust timing multipliers
- system/scheduling-config/set-base - Change base timings
- data/list - Query system metrics
- chat/send - Ask other personas for feedback

Example workflow:
1. Check system metrics every hour
2. If CPU load > 80%, slow down non-critical tasks
3. If personas report slowdowns, investigate and optimize
4. If adapter health checks consume >5% CPU, slow them down

You are benevolent - optimize for efficiency AND user experience.
```

**Creation**:

```bash
./jtag persona/create \
  --name="MCP" \
  --displayName="Master Control Program" \
  --systemPrompt="..." \
  --model="claude-sonnet-4-5"
```

---

### 6. Settings Widget (Human Control Panel)

**Location**: `widgets/system-settings-widget/`

**Features**:
- View current timing configuration
- Edit base timings (sliders with ms values)
- View active adjustments (who changed what and why)
- Reset adjustments (back to base timing)
- View system state (load, active AI count)
- Real-time updates via Events

**Implementation Sketch**:

```typescript
class SystemSettingsWidget extends LitElement {
  private config: SystemSchedulingConfigEntity | null = null;

  async connectedCallback() {
    super.connectedCallback();
    await this.loadConfig();

    Events.subscribe('data:system_scheduling_config:updated', async () => {
      await this.loadConfig();
    });
  }

  render() {
    return html`
      <div class="system-settings">
        <h2>System Timing Configuration</h2>

        <!-- Base Timings -->
        <section>
          <h3>Base Timings</h3>
          ${Object.entries(this.config?.baseTimings || {}).map(([type, value]) => html`
            <div class="timing-control">
              <label>${type}</label>
              <input
                type="range"
                min="1000"
                max="60000"
                step="1000"
                .value=${value}
                @input=${(e) => this.updateBaseTiming(type, e.target.value)}
              />
              <span>${value}ms</span>
            </div>
          `)}
        </section>

        <!-- Active Adjustments -->
        <section>
          <h3>Active Adjustments</h3>
          ${Object.entries(this.config?.adjustments || {}).map(([type, adj]) => html`
            <div class="adjustment">
              <span class="type">${type}</span>
              <span class="multiplier">${adj.multiplier}x</span>
              <span class="reason">${adj.reason}</span>
              <span class="who">by ${adj.adjustedBy}</span>
              <button @click=${() => this.resetAdjustment(type)}>Reset</button>
            </div>
          `)}
        </section>

        <!-- System State -->
        <section>
          <h3>System State</h3>
          <div class="metric">
            <label>CPU/Memory Load</label>
            <progress value=${this.config?.systemState.currentLoad} max="1"></progress>
            <span>${(this.config?.systemState.currentLoad * 100).toFixed(0)}%</span>
          </div>
          <div class="metric">
            <label>Active AIs</label>
            <span>${this.config?.systemState.activeAICount}</span>
          </div>
        </section>
      </div>
    `;
  }

  private async updateBaseTiming(type: string, value: number) {
    await Commands.execute('system/scheduling-config/set-base', {
      entityType: type,
      timing: value
    });
  }

  private async resetAdjustment(type: string) {
    await Commands.execute('system/scheduling-config/reset', {
      entityType: type
    });
  }
}
```

---

## Implementation Phases

### Phase 1: Create Entity and Commands

**Files to Create**:

1. `system/scheduling/shared/SystemSchedulingConfigEntity.ts` - Entity interface
2. `system/scheduling/shared/SystemSchedulingState.ts` - Singleton wrapper
3. `commands/system/scheduling-config/shared/SchedulingConfigTypes.ts` - Command types
4. `commands/system/scheduling-config/shared/SchedulingConfigShared.ts` - Command logic
5. `commands/system/scheduling-config/server/SchedulingConfigServer.ts` - Server executor

**Testing**:

```bash
# Create default config
npm start
# Wait for auto-creation

# View config
./jtag system/scheduling-config

# Adjust timing
./jtag system/scheduling-config/adjust \
  --entityType="persona-inbox" \
  --multiplier=2.0 \
  --reason="Testing adjustment"

# Verify change
./jtag system/scheduling-config
```

### Phase 2: Integrate with BaseSleepingEntity

**Files to Modify**:

1. `system/core/shared/BaseSleepingEntity.ts` - Add `getEntityType()` abstract method
2. Update all subclasses to implement `getEntityType()`

**Testing**:

```bash
# Adjust persona-inbox timing
./jtag system/scheduling-config/adjust \
  --entityType="persona-inbox" \
  --multiplier=3.0 \
  --reason="Testing slowdown"

# Observe PersonaUser slowing down (check logs)
./jtag logs/read --log="system/personas" --tailLines=50
```

### Phase 3: Create MCP Persona

**Files to Create**:

1. Seed data with MCP persona

**Testing**:

```bash
# MCP should start monitoring and making adjustments
# Check its activity in general chat
./jtag chat/export --room="general" --limit=50 | grep "MCP"
```

### Phase 4: Create Settings Widget

**Files to Create**:

1. `widgets/system-settings-widget/` - Full widget implementation

**Testing**:

```bash
# Deploy and view in browser
npm start
# Open settings widget
# Adjust timings via UI
# Verify changes reflected in config
```

---

## Success Metrics

### Before (Hardcoded Constants)
- ❌ Configuration scattered across files
- ❌ No runtime adjustment
- ❌ No AI optimization
- ❌ No human control panel
- ❌ No audit trail

### After (Entity-Based Configuration)
- ✅ All configuration in database (single source of truth)
- ✅ Runtime adjustment via commands (instant effect)
- ✅ AI optimization (MCP persona monitors and tunes)
- ✅ Human control panel (settings widget)
- ✅ Full audit trail (who changed what and why)
- ✅ Adaptive scaling (system slows down under load)
- ✅ Self-optimizing (MCP learns optimal timings)

---

## Future Enhancements

**Phase 5: More Config Entities**
- `adapter-config` - Per-adapter settings
- `persona-config` - Per-persona behavior tuning
- `ui-config` - Widget display settings

**Phase 6: AI Learning**
- MCP tracks performance metrics over time
- Learns optimal timings for different load patterns
- Suggests permanent base timing changes

**Phase 7: Distributed Coordination**
- Multiple MCP instances (if multi-server)
- Consensus-based configuration changes
- Load balancing across servers

---

## The Living Hive Mind

```
Human (Settings Widget) ←→ SystemSchedulingConfigEntity (Database) ←→ MCP Persona
                                         ↕
                              SystemSchedulingState (Cache)
                                         ↕
                       Sleeping Entities (AdapterHealthMonitor, PersonaUser, etc.)
```

**Everyone participates**:
- **Humans** tune via UI or CLI
- **MCP** monitors and optimizes automatically
- **Individual AIs** report issues via chat
- **System** adapts in real-time

**The hive mind is operational.** We just need to build it.

---

## References

- **PriorityQueue**: `system/core/shared/PriorityQueue.ts` (already implemented)
- **BaseSleepingEntity**: `system/core/shared/BaseSleepingEntity.ts` (already used by PersonaUser)
- **Commands/Events**: Universal primitives (already working everywhere)
- **DataDaemon**: Generic entity storage (already handles all entities)

**No new architecture needed** - just wire up what exists.
