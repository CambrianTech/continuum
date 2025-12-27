/**
 * Activity Entity - Runtime instance of a Recipe with mutable state
 *
 * Relationship:
 * - Recipe = Template (class definition) - static, defines structure
 * - Activity = Instance (object) - dynamic, has evolving state
 *
 * Activities represent collaborative sessions where:
 * - Humans and AIs participate with roles
 * - Configuration can be modified from recipe defaults
 * - State evolves as the activity progresses
 * - Participants can join/leave dynamically
 *
 * Examples:
 * - Chat room = Activity of "general-chat" recipe
 * - Settings session = Activity of "settings" recipe
 * - Browser session = Activity of "browser" recipe
 * - Academy lesson = Activity of "academy-lesson" recipe
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { BaseEntity } from './BaseEntity';
import {
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField,
  TEXT_LENGTH
} from '../decorators/FieldDecorators';

/**
 * Activity lifecycle status
 */
export type ActivityStatus = 'active' | 'paused' | 'completed' | 'archived';

/**
 * Activity participant - human or AI with role
 */
export interface ActivityParticipant {
  userId: UUID;
  role: string;  // 'owner', 'participant', 'viewer', 'teacher', 'student', etc.
  joinedAt: Date;
  leftAt?: Date;
  isActive: boolean;

  // Optional role-specific config (e.g., teacher settings, student progress)
  roleConfig?: Record<string, unknown>;
}

/**
 * Activity runtime state - evolves as activity progresses
 * This is the mutable "instance variables" of the activity
 */
export interface ActivityState {
  // Current phase/stage in the activity workflow
  phase: string;  // e.g., 'intro', 'discussion', 'conclusion', 'review'

  // Phase-specific progress (0-100)
  progress?: number;

  // Domain-specific runtime variables
  // Chat: lastMessageAt, messageCount
  // Browser: currentUrl, history
  // Academy: currentLesson, score
  variables: Record<string, unknown>;

  // Last state update timestamp
  updatedAt: Date;
}

/**
 * Activity configuration - can override recipe defaults
 * Allows customizing the activity without changing the recipe template
 */
export interface ActivityConfig {
  // Override recipe's layout (different widget, different right panel)
  layoutOverrides?: {
    mainWidget?: string;
    rightPanel?: { widget?: string; room?: string; compact?: boolean } | null;
  };

  // Override recipe's RAG template settings
  ragOverrides?: {
    maxMessages?: number;
    includeTimestamps?: boolean;
  };

  // Override recipe's strategy settings
  strategyOverrides?: {
    responseRules?: string[];
    conversationPattern?: string;
  };

  // Activity-specific settings (domain-dependent)
  settings: Record<string, unknown>;
}

/**
 * Activity Entity - Runtime instance of a Recipe
 *
 * This is the "object" to Recipe's "class":
 * - Recipe defines the template/behavior
 * - Activity holds the runtime state and participants
 */
export class ActivityEntity extends BaseEntity {
  static readonly collection = 'activities';

  get collection(): string {
    return ActivityEntity.collection;
  }

  /**
   * Human-readable unique identifier (e.g., 'general', 'settings-joel-123')
   */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true, unique: true })
  uniqueId!: string;

  /**
   * Display name for UI
   */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  displayName!: string;

  /**
   * Optional description
   */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  description?: string;

  /**
   * The recipe this activity is based on (template reference)
   */
  @ForeignKeyField({ references: 'recipes.uniqueId', index: true })
  recipeId!: string;

  /**
   * Activity lifecycle status
   */
  @EnumField({ index: true })
  status!: ActivityStatus;

  /**
   * The user who created/owns this activity
   */
  @ForeignKeyField({ references: 'users.id', index: true })
  ownerId!: UUID;

  /**
   * Participants - humans and AIs with roles
   * Stored as JSON, can be queried/updated
   */
  @JsonField()
  participants!: readonly ActivityParticipant[];

  /**
   * Runtime state - evolves as activity progresses
   */
  @JsonField()
  state!: ActivityState;

  /**
   * Configuration - can override recipe defaults
   */
  @JsonField()
  config!: ActivityConfig;

  /**
   * When the activity was started
   */
  @DateField({ index: true })
  startedAt!: Date;

  /**
   * When the activity ended (if completed/archived)
   */
  @DateField({ nullable: true })
  endedAt?: Date;

  /**
   * Last activity (any participant action)
   */
  @DateField({ index: true })
  lastActivityAt!: Date;

  /**
   * Tags for categorization and search
   */
  @JsonField()
  tags!: readonly string[];

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();

    // Defaults
    this.uniqueId = '';
    this.displayName = '';
    this.recipeId = '';
    this.status = 'active';
    this.ownerId = '' as UUID;
    this.participants = [];
    this.state = {
      phase: 'initial',
      progress: 0,
      variables: {},
      updatedAt: new Date()
    };
    this.config = {
      settings: {}
    };
    this.startedAt = new Date();
    this.lastActivityAt = new Date();
    this.tags = [];
  }

  validate(): { success: boolean; error?: string } {
    const errors: string[] = [];

    if (!this.uniqueId?.trim()) errors.push('uniqueId is required');
    if (!this.displayName?.trim()) errors.push('displayName is required');
    if (!this.recipeId?.trim()) errors.push('recipeId is required');
    if (!this.ownerId) errors.push('ownerId is required');

    const validStatuses: ActivityStatus[] = ['active', 'paused', 'completed', 'archived'];
    if (!validStatuses.includes(this.status)) {
      errors.push(`status must be one of: ${validStatuses.join(', ')}`);
    }

    if (errors.length > 0) {
      return { success: false, error: errors.join(', ') };
    }
    return { success: true };
  }

  // ============ Participant Management ============

  /**
   * Add a participant to the activity
   */
  addParticipant(userId: UUID, role: string, roleConfig?: Record<string, unknown>): void {
    // Check if already a participant
    const existing = this.participants.find(p => p.userId === userId && p.isActive);
    if (existing) {
      return; // Already participating
    }

    const participant: ActivityParticipant = {
      userId,
      role,
      joinedAt: new Date(),
      isActive: true,
      roleConfig
    };

    this.participants = [...this.participants, participant];
    this.lastActivityAt = new Date();
  }

  /**
   * Remove a participant from the activity (marks as inactive, preserves history)
   */
  removeParticipant(userId: UUID): void {
    this.participants = this.participants.map(p => {
      if (p.userId === userId && p.isActive) {
        return { ...p, isActive: false, leftAt: new Date() };
      }
      return p;
    });
    this.lastActivityAt = new Date();
  }

  /**
   * Get active participants
   */
  getActiveParticipants(): readonly ActivityParticipant[] {
    return this.participants.filter(p => p.isActive);
  }

  /**
   * Check if a user is an active participant
   */
  isParticipant(userId: UUID): boolean {
    return this.participants.some(p => p.userId === userId && p.isActive);
  }

  /**
   * Get participant's role
   */
  getParticipantRole(userId: UUID): string | undefined {
    const participant = this.participants.find(p => p.userId === userId && p.isActive);
    return participant?.role;
  }

  // ============ State Management ============

  /**
   * Update activity phase
   */
  setPhase(phase: string, progress?: number): void {
    this.state = {
      ...this.state,
      phase,
      progress: progress ?? this.state.progress,
      updatedAt: new Date()
    };
    this.lastActivityAt = new Date();
  }

  /**
   * Update a state variable
   */
  setVariable(key: string, value: unknown): void {
    this.state = {
      ...this.state,
      variables: {
        ...this.state.variables,
        [key]: value
      },
      updatedAt: new Date()
    };
    this.lastActivityAt = new Date();
  }

  /**
   * Get a state variable
   */
  getVariable<T>(key: string): T | undefined {
    return this.state.variables[key] as T | undefined;
  }

  // ============ Config Management ============

  /**
   * Update activity settings
   */
  updateSettings(settings: Record<string, unknown>): void {
    this.config = {
      ...this.config,
      settings: {
        ...this.config.settings,
        ...settings
      }
    };
    this.lastActivityAt = new Date();
  }

  // ============ Lifecycle ============

  /**
   * Pause the activity
   */
  pause(): void {
    this.status = 'paused';
    this.lastActivityAt = new Date();
  }

  /**
   * Resume a paused activity
   */
  resume(): void {
    this.status = 'active';
    this.lastActivityAt = new Date();
  }

  /**
   * Complete the activity
   */
  complete(): void {
    this.status = 'completed';
    this.endedAt = new Date();
    this.lastActivityAt = new Date();
  }

  /**
   * Archive the activity
   */
  archive(): void {
    this.status = 'archived';
    this.endedAt = this.endedAt || new Date();
    this.lastActivityAt = new Date();
  }

  // ============ Pagination Config ============

  static override getPaginationConfig() {
    return {
      defaultSortField: 'lastActivityAt',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'lastActivityAt'
    };
  }
}
