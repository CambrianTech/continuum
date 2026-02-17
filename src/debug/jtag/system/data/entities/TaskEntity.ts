/**
 * TaskEntity - Universal task representation for autonomous PersonaUser work
 *
 * Tasks can be:
 * - External (from users, chat messages, code sessions)
 * - Self-created (memory review, learning, skill audit)
 *
 * Domain-agnostic: Works for chat, code, game, academy, etc.
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../config/DatabaseConfig';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, EnumField, DateField, JsonField, NumberField } from '../decorators/FieldDecorators';

/**
 * Task status lifecycle
 */
export type TaskStatus =
  | 'pending'      // Created, not yet started
  | 'in_progress'  // Currently being worked on
  | 'completed'    // Successfully finished
  | 'failed'       // Failed with error
  | 'cancelled';   // Cancelled before completion

/**
 * Task priority (0.0 - 1.0)
 * Higher = more urgent
 */
export type TaskPriority = number; // 0.0 (lowest) to 1.0 (highest)

/**
 * Domain the task belongs to
 */
export type TaskDomain =
  | 'chat'          // Chat message processing
  | 'code'          // Code editing, review, debugging
  | 'game'          // Game playing, strategy
  | 'academy'       // Training, learning
  | 'analysis'      // Data analysis, research
  | 'canvas'        // Visual/drawing activities (collaborative canvas)
  | 'browser'       // Web browsing co-pilot
  | 'sentinel'      // Sentinel lifecycle events (escalation, completion, approval)
  | 'self';         // Self-improvement tasks (memory, learning, audit)

/**
 * Task type (domain-specific)
 */
export type TaskType =
  // Chat domain
  | 'respond-to-message'
  | 'moderate-chat'

  // Code domain
  | 'review-code'
  | 'debug-issue'
  | 'write-feature'
  | 'fix-error'          // Fix a build/compilation error
  | 'shell-complete'     // Review shell command output
  | 'shell-started'      // Track started async shell execution

  // Game domain
  | 'make-move'
  | 'analyze-position'

  // Academy domain
  | 'complete-exercise'
  | 'review-material'

  // Canvas domain (visual collaboration)
  | 'observe-canvas'      // Watch and understand canvas activity
  | 'draw-on-canvas'      // Contribute to collaborative drawing
  | 'describe-canvas'     // Generate description of canvas content

  // Browser domain (co-browsing)
  | 'observe-page'        // Watch and understand page content
  | 'assist-navigation'   // Help with web browsing

  // Self domain (autonomous tasks)
  | 'memory-consolidation'
  | 'skill-audit'
  | 'fine-tune-lora'
  | 'resume-work'

  // Sentinel domain (sentinel lifecycle events → persona inbox)
  | 'sentinel-complete'      // Sentinel finished successfully
  | 'sentinel-failed'        // Sentinel failed with error
  | 'sentinel-escalation'    // Sentinel needs human/persona attention
  | 'sentinel-approval';     // Sentinel paused, awaiting approval

export class TaskEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.TASKS;

  // Identity
  @TextField({ index: true })
  assigneeId!: UUID;             // Who should do this task (PersonaUser ID)

  @TextField({ index: true })
  createdBy!: UUID;              // Who created this task (can be same as assignee for self-tasks)

  // Classification
  @EnumField({ index: true })
  domain!: TaskDomain;           // Which domain this task belongs to

  @EnumField({ index: true })
  taskType!: TaskType;           // Specific type within domain

  // Context
  @TextField({ index: true })
  contextId!: UUID;              // Domain-specific context (roomId, fileId, gameId, personaId, etc.)

  @TextField()
  description!: string;          // Human-readable task description

  // Priority & Scheduling
  @NumberField()
  priority!: TaskPriority;       // Task priority (0.0-1.0, used by PersonaInbox)

  @DateField({ nullable: true })
  dueDate?: Date;                // Optional deadline

  @NumberField({ nullable: true })
  estimatedDuration?: number;    // Estimated time to complete (ms)

  // Status
  @EnumField({ index: true })
  status!: TaskStatus;           // Current status

  @DateField({ nullable: true })
  startedAt?: Date;              // When work began

  @DateField({ nullable: true })
  completedAt?: Date;            // When finished

  // Results
  @JsonField({ nullable: true })
  result?: {                     // Task result (structure depends on taskType)
    success: boolean;
    output?: unknown;            // Domain-specific output
    error?: string;              // Error message if failed
    metrics?: {                  // Performance metrics
      tokensUsed?: number;
      latencyMs?: number;
      confidence?: number;
    };
  };

  // Dependencies
  @JsonField({ nullable: true })
  dependsOn?: UUID[];            // Task IDs this task depends on (must complete first)

  @JsonField({ nullable: true })
  blockedBy?: UUID[];            // Tasks blocking this one

  // Metadata
  @JsonField({ nullable: true })
  metadata?: {                   // Domain-specific metadata
    // Chat domain
    messageId?: UUID;
    roomId?: UUID;

    // Code domain
    fileId?: UUID;
    pullRequestId?: UUID;

    // Game domain
    gameId?: UUID;
    moveNotation?: string;

    // Academy domain
    exerciseId?: UUID;
    skillName?: string;

    // Self domain (fine-tuning)
    loraLayer?: string;          // Which LoRA adapter to train
    trainingData?: unknown[];    // Training examples
  };

  constructor() {
    super(); // Initialize BaseEntity fields (id, createdAt, updatedAt, version)

    // Set defaults
    this.status = 'pending';
    this.priority = 0.5;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return TaskEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate task data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.assigneeId) {
      return { success: false, error: 'Task assigneeId is required' };
    }
    if (!this.createdBy) {
      return { success: false, error: 'Task createdBy is required' };
    }
    if (!this.domain) {
      return { success: false, error: 'Task domain is required' };
    }
    if (!this.taskType) {
      return { success: false, error: 'Task taskType is required' };
    }
    if (!this.contextId) {
      return { success: false, error: 'Task contextId is required' };
    }
    if (!this.description?.trim()) {
      return { success: false, error: 'Task description is required' };
    }

    // Validate priority range
    if (this.priority < 0 || this.priority > 1) {
      return { success: false, error: 'Task priority must be between 0.0 and 1.0' };
    }

    // Validate status enum
    const validStatuses: TaskStatus[] = ['pending', 'in_progress', 'completed', 'failed', 'cancelled'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Task status must be one of: ${validStatuses.join(', ')}` };
    }

    return { success: true };
  }

  /**
   * Calculate message priority for inbox (used by PersonaInbox.enqueue())
   * Based on task priority, domain, type, and due date
   */
  calculateInboxPriority(): number {
    let priority = this.priority;

    // Boost priority based on due date urgency
    if (this.dueDate) {
      const timeUntilDue = this.dueDate.getTime() - Date.now();
      const hoursUntilDue = timeUntilDue / (1000 * 60 * 60);

      if (hoursUntilDue < 1) {
        priority += 0.3; // Very urgent
      } else if (hoursUntilDue < 24) {
        priority += 0.2; // Urgent
      } else if (hoursUntilDue < 168) {
        priority += 0.1; // Due this week
      }
    }

    // Boost priority for self-improvement tasks (long-term investment)
    if (this.domain === 'self') {
      priority += 0.15;
    }

    // Cap at 1.0
    return Math.min(priority, 1.0);
  }

}
