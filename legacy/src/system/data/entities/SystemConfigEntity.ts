/**
 * SystemConfigEntity - Hierarchical System Configuration
 *
 * Single source of truth for ALL system configuration.
 * Organized hierarchically like commands: groups/subgroups/settings
 *
 * Philosophy: Everything is a Setting
 * - Replace scattered constants across codebase
 * - Enable runtime tuning by humans AND AIs
 * - Factory reset = re-seed specific groups
 * - Full audit trail (who changed what, when, why)
 *
 * Examples:
 * - system/scheduling/timings/adapter-health-check = 30000
 * - system/scheduling/policies/ai-count-scaling = 'sqrt'
 * - system/ai/providers/candle/enabled = true
 * - system/ai/providers/candle/max-concurrent = 4
 * - system/ui/theme/dark-mode = true
 * - system/ui/chat/max-history = 100
 *
 * Commands:
 * - ./jtag system/config/get --path="system/scheduling/timings/adapter-health-check"
 * - ./jtag system/config/set --path="system/scheduling/timings/adapter-health-check" --value=45000 --reason="System under heavy load"
 * - ./jtag system/config/list --group="system/scheduling"
 * - ./jtag system/config/reset --group="system/scheduling/timings"  # Factory reset just this group
 * - ./jtag system/config/reset --all  # Factory reset EVERYTHING (re-seed all)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, JsonField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * Setting value types
 */
export type SettingValue = string | number | boolean | object | null;

/**
 * Setting change record - full audit trail
 */
export interface SettingChange {
  value: SettingValue;
  changedBy: UUID;         // userId (human or AI)
  changedAt: number;       // Timestamp
  reason?: string;         // Optional explanation
}

/**
 * Setting metadata - defines behavior and constraints
 */
export interface SettingMetadata {
  type: 'string' | 'number' | 'boolean' | 'object';
  description: string;
  defaultValue: SettingValue;

  // Validation
  min?: number;           // For numbers
  max?: number;           // For numbers
  enum?: string[];        // For strings (allowed values)
  required?: boolean;     // Cannot be null

  // UI hints
  category?: string;      // For grouping in settings panels
  displayName?: string;   // Human-readable name
  unit?: string;          // "ms", "bytes", "%", etc.

  // Runtime behavior
  requiresRestart?: boolean;  // Does changing this require system restart?
  affectsComponents?: string[]; // Which components use this setting?
}

/**
 * Hierarchical setting path structure
 * Examples:
 * - "system/scheduling/timings/adapter-health-check"
 * - "system/ai/providers/candle/enabled"
 */
export interface SettingNode {
  path: string;                    // Full path (e.g., "system/scheduling/timings/adapter-health-check")
  value: SettingValue;             // Current value
  metadata: SettingMetadata;       // Type info, constraints, description
  history: SettingChange[];        // Change history (last 10 changes)
}

/**
 * SystemConfigEntity - Hierarchical configuration store
 *
 * IMPORTANT: There should only be ONE instance of this entity in the database.
 * - First startup: Auto-creates with factory defaults
 * - Subsequent starts: Loads existing config
 * - Updates: Modify in place (never create new instances)
 * - Factory reset: Re-seed specific groups or entire config
 */
export class SystemConfigEntity extends BaseEntity {
  static readonly collection = 'system_config';

  @TextField({ index: true })
  name!: string; // Always 'default' - ensures singleton pattern

  // Hierarchical settings tree
  // Key = full path (e.g., "system/scheduling/timings/adapter-health-check")
  // Value = setting node with current value, metadata, history
  @JsonField()
  settings!: {
    [path: string]: SettingNode;
  };

  // System state (runtime values, not settings)
  @JsonField()
  systemState!: {
    currentLoad: number;      // 0.0 - 1.0
    activeAICount: number;
    lastUpdated: number;
  };

  constructor() {
    super();
    this.name = 'default'; // Singleton pattern
    this.settings = {};
    this.systemState = {
      currentLoad: 0.0,
      activeAICount: 1,
      lastUpdated: Date.now(),
    };
  }

  /**
   * Implement BaseEntity abstract methods
   */
  get collection(): string {
    return SystemConfigEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.name) {
      return { success: false, error: 'Name is required' };
    }

    if (this.name !== 'default') {
      return { success: false, error: 'Only "default" config entity is allowed (singleton pattern)' };
    }

    if (!this.settings || typeof this.settings !== 'object') {
      return { success: false, error: 'Settings must be an object' };
    }

    if (!this.systemState || typeof this.systemState !== 'object') {
      return { success: false, error: 'SystemState must be an object' };
    }

    return { success: true };
  }

  /**
   * Get setting value by path
   * Returns undefined if path doesn't exist
   */
  get(path: string): SettingValue | undefined {
    return this.settings[path]?.value;
  }

  /**
   * Set setting value by path
   * Records change in history
   */
  set(path: string, value: SettingValue, changedBy: UUID, reason?: string): void {
    const node = this.settings[path];
    if (!node) {
      throw new Error(`Setting path '${path}' does not exist`);
    }

    // Validate value against metadata
    this.validateValue(node.metadata, value);

    // Record change in history (keep last 10)
    const change: SettingChange = {
      value,
      changedBy,
      changedAt: Date.now(),
      reason,
    };

    node.history.push(change);
    if (node.history.length > 10) {
      node.history.shift(); // Remove oldest
    }

    // Update value
    node.value = value;
  }

  /**
   * Get all settings under a group path
   * Example: getGroup('system/scheduling') returns all scheduling settings
   */
  getGroup(groupPath: string): { [path: string]: SettingNode } {
    const result: { [path: string]: SettingNode } = {};
    for (const [path, node] of Object.entries(this.settings)) {
      if (path.startsWith(groupPath + '/') || path === groupPath) {
        result[path] = node;
      }
    }
    return result;
  }

  /**
   * Reset setting to factory default
   */
  reset(path: string, changedBy: UUID): void {
    const node = this.settings[path];
    if (!node) {
      throw new Error(`Setting path '${path}' does not exist`);
    }

    this.set(path, node.metadata.defaultValue, changedBy, 'Factory reset');
  }

  /**
   * Reset entire group to factory defaults
   */
  resetGroup(groupPath: string, changedBy: UUID): void {
    const group = this.getGroup(groupPath);
    for (const path of Object.keys(group)) {
      this.reset(path, changedBy);
    }
  }

  /**
   * Validate value against metadata constraints
   */
  private validateValue(metadata: SettingMetadata, value: SettingValue): void {
    // Type check
    const actualType = typeof value;
    if (actualType === 'object' && value !== null && !Array.isArray(value)) {
      // Allow objects
      if (metadata.type !== 'object') {
        throw new Error(`Type mismatch: expected ${metadata.type}, got object`);
      }
    } else if (actualType !== metadata.type) {
      throw new Error(`Type mismatch: expected ${metadata.type}, got ${actualType}`);
    }

    // Required check
    if (metadata.required && value === null) {
      throw new Error('Value is required');
    }

    // Number constraints
    if (metadata.type === 'number' && typeof value === 'number') {
      if (metadata.min !== undefined && value < metadata.min) {
        throw new Error(`Value ${value} is below minimum ${metadata.min}`);
      }
      if (metadata.max !== undefined && value > metadata.max) {
        throw new Error(`Value ${value} is above maximum ${metadata.max}`);
      }
    }

    // Enum constraint
    if (metadata.enum && typeof value === 'string') {
      if (!metadata.enum.includes(value)) {
        throw new Error(`Value '${value}' is not in allowed values: ${metadata.enum.join(', ')}`);
      }
    }
  }

  /**
   * Register a new setting (used during initialization/seeding)
   */
  registerSetting(path: string, metadata: SettingMetadata, initialValue?: SettingValue): void {
    if (this.settings[path]) {
      return; // Already exists
    }

    const value = initialValue !== undefined ? initialValue : metadata.defaultValue;
    this.validateValue(metadata, value);

    this.settings[path] = {
      path,
      value,
      metadata,
      history: [],
    };
  }
}

/**
 * Factory defaults for system configuration
 * This is the "seed" data for factory reset
 */
export const FACTORY_DEFAULTS = {
  // system/scheduling/timings/*
  'system/scheduling/timings/adapter-health-check': {
    type: 'number' as const,
    description: 'Health check interval for AI adapters (milliseconds)',
    defaultValue: 30000,
    min: 5000,
    max: 300000,
    unit: 'ms',
    category: 'Scheduling',
    displayName: 'Adapter Health Check Interval',
    affectsComponents: ['AdapterHealthMonitor'],
  },
  'system/scheduling/timings/persona-inbox': {
    type: 'number' as const,
    description: 'PersonaUser inbox polling interval (milliseconds)',
    defaultValue: 3000,
    min: 1000,
    max: 30000,
    unit: 'ms',
    category: 'Scheduling',
    displayName: 'Persona Inbox Interval',
    affectsComponents: ['PersonaUser'],
  },
  'system/scheduling/timings/session-expiry': {
    type: 'number' as const,
    description: 'Session expiry check interval (milliseconds)',
    defaultValue: 60000,
    min: 10000,
    max: 600000,
    unit: 'ms',
    category: 'Scheduling',
    displayName: 'Session Expiry Check Interval',
    affectsComponents: ['SessionDaemon'],
  },
  'system/scheduling/timings/memory-consolidation': {
    type: 'number' as const,
    description: 'Memory consolidation interval (milliseconds)',
    defaultValue: 300000,
    min: 60000,
    max: 3600000,
    unit: 'ms',
    category: 'Scheduling',
    displayName: 'Memory Consolidation Interval',
    affectsComponents: ['PersonaUser'],
  },

  // system/scheduling/policies/*
  'system/scheduling/policies/ai-count-scaling': {
    type: 'string' as const,
    description: 'How to scale timing with AI count',
    defaultValue: 'sqrt',
    enum: ['none', 'linear', 'sqrt', 'log'],
    category: 'Scheduling',
    displayName: 'AI Count Scaling Policy',
    affectsComponents: ['SystemSchedulingState'],
  },
  'system/scheduling/policies/load-scaling-enabled': {
    type: 'boolean' as const,
    description: 'Enable load-based adaptive scaling',
    defaultValue: true,
    category: 'Scheduling',
    displayName: 'Load-Based Scaling',
    affectsComponents: ['SystemSchedulingState'],
  },
  'system/scheduling/policies/load-scaling-threshold': {
    type: 'number' as const,
    description: 'Load threshold to trigger scaling (0.0-1.0)',
    defaultValue: 0.5,
    min: 0.0,
    max: 1.0,
    category: 'Scheduling',
    displayName: 'Load Scaling Threshold',
    affectsComponents: ['SystemSchedulingState'],
  },
  'system/scheduling/policies/load-scaling-exponent': {
    type: 'number' as const,
    description: 'Exponential factor for load scaling',
    defaultValue: 4,
    min: 1,
    max: 10,
    category: 'Scheduling',
    displayName: 'Load Scaling Exponent',
    affectsComponents: ['SystemSchedulingState'],
  },
  'system/scheduling/policies/adapter-max-consecutive-failures': {
    type: 'number' as const,
    description: 'Max consecutive health check failures before restart',
    defaultValue: 3,
    min: 1,
    max: 10,
    category: 'Scheduling',
    displayName: 'Adapter Max Failures',
    affectsComponents: ['AdapterHealthMonitor'],
  },

  // system/adapters/*
  'system/adapters/sentinel/startup-timeout': {
    type: 'number' as const,
    description: 'Sentinel server startup timeout (milliseconds)',
    defaultValue: 30000,
    min: 5000,
    max: 120000,
    unit: 'ms',
    category: 'Adapters',
    displayName: 'Sentinel Startup Timeout',
    affectsComponents: ['SentinelAdapter'],
  },
  'system/adapters/sentinel/restart-stabilization-delay': {
    type: 'number' as const,
    description: 'Delay after Sentinel restart before health check (milliseconds)',
    defaultValue: 3000,
    min: 1000,
    max: 10000,
    unit: 'ms',
    category: 'Adapters',
    displayName: 'Sentinel Restart Delay',
    affectsComponents: ['SentinelAdapter'],
  },

  // Add more settings as we migrate constants...
  // system/ai/providers/*
  // system/ui/*
  // system/data/*
  // etc.
};
