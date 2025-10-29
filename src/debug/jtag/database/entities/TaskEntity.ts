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
import { COLLECTIONS } from '../shared/Collections';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

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

  // Game domain
  | 'make-move'
  | 'analyze-position'

  // Academy domain
  | 'complete-exercise'
  | 'review-material'

  // Self domain (autonomous tasks)
  | 'memory-consolidation'
  | 'skill-audit'
  | 'fine-tune-lora'
  | 'resume-work';

export class TaskEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.TASKS;
  static readonly version = 1;

  // Identity
  id!: UUID;                     // Task ID
  assigneeId!: UUID;             // Who should do this task (PersonaUser ID)
  createdBy!: UUID;              // Who created this task (can be same as assignee for self-tasks)

  // Classification
  domain!: TaskDomain;           // Which domain this task belongs to
  taskType!: TaskType;           // Specific type within domain

  // Context
  contextId!: UUID;              // Domain-specific context (roomId, fileId, gameId, personaId, etc.)
  description!: string;          // Human-readable task description

  // Priority & Scheduling
  priority!: TaskPriority;       // Task priority (0.0-1.0, used by PersonaInbox)
  dueDate?: Date;                // Optional deadline
  estimatedDuration?: number;    // Estimated time to complete (ms)

  // Status
  status!: TaskStatus;           // Current status
  startedAt?: Date;              // When work began
  completedAt?: Date;            // When finished

  // Results
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
  dependsOn?: UUID[];            // Task IDs this task depends on (must complete first)
  blockedBy?: UUID[];            // Tasks blocking this one

  // Metadata
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

  // Timestamps
  createdAt!: Date;
  updatedAt!: Date;

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

  /**
   * Check if task is ready to be worked on (all dependencies met)
   */
  async isReady(): Promise<boolean> {
    if (!this.dependsOn || this.dependsOn.length === 0) {
      return true; // No dependencies
    }

    // Check if all dependencies are completed
    for (const taskId of this.dependsOn) {
      const dependency = await TaskEntity.findById(taskId);
      if (!dependency || dependency.status !== 'completed') {
        return false; // Dependency not met
      }
    }

    return true; // All dependencies met
  }

  /**
   * Mark task as started
   */
  async markStarted(): Promise<void> {
    this.status = 'in_progress';
    this.startedAt = new Date();
    this.updatedAt = new Date();
    await this.save();
  }

  /**
   * Mark task as completed with result
   */
  async markCompleted(result: TaskEntity['result']): Promise<void> {
    this.status = 'completed';
    this.completedAt = new Date();
    this.updatedAt = new Date();
    this.result = result;
    await this.save();
  }

  /**
   * Mark task as failed with error
   */
  async markFailed(error: string): Promise<void> {
    this.status = 'failed';
    this.completedAt = new Date();
    this.updatedAt = new Date();
    this.result = {
      success: false,
      error
    };
    await this.save();
  }

  /**
   * Cancel task
   */
  async cancel(): Promise<void> {
    this.status = 'cancelled';
    this.updatedAt = new Date();
    await this.save();
  }
}
